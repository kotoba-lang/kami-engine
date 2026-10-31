#!/usr/bin/env python3
"""Build or check the kami migration ledger (JUN-641).

The ledger (docs/migration-ledger.json) classifies every tracked file of this
repository for the move to canonical .kotoba + Amu selfhost, lists each unit's
namespace dependencies and external libraries, and derives the port order
(waves) from the dependency DAG.

    python3 scripts/migration_ledger.py --kotoba ../kotoba --kotoba-rev REV
        regenerate docs/migration-ledger.json
    python3 scripts/migration_ledger.py --check
        fail unless the committed ledger lists exactly the tracked files

The Amu blockers (class e) are not guessed: BLOCKERS below records what the
pinned Amu CLI refused when the probe sources in
docs/evidence/amu-capability-probe-20261101d.json were run against it. The
atom blockers (JUN-731) and the keyword map blockers (kwmap/*, JUN-732) are
split by the port they need: the rewrite rules are docs/port-rewrite-rules.md,
the atom ones measured in docs/evidence/port-rewrite-rules-20261101.json.
"""

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs" / "migration-ledger.json"
PLACEMENT = ROOT / "docs" / "migration-placement.json"  # JUN-649, hand-written decisions
SCHEMA = "kami.migration-ledger.v1"

CLASSES = {
    "a": "data kept as the source of truth (EDN, WGSL, WIT, URDF, YAML, fixtures, assets)",
    "b": "pure logic to port to .kotoba (used only as a port target of an o file)",
    "c": "host or browser glue: Amu browser Wasm plus kotoba-client-runtime",
    "d": "remove from CI and execution paths (kbb/nbb manifests and runners, Rust-era scripts, built output)",
    "o": "CLJK source kept as the reference oracle; `port` says where its .kotoba twin goes",
    "keep": "documentation, licences, provenance records, bot profiles and repository metadata, unchanged by the migration",
    "native": "already native: .kotoba sources and the Amu gate runner and CI definition",
}

# Measured with the pinned Amu macOS CLI (kotoba-lang/amu 11df09b1f60e, the
# one CI uses) on 2026-11-01 (JUN-730):
# docs/evidence/amu-capability-probe-20261101d.json. Against the previous pin
# 05668c77134a (amu-capability-probe-20261101c.json) one earlier probe changed:
# amu PR #1754 (JUN-730) expands a closed same-module defmulti and its
# defmethods to one defn with a static case, so `defmulti` (a defmulti with no
# defmethod) is E2006 instead of E2003. 42 new probes record the admitted shape
# (keyword or module-defn dispatch, identical plain parameter vectors, integer /
# keyword keys) and what still refuses: E2006 / E2001 / E2125 / E2101 / E2104 /
# E2103 shapes, E2007 / E6010 open multimethods, E2003 protocols, E2181 reify.
# Every other probe refuses or runs as before. `detect` is a regex over a CLJK
# file with its comments and string contents removed; the kwmap/* blockers are
# read by kwmap_patterns(), destructuring by destructuring_patterns() and the
# dispatch/* blockers by dispatch_patterns() instead.
# Earlier: 05668c77134a (JUN-729, JUN-734, 20261101c), c2186d762e20 (JUN-728,
# 20261101 / 20261101b), 3c9eb7f7f74e (20261031b).
BLOCKERS = {
    "multi-module": {
        "refusal": "E6002 required module ... is missing from the explicit source paths",
        "what": "a source that requires another namespace; the CLI compiles one self-contained file",
        "issues": ["AWAI-50", "JUN-384"],
    },
    # JUN-732 split the former single keyword-map-literal blocker (391 files at
    # 25c3cc6, any `{:`) by the representation a port needs; the rewrites are
    # the "Keyword maps" section of docs/port-rewrite-rules.md. They are read by
    # kwmap_patterns() over files that contain a keyword map literal; a keyword
    # map that is only a local fixed-shape record, a constant table read by a
    # literal key, or ex-info data is admitted and is no blocker.
    # JUN-734 (D5, amu 05668c77) admits def literals holding f64 / bool values,
    # vectors of maps, f64 / bool / keyword vectors and vectors of vectors, so
    # this blocker shrank from every such def (109 files at c2186d7) to the def
    # literals that still fail to type: a vector whose literal items differ in
    # type (maps with different key sets or value types, mixed kinds) or that
    # holds more than 64 non-integer items. An item that is a symbol, a call or
    # nil has a type only type flow can tell; it is read as matching anything
    # (nil fields are kwmap/nil-field).
    "kwmap/def-literal": {
        "refusal": "E2116 vector literal items must be i64 (at most 64)",
        "what": "a top-level def of a keyword map (or a vector of them) holding a vector whose items differ in type "
                "(maps with different keys, mixed kinds) or that holds more than 64 non-integer items; port the items "
                "as one defrecord with [:option T] fields built by ->Rec, or a typed map",
        "issues": ["JUN-734"],
    },
    "kwmap/table-runtime-key": {
        "refusal": "E2127 lookup of an absent key; E2104 contains? / get with a default on a record",
        "what": "a top-level keyword-map table looked up by a runtime key ((get T k), (T k)) or iterated (keys, vals, "
                "seq, reduce-kv); port as (typed-map-new [:map :keyword V]) with V a scalar, [:ref R] or [:fn ..]",
        "issues": ["JUN-735"],
    },
    "kwmap/runtime-key": {
        "refusal": "E2127 assoc / lookup of an absent key; E2104 for a non-literal key on a record",
        "what": "assoc/update/dissoc/contains? with a non-literal key, into {}, zipmap, group-by, frequencies, keys/vals, "
                "hash-map, keyword: a map keyed at run time; port as [:map :i64|:keyword [:ref R]] (iterating a keyword-keyed "
                "typed map and more than 31 entries are not admitted yet)",
        "issues": ["JUN-735"],
    },
    "kwmap/param-row": {
        "refusal": "E2104 a literal passed to an unannotated parameter or an unannotated {:keys ..} parameter; "
                   "E2105 a bare record name as a parameter type; E2179 [:ref] to another namespace",
        "what": "a map read through a parameter ((:k p), {:keys ..} destructuring) or a literal passed to another "
                "namespace's fn; port as a defrecord and [:ref :ns/R] parameters (:ns the file's own), built with ->R / map->R",
        "issues": ["JUN-733"],
    },
    "kwmap/row-op": {
        "refusal": "E2101 merge / merge-with / update / update-in / assoc-in / get-in; E2104 dissoc / select-keys on a record",
        "what": "a map that grows, shrinks or is reached by a path; port as nested assoc on a declared record",
        "issues": ["JUN-733"],
    },
    "kwmap/empty": {
        "refusal": "E2133 {}",
        "what": "an empty map literal {}; port as (typed-map-new [:map K V]) or a full record",
        "issues": ["JUN-733"],
    },
    "kwmap/nil-field": {
        "refusal": "E2102 unknown symbol 'nil'",
        "what": "a keyword map field (or an assoc) holding nil; port as an [:option T] record field",
        "issues": ["JUN-733"],
    },
    "kwmap/option-default": {
        "refusal": "E2104 get with a default on a record, (or (:k m) d), :or in an unannotated {:keys ..} parameter or on a "
                   "record; E2103 (:k m d) on a record; E2101 merge",
        "what": "an option map with defaults ((get m :k d), (:k m d), (or (:k m) d), :or, (merge {defaults} m)); port as a "
                "defrecord with [:option T] fields read by option-or, or a typed map read by (get m k d) / (:k m d) "
                "(admitted since amu 05668c77, JUN-734 D10); whether a map becomes a record or a typed map is the "
                "port's decision, so every such use stays counted",
        "issues": ["JUN-733", "JUN-734"],
    },
    "kwmap/dynamic": {
        "refusal": "E2101 read-string / slurp / spit / pprint / walk; E2104 an f64 inside a :document",
        "what": "EDN read or written, or a shape inspected at run time (map?, coll?, postwalk); port as :document and the "
                "document ops; an f64 in a document is not admitted yet",
        "issues": ["JUN-735"],
    },
    "kwmap/fn-value": {
        "refusal": "E2128 a fn value in a map; E2124 for a def",
        "what": "a fn, #(..), partial or comp as a keyword map value; port as [:map :keyword [:fn ..]] with fn-ref / invoke, "
                "or a case on the key",
        "issues": ["JUN-732"],
    },
    # kwmap/variadic-assoc (55 files at c2186d7) is gone: JUN-734 (D6, amu
    # 05668c77) reads (assoc m k1 v1 k2 v2 ..) as nested assoc on records,
    # typed maps and documents; only an odd key/value count is E2103.
    "f64-generic-arith": {
        "refusal": "E2104 type mismatch in '*': expected :i64; E2101 for f64->i64",
        "what": "+ - * / < > on f64 values and f64 -> i64 conversion; the named f64-add / f64-mul / f64-lt / f64-sqrt are admitted",
        "issues": ["KOTOBA-161"],
        "detect": r"(?<![\w.\-])-?\d+\.\d+|" + r"\b(?:js/)?Math[./]\w+|\bjs/(?:isFinite|isNaN|Number|parseFloat|Infinity|NaN)\b|\b(?:Double|Float)/\w+",
    },
    "regex-literal": {
        "refusal": "E1005 unsupported reader syntax '#'",
        "what": "a regex literal #\"...\"",
        "issues": ["AWAI-47"],
    },
    "fn-literal": {
        "refusal": "E1005 unsupported reader syntax '#'",
        "what": "an anonymous fn literal #(..); port as (fn [x] ..) where the context types the parameter (an annotated "
                "fn literal parameter is E2139). amu refactor rule g (fnlit, amu acf8e1d24) did this rewrite on the hosted "
                "route and left amu with it (157355127); the native CLI has no refactor command",
        "issues": ["JUN-651"],
        "detect": r"#\(",
    },
    "auto-resolved-keyword": {
        "refusal": "E1005 unsupported reader syntax ':'",
        "what": "an auto-resolved keyword ::k or ::alias/k; port as the fully qualified :ns/k",
        "issues": ["JUN-732"],
        "detect": r"(?<![\w:])::[A-Za-z_*!?<>=+\-]",
    },
    # JUN-730 (amu 11df09b1, PR #1754) expands a closed same-module defmulti and
    # its defmethods to one defn with a static case (seed/20-names), so the
    # former single dispatch-forms blocker (18 files at 05668c7, any
    # defmulti/defmethod/defprotocol/extend-*/reify) is split by what still
    # refuses. They are read by dispatch_patterns(); a file whose multimethods
    # all have the admitted shape carries none of them.
    "dispatch/multimethod-shape": {
        "refusal": "E2006 malformed closed dispatch form (fn-literal or other non-name dispatch, defmulti options or "
                   "attr map, no defmethod, a non-integer/keyword key, differing / pattern / & / multi-arity "
                   "parameter vectors, empty body, two :default); E2001 a duplicate defmulti or an already-defined "
                   "name; E2125 a duplicate key; E2101 a dispatch symbol that is not a defn of the module; E2104 / "
                   "E2103 keyword dispatch over an unannotated or more than one parameter",
        "what": "a same-module defmulti whose group is not in the admitted shape: dispatch a keyword (one annotated "
                "parameter) or a defn of this module, integer or keyword keys, identical plain parameter vectors; "
                "port by rewriting the dispatch fn as a named defn (or a keyword) and giving every method the same "
                "parameter vector (Dispatch section of docs/port-rewrite-rules.md)",
        "issues": ["JUN-730"],
    },
    "dispatch/open-multimethod": {
        "refusal": "E2007 a defmethod of a multimethod this module does not declare; E6010 a qualified defmethod "
                   "name such as t/report; E2003 prefer-method / remove-method / derive",
        "what": "an open multimethod: a defmethod extending another namespace's multimethod (clojure.test's "
                "t/report in the run_portable runners, solver/solve) or a hierarchy; no cross-module or runtime "
                "extension, so the extension point has to become a closed case in the owning module or an explicit "
                "fn-ref argument",
        "issues": ["JUN-730", "JUN-651"],
    },
    "dispatch/protocol": {
        "refusal": "E2003 unsupported top-level form 'defprotocol' / 'extend-type' / 'extend-protocol' / "
                   "'definterface' / 'deftype'; E2128 a defrecord with inline protocol implementations",
        "what": "defprotocol, extend-type, extend-protocol, definterface, deftype, inline defrecord implementations; "
                "not admitted at 11df09b (the static protocol expansion is on amu branch claude/jun-730-protocols, "
                "blocked by the static profile's token cap); interim rewrite: record of fn-ref closures, rule 4 of "
                "docs/port-rewrite-rules.md",
        "issues": ["JUN-730"],
    },
    "dispatch/reify": {
        "refusal": "E2181 reify is not admitted (permanent refusal: closed static dispatch; implement the protocol on a record)",
        "what": "reify; permanent refusal, port as a record of fn-ref closures (rule 4 of docs/port-rewrite-rules.md); "
                "a reify that closes over locals needs those values as extra record fields",
        "issues": ["JUN-730"],
    },
    # JUN-729 (amu 5667e44c) admits destructuring in let / loop and annotated
    # defn parameters, so the blocker is read by destructuring_patterns() for
    # what native still refuses rather than for every pattern.
    "destructuring": {
        "refusal": "E2104 an unannotated defn pattern parameter, :or on a record, & over a list; E2139 a fn pattern "
                   "parameter; E2111 :strs / :syms / a non-keyword key, a pattern in doseq; E2127 an absent key; "
                   "E2101 for; E2103 when-let / if-let",
        "what": "a pattern in a defn / fn parameter (CLJK parameters are unannotated), in doseq / for / when-let / if-let "
                "bindings, or a let / loop pattern that is a map pattern or uses &; plain let / loop vector patterns "
                "(nested, :as, _) are admitted; port as an annotated parameter or a let in the body",
        "issues": ["JUN-729"],
    },
    # JUN-731 split the former single atom-escape blocker (46 files at
    # 3c9eb7f) by what a port has to do; a file may carry several of the three.
    "atom-local-iteration": {
        "refusal": "E2140 atom escapes its let scope (a write inside loop/doseq/dotimes)",
        "what": "a let-bound atom written by swap!/reset! inside loop, doseq or dotimes; rewritable by rule 1 of docs/port-rewrite-rules.md (loop accumulator or reduce)",
        "issues": ["JUN-731"],
    },
    "atom-global": {
        "refusal": "E2141 atom must be the init of a let binding (def); E2003 unsupported top-level form 'defonce'",
        "what": "a top-level def/defonce atom: state that outlives one call, out of scope for local state (state ability / :state kit)",
        "issues": ["JUN-651"],
        "detect": r"\(def\s+(?:\^\S+\s+)?\S+\s+\((?:\w+/)?atom\b|\(defonce\b",
    },
    "atom-escape": {
        "refusal": "E2140 atom escapes its let scope (straight-line swap!/reset!/deref only)",
        "what": "swap!/reset! in a file with neither of the two atom blockers above: an atom passed in, returned, stored or captured by a closure; hand work",
        "issues": ["JUN-651"],
        "detect": r"\(swap!|\(reset!",
    },
    "host-interop": {
        "refusal": "E6010 named operation js/... is not a registered capability; E2003 for an import declaration",
        "what": "js/ globals, (.method obj), (.-field obj), #js literals and other host calls (platform math such as Math/sin counts as f64, not here)",
        "issues": ["AWAI-25", "JUN-353"],
        "detect": r"\bjs/|\(\.-?[A-Za-z_]|#js\b|\bjava\.|\bSystem/",
    },
}

DATA_EXT = {".edn", ".wgsl", ".wit", ".urdf", ".csv", ".yaml", ".yml", ".inp", ".stp",
            ".cypher", ".json", ".png", ".ttf", ".otf", ".svg", ".m"}
GLUE_EXT = {".js", ".mjs", ".ts", ".html", ".webmanifest", ".wasm"}
KEEP_NAMES = {"LICENSE", "NOTICE", ".gitignore", ".nojekyll", "imports.json",
              "local-changes.json", "cljk-origin.edn", "Dockerfile"}
REMOVE_NAMES = {"deps.edn", "nbb.edn", "package.json", "tasks.edn"}
NATIVE_PATHS = {"scripts/native_gates.py", "scripts/migration_ledger.py", "scripts/amu_capability_probe.py",
                "scripts/placement_consumers.py", "scripts/upstream_apis.py", ".github/workflows/ci.yml"}
# CLJK scripts whose job the native gates took over in JUN-642: oracle only.
GATE_ORACLES = {"scripts/wit_test.cljk", "scripts/check_adapter_registry.cljk", "scripts/guest_lint.cljk"}


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True,
                          text=True).stdout


def tracked():
    """(path, mode) of every tracked file, gitlinks included."""
    out = []
    for line in git("ls-files", "-s", "-z").split("\0"):
        if line:
            meta, path = line.split("\t", 1)
            out.append((path, meta.split()[0]))
    return out


def unit_of(path):
    parts = path.split("/")
    if parts[0] == "packages" and len(parts) > 2:
        return "packages/" + parts[1]
    return parts[0] if len(parts) > 1 else "(root)"


def strip_code(src):
    """Remove ; comments and string contents; keep a marker for regex literals."""
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if c == ";":
            while i < n and src[i] != "\n":
                i += 1
        elif c == "\\" and i + 1 < n:
            out.append("\\x")
            i += 2
        elif c == '"':
            regex = i > 0 and src[i - 1] == "#"
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            out.append('"REGEX"' if regex else '""')
            i += 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


def form_at(src, start):
    """The balanced form starting at src[start] == '('."""
    depth = 0
    for j in range(start, len(src)):
        if src[j] in "([{":
            depth += 1
        elif src[j] in ")]}":
            depth -= 1
            if depth == 0:
                return src[start:j + 1]
    return src[start:]


SYM = r"[A-Za-z][\w.\-]*"
REFER_KEYS = (":refer", ":exclude", ":rename", ":only", ":refer-macros")


def ns_info(code):
    m = re.search(r"\(ns\s+(?:\^\S+\s+)?(" + SYM + ")", code)
    if not m:
        return None, []
    form = form_at(code, m.start())
    reqs = set()
    for r in re.finditer(r"\(:require\b", form):
        body = form_at(form, r.start())[len("(:require"):-1]
        # opened: the token before each open bracket. A symbol is a required
        # namespace at the top of :require or as the head of a libspec, but not
        # inside a :refer/:exclude/:rename/:only list, where single-segment
        # names (scene, bim, input) would otherwise read as namespaces.
        opened = []
        tok = re.compile(r"[\[\](){}]|#\?@?|" + SYM + r"|:[\w.\-/]+|\S")
        prev = None
        for t in tok.findall(body):
            if t in "([{":
                opened.append(prev)
            elif t in ")]}":
                if opened:
                    opened.pop()
            elif re.fullmatch(SYM, t):
                if not opened or (prev in ("[", "(") and opened[-1] not in REFER_KEYS):
                    reqs.add(t)
            prev = t
    return m.group(1), sorted(reqs)


def classify(path, mode, code):
    name = path.rsplit("/", 1)[-1]
    ext = "." + name.rsplit(".", 1)[-1] if "." in name else ""
    parts = path.split("/")
    if mode == "160000":
        return {"class": "keep", "note": "gitlink without a .gitmodules entry; decided in JUN-647"}
    if path in NATIVE_PATHS or ext == ".kotoba":
        return {"class": "native"}
    if "hermes" in parts or parts[0] in (".claude", ".github"):
        return {"class": "keep"}
    if parts[:2] == ["docs", "evidence"] or path in ("docs/migration-ledger.json", "docs/migration-ledger.md",
                                                     "docs/migration-placement.json", "docs/upstream-apis.json",
                                                     "docs/upstream-apis.md"):
        return {"class": "keep"}
    if name in REMOVE_NAMES:
        return {"class": "d", "note": "kbb/nbb manifest; JUN-647 decides what the hand-run oracle keeps"}
    if name in KEEP_NAMES or ext == ".md":
        return {"class": "keep"}
    if "dist" in parts:
        return {"class": "d", "note": "built output"}
    if ext == ".sh":
        return {"class": "d", "note": "shell runner outside the native gate"}
    if path == "wrangler.jsonc" or name == "wrangler.jsonc":
        return {"class": "keep", "note": "static asset hosting configuration"}
    if ext == ".cljk":
        rec = {"class": "o", "kotoba": path[:-len(".cljk")] + ".kotoba"}
        if path in GATE_ORACLES:
            rec.update(port="native", note="replaced by scripts/native (JUN-642)")
        elif "scripts" in parts or "bin" in parts:
            rec.update(port="d", note="kbb runner or tooling")
            del rec["kotoba"]
        elif re.search(r"\bjs/|#js\b|\(\.-[A-Za-z_]", host_code(code)):
            rec["port"] = "c"
        else:
            rec["port"] = "b"
        if "test" in parts:
            rec["test"] = True
        return rec
    if ext in GLUE_EXT:
        return {"class": "c"}
    if ext in DATA_EXT:
        return {"class": "a"}
    return {"class": "keep", "note": "unclassified extension"}


# Platform math, number helpers and error classes written per platform with
# #?(:clj ... :cljs ...). Math is f64 arithmetic and the error classes are
# catch clauses; neither is host glue.
PLATFORM = re.compile(r"\b(?:js/)?Math[./]\w+|\bjs/(?:isFinite|isNaN|Number|parseFloat|parseInt|Infinity|NaN|Error)\b"
                      r"|\b(?:Double|Float|Long|Integer)/\w+|\b(?:clojure\.lang\.)?ExceptionInfo\b")


def host_code(code):
    return PLATFORM.sub("", code)


ITERATION = re.compile(r"\((loop|doseq|dotimes)(?=[\s)])")
LOCAL_ATOM = re.compile(r"[\[\s](" + SYM + r")\s+\((?:\w+/)?atom\b")


def atom_written_in_iteration(code):
    """A let-bound atom name that swap!/reset! writes inside loop/doseq/dotimes.

    Names bound by def/defonce are excluded, so a global written in a loop
    counts as atom-global only."""
    names = {m.group(1) for m in LOCAL_ATOM.finditer(code)
             if not re.search(r"\((?:def|defonce)\s+(?:\^\S+\s+)?$", code[:m.start(1)])}
    if not names:
        return False
    write = re.compile(r"\((?:swap!|reset!)\s+(" + "|".join(re.escape(n) for n in sorted(names)) + r")(?=[\s)])")
    return any(write.search(form_at(code, m.start())) for m in ITERATION.finditer(code))


# JUN-732: a small reader over strip_code output (strings are "" and regex
# literals "REGEX"), enough to tell how a keyword map is used. A node is
# (kind, items, pos); atoms are token strings.
TOKEN = re.compile(r'#\{|#\(|#js|#_|\^|[()\[\]{}]|"[^"]*"|\\x|[^\s()\[\]{},"]+')
OPEN = {"(": (")", "list"), "[": ("]", "vec"), "{": ("}", "map"), "#{": ("}", "set"), "#(": (")", "fnlit")}


def read_forms(code):
    toks = [(m.group(0), m.start()) for m in TOKEN.finditer(code)]
    i = 0

    def rd():
        nonlocal i
        t, p = toks[i]
        i += 1
        if t in OPEN:
            close, kind = OPEN[t]
            items = []
            while i < len(toks) and toks[i][0] != close:
                x = rd()
                if x is not None:
                    items.append(x)
            i += 1
            return (kind, items, p)
        if t == "#js":
            return ("js", [rd()], p) if i < len(toks) else None
        if t == "#_":
            if i < len(toks):
                rd()
            return None
        if t == "^":
            if i < len(toks):
                rd()
            return rd() if i < len(toks) else None
        return None if t in (")", "]", "}") else t

    out = []
    while i < len(toks):
        x = rd()
        if x is not None:
            out.append(x)
    return out


def is_node(x, *kinds):
    return isinstance(x, tuple) and (not kinds or x[0] in kinds)


def is_kw(x):
    return isinstance(x, str) and x.startswith(":")


def is_sym(x):
    return (isinstance(x, str) and x not in ("nil", "true", "false")
            and re.match(r"^[A-Za-z_*!?<>=+\-][\w*!?<>=+\-'./]*$", x) is not None)


def is_name(x, names):
    return isinstance(x, str) and x in names


def is_kwmap(x):
    return is_node(x, "map") and len(x[1]) >= 2 and all(is_kw(k) for k in x[1][0::2])


def head(x):
    return x[1][0] if is_node(x, "list", "fnlit") and x[1] and isinstance(x[1][0], str) else None


def walk(x, parents=()):
    yield x, parents
    if is_node(x):
        for c in x[1]:
            yield from walk(c, parents + (x,))


def contains_node(x, pred):
    return any(pred(y) for y, _ in walk(x))


# The former keyword-map-literal detection: the kwmap/* blockers are read only
# in files that hold a keyword map literal.
KWMAP_LITERAL = r"\{\s*:"
FLOAT = re.compile(r"^-?\d+\.\d")
ITER_HEADS = {"reduce", "reduce-kv", "loop", "swap!", "doseq", "iterate", "dotimes", "for"}
ROW_OPS = {"assoc", "dissoc", "merge", "update", "update-in", "assoc-in", "select-keys", "merge-with"}
RUNTIME_COLL = {"zipmap", "group-by", "frequencies", "reduce-kv", "keys", "vals", "index-by", "update-vals",
                "update-keys", "sorted-map", "hash-map", "array-map", "keyword"}
TABLE_READ = {"keys", "vals", "reduce-kv", "seq"}
EDN_IO = {"edn/read-string", "clojure.edn/read-string", "read-string", "slurp", "spit", "pp/pprint", "pprint",
          "edn/read", "clojure.edn/read", "pr-str", "prn-str", "load-edn", "read-edn"}
SHAPE_TESTS = {"map?", "coll?", "postwalk", "prewalk", "walk/postwalk", "walk/prewalk", "w/postwalk",
               "clojure.walk/postwalk", "clojure.walk/prewalk", "sequential?", "vector?", "keyword?"}
CORE_NS = ("clojure.", "str/", "string/", "set/", "math/", "Math/", "js/", "edn/", "pp/", "walk/", "w/", "s/", "t/", "test/")


def literal_type(x):
    """The type of a literal item as far as syntax tells (JUN-734 D5); None
    for a symbol, a call, nil or an empty vector, whose type is type flow."""
    if isinstance(x, str):
        if FLOAT.match(x):
            return "f64"
        if re.match(r"^-?\d+$", x):
            return "i64"
        if x in ("true", "false"):
            return "bool"
        if x.startswith('"'):
            return "string"
        if is_kw(x):
            return "keyword"
        return None
    if is_kwmap(x):
        return ("map",) + tuple(sorted((k, literal_type(v)) for k, v in zip(x[1][0::2], x[1][1::2])))
    if is_node(x, "vec") and x[1]:
        kinds = {literal_type(y) for y in x[1]} - {None}
        return ("vec", next(iter(kinds))) if len(kinds) == 1 else None
    return None


def compatible(a, b):
    """Two literal item types the checker may unify (None matches anything)."""
    if a is None or b is None:
        return True
    if isinstance(a, tuple) and isinstance(b, tuple) and a[0] == b[0] == "map":
        return ([k for k, _ in a[1:]] == [k for k, _ in b[1:]]
                and all(compatible(u, v) for (_, u), (_, v) in zip(a[1:], b[1:])))
    return a == b


def refused_def_value(v):
    """A def value the native checker still refuses at amu 05668c77 (E2116): a
    vector inside it whose literal items differ in type, or that holds more
    than 64 items that are not integers."""
    for y, _ in walk(v):
        if not is_node(y, "vec") or not y[1]:
            continue
        if len(y[1]) > 64 and any(literal_type(z) != "i64" for z in y[1]):
            return True
        ts = [literal_type(z) for z in y[1]]
        if any(not compatible(a, b) for i, a in enumerate(ts) for b in ts[i + 1:]):
            return True
    return False


def fn_params(x):
    """Parameter vectors of a defn/fn node (single and multi-arity)."""
    out = []
    for y in x[1][1:4]:
        if is_node(y, "vec"):
            out.append(y)
            break
    out += [y[1][0] for y in x[1] if is_node(y, "list") and y[1] and is_node(y[1][0], "vec")]
    return out


def kwmap_patterns(code):
    """The kwmap/* blockers (without the prefix) a CLJK file needs (JUN-732)."""
    forms = read_forms(code)
    found = set()
    tables = set()
    for f in forms:
        if head(f) in ("def", "defonce") and len(f[1]) >= 3:
            v = f[1][-1]
            if is_kwmap(v) or (is_node(v, "vec") and v[1] and all(is_kwmap(y) for y in v[1])):
                tables.add(f[1][1])
                if refused_def_value(v):
                    found.add("def-literal")
    for x, par in walk(("list", forms, 0)):
        if not is_node(x):
            continue
        h = head(x)
        a = x[1][1:] if h else []
        threaded = bool(h and par and len(par[-1][1]) > 1 and par[-1][1][1] is not x)
        if threaded and head(par[-1]) in ("->", "some->", "cond->", "doto"):
            a = ["%THREAD%"] + a
        elif threaded and head(par[-1]) in ("->>", "some->>", "cond->>"):
            a = a + ["%THREAD%"]
        if ((h == "get" and len(a) >= 2 and is_name(a[0], tables) and not is_kw(a[1]))
                or (h in tables and a and not is_kw(a[0]))
                or (h in TABLE_READ and a and is_name(a[0], tables))):
            found.add("table-runtime-key")
        if h in ("assoc", "update", "dissoc", "contains?") and len(a) >= 2:
            if any(not is_kw(k) for k in (a[1::2] if h == "assoc" else a[1:2])):
                found.add("runtime-key")
        if h in RUNTIME_COLL or (h == "into" and a and is_node(a[0], "map") and not a[0][1]):
            found.add("runtime-key")
        if h in ("update", "update-in", "merge", "merge-with", "dissoc", "select-keys", "assoc-in", "get-in"):
            found.add("row-op")
        if ((h in ("merge", "merge-with") and a and is_kwmap(a[0]))
                or (h == "get" and len(a) == 3 and is_kw(a[1]))
                or (is_kw(h) and len(a) == 2)
                or (h == "or" and a and is_kw(head(a[0])))):
            found.add("option-default")
        if h in EDN_IO or h in SHAPE_TESTS:
            found.add("dynamic")
        if h and "/" in h and not h.startswith(CORE_NS) and any(is_kwmap(y) for y in a):
            found.add("param-row")
        if h in ("defn", "defn-", "fn") or x[0] == "fnlit":
            for pv in fn_params(x):
                if any(is_node(q, "map") for q in pv[1]):
                    found.add("param-row")
                    if any(is_node(q, "map") and ":or" in q[1] for q in pv[1]):
                        found.add("option-default")
                names = {q for q in pv[1] if is_sym(q) and q != "&"}
                if names and h in ("defn", "defn-", "fn") and pv in x[1]:
                    body = x[1][x[1].index(pv) + 1:]
                    if any(contains_node(b, lambda y: is_node(y, "list") and len(y[1]) >= 2 and is_kw(y[1][0])
                                         and is_name(y[1][1], names)) for b in body):
                        found.add("param-row")
        if x[0] == "map":
            if not x[1]:
                found.add("empty")
            elif is_kwmap(x):
                vals = x[1][1::2]
                if "nil" in vals:
                    found.add("nil-field")
                if any(is_node(v) and (v[0] == "fnlit" or head(v) in ("fn", "fn-ref", "partial", "comp")) for v in vals):
                    found.add("fn-value")
        if h == "assoc" and len(a) >= 3 and "nil" in a[2::2]:
            found.add("nil-field")
    return found


PATTERN_FNS = ("defn", "defn-", "fn", "defmethod")
PATTERN_LETS = ("let", "loop")
PATTERN_OTHER = ("doseq", "for", "when-let", "if-let", "when-some", "if-some", "when-first", "dotimes")


def destructuring_patterns(code):
    """Why a CLJK file's destructuring is still refused at amu 05668c77 (JUN-729).

    Refused: a pattern in a defn / defmethod parameter (CLJK parameters carry no
    annotation, E2104) or a fn parameter (E2139); a pattern in doseq / for /
    when-let / if-let .. bindings (E2111 / E2101 / E2103); in let / loop, a map
    pattern and a & rest. A map pattern is counted whole: an absent key (E2127),
    :or on a record (E2104) and a :keys name never written as a keyword (E2111)
    depend on the type and the module, not on the pattern, and :strs / :syms /
    a non-keyword key (E2111) are map patterns too. & over a vector is admitted
    and over a list E2104, which is type flow, so every & counts. Admitted and
    not counted: a let / loop vector pattern without & (nested, :as, _); its nth
    traps when the vector is short where Clojure binds nil."""
    found = set()
    for x, _ in walk(("list", read_forms(code), 0)):
        h = head(x)
        if h in PATTERN_FNS:
            if any(is_node(q, "vec", "map") for pv in fn_params(x) for q in pv[1]):
                found.add("fn-param" if h == "fn" else "defn-param")
        elif h in PATTERN_LETS + PATTERN_OTHER and len(x[1]) > 1 and is_node(x[1][1], "vec"):
            pats = [q for q in x[1][1][1][0::2] if is_node(q, "vec", "map")]
            if pats and h in PATTERN_OTHER:
                found.add("binding-form")
            for p in pats:
                for y, _ in walk(p):
                    if is_node(y, "map"):
                        found.add("map-pattern")
                    elif is_node(y, "vec") and "&" in y[1]:
                        found.add("rest")
    return found


DISPATCH_PROTOCOL = ("defprotocol", "extend-type", "extend-protocol", "definterface", "deftype")
DISPATCH_OPEN = ("prefer-method", "remove-method", "derive", "underive", "make-hierarchy")
INT = re.compile(r"^-?\d+$")


def unpos(x):
    """A node without its source positions, for comparing forms as written."""
    return (x[0], [unpos(y) for y in x[1]]) if is_node(x) else x


def plain_params(pv):
    """A defmethod parameter vector the closed expansion admits: plain names,
    each optionally followed by a type (a keyword or a vector headed by a
    keyword); no pattern and no &. Returns [(name, typed?)] or None."""
    out, items = [], pv[1]
    i = 0
    while i < len(items):
        q = items[i]
        if not is_sym(q) or q == "&" or "/" in q:
            return None
        typed = i + 1 < len(items) and (is_kw(items[i + 1]) or (is_node(items[i + 1], "vec") and items[i + 1][1]
                                                                 and is_kw(items[i + 1][1][0])))
        out.append((q, typed))
        i += 2 if typed else 1
    return out


def dispatch_patterns(code):
    """The dispatch/* blockers (without the prefix) a CLJK file needs at amu
    11df09b1 (JUN-730).

    seed/20-names expands a closed multimethod: a top-level (defmulti NAME [doc]
    DISPATCH) and every top-level (defmethod NAME KEY [params] body ..) of the
    same module become one defn with a static case. Conservative reading of the
    probes in docs/evidence/amu-capability-probe-20261101d.json:

    - open-multimethod: a defmethod of a name no top-level defmulti of the file
      declares (E2007; a qualified name such as t/report is E6010), and
      prefer-method / remove-method / derive / underive / make-hierarchy.
    - multimethod-shape: a declared multimethod whose group is not the admitted
      shape. DISPATCH must be a keyword (then exactly one parameter, annotated:
      an unannotated one is E2104, two are E2103) or a symbol that a top-level
      defn / defn- of the file defines (clojure.core names such as identity or
      type are E2101, a fn literal or call is E2006); nothing after DISPATCH and
      no attr map; at least one defmethod; keys are integers or keywords other
      than ::auto-resolved ones (strings, nil, class names are E2006), unique
      (E2125), at most one :default; every parameter vector identical as written
      (names included: [_ y] next to [x y] is E2006) and plain (no pattern, no &,
      not multi-arity); a non-empty body; the name defined once (E2001). A
      defmulti or defmethod that is not a top-level form (inside #? or a do) is
      counted here too.
    - protocol: defprotocol / extend-type / extend-protocol / definterface /
      deftype heads anywhere, and a defrecord with forms after its field vector.
    - reify: any reify head (E2181, permanent).

    Whether the expanded defn then checks (parameter types, keyword-map use,
    a throwing method in a public defn being E2130) is left to the other
    blockers, as for any defn."""
    forms = read_forms(code)
    found = set()
    top = set(id(f) for f in forms)
    defined = defaultdict(int)
    defns = set()
    multis, methods = defaultdict(list), defaultdict(list)
    for f in forms:
        h = head(f)
        if h in ("def", "defn", "defn-", "defonce", "defrecord", "defmulti") and len(f[1]) > 1 and is_sym(f[1][1]):
            defined[f[1][1]] += 1
            if h in ("defn", "defn-"):
                defns.add(f[1][1])
    for x, _ in walk(("list", forms, 0)):
        h = head(x)
        if h in DISPATCH_PROTOCOL:
            found.add("protocol")
        elif h == "reify":
            found.add("reify")
        elif h in DISPATCH_OPEN:
            found.add("open-multimethod")
        elif h == "defrecord" and len(x[1]) > 3:
            found.add("protocol")
        elif h in ("defmulti", "defmethod"):
            name = x[1][1] if len(x[1]) > 1 else None
            (multis if h == "defmulti" else methods)[name].append((x, id(x) in top))
    for name, ms in methods.items():
        if name not in multis:
            found.add("open-multimethod")
    for name, ds in multis.items():
        ms = methods.get(name, [])
        ok = (len(ds) == 1 and ds[0][1] and is_sym(name) and "/" not in name and defined[name] == 1
              and ms and all(t for _, t in ms))
        if ok:
            rest = ds[0][0][1][2:]
            if rest and rest[0] == '""':
                rest = rest[1:]
            disp = rest[0] if len(rest) == 1 else None
            ok = isinstance(disp, str) and ((is_kw(disp) and not disp.startswith("::"))
                                            or (is_sym(disp) and disp in defns and disp != name))
        if ok:
            keys, pvs = [], []
            for m, _ in ms:
                a = m[1][2:]
                if len(a) < 3 or not is_node(a[1], "vec"):
                    ok = False
                    break
                keys.append(a[0])
                pvs.append(a[1])
            ok = (ok and all(isinstance(k, str) and (INT.match(k) or (is_kw(k) and not k.startswith("::")))
                             for k in keys)
                  and len(set(keys)) == len(keys))
            if ok:
                shapes = [plain_params(pv) for pv in pvs]
                ok = all(sh is not None for sh in shapes) and all(unpos(pv) == unpos(pvs[0]) for pv in pvs[1:])
                if ok and is_kw(disp):
                    ok = len(shapes[0]) == 1 and shapes[0][0][1]
        if not ok:
            found.add("multimethod-shape")
    return found


def blockers_of(code, reqs):
    found = []
    kwmap = kwmap_patterns(code) if re.search(KWMAP_LITERAL, code) else set()
    dispatch = dispatch_patterns(code)
    for bid, b in BLOCKERS.items():
        if bid.startswith("kwmap/"):
            hit = bid[len("kwmap/"):] in kwmap
        elif bid == "destructuring":
            hit = bool(destructuring_patterns(code))
        elif bid.startswith("dispatch/"):
            hit = bid[len("dispatch/"):] in dispatch
        elif bid == "atom-local-iteration":
            hit = atom_written_in_iteration(code)
        elif bid == "atom-escape":
            hit = (re.search(b["detect"], code) is not None
                   and not {"atom-local-iteration", "atom-global"} & set(found))
        elif bid == "multi-module":
            hit = bool(reqs)
        elif bid == "regex-literal":
            hit = '"REGEX"' in code
        elif bid == "host-interop":
            hit = re.search(b["detect"], host_code(code)) is not None
        else:
            hit = re.search(b["detect"], code) is not None
        if hit:
            found.append(bid)
    return found


def kotoba_namespaces(kotoba, rev):
    """namespace -> {lib, kotoba?} from the kotoba monorepo's lib/ at rev.

    A namespace defined by several libraries resolves to the first in path
    order, and kotoba? says whether that library has a .kotoba for it: a
    .kotoba in another library (kotoba.io is in both lib/kotoba.io and
    lib/kotoba.kotobaIo) does not port the one kami depends on."""
    names = subprocess.run(["git", "-C", str(kotoba), "ls-tree", "-r", "--name-only", rev, "lib"],
                           check=True, capture_output=True, text=True).stdout.split("\n")
    table, ported = {}, set()
    for p in names:
        m = re.match(r"lib/([^/]+)/src/(.+)\.(cljk|cljc|cljs|clj|kotoba)$", p)
        if not m:
            continue
        ns = m.group(2).replace("/", ".").replace("_", "-")
        table.setdefault(ns, {"lib": m.group(1), "kotoba": False})
        if m.group(3) == "kotoba":
            ported.add((ns, m.group(1)))
    for ns, e in table.items():
        e["kotoba"] = (ns, e["lib"]) in ported
    return table


def waves(units, deps):
    """Collapse cycles (Tarjan), then wave = 1 + longest dependency chain."""
    index, low, stack, on, comp, sccs = {}, {}, [], set(), {}, []
    sys.setrecursionlimit(10000)

    def visit(v):
        index[v] = low[v] = len(index)
        stack.append(v)
        on.add(v)
        for w in deps[v]:
            if w not in index:
                visit(w)
                low[v] = min(low[v], low[w])
            elif w in on:
                low[v] = min(low[v], index[w])
        if low[v] == index[v]:
            group = []
            while True:
                w = stack.pop()
                on.discard(w)
                comp[w] = len(sccs)
                group.append(w)
                if w == v:
                    break
            sccs.append(sorted(group))

    for u in sorted(units):
        if u not in index:
            visit(u)
    level = {}
    for i, group in enumerate(sccs):  # Tarjan emits dependencies first
        below = [level[comp[w]] for v in group for w in deps[v] if comp[w] != i]
        level[i] = 1 + max(below, default=-1)
    return {u: level[comp[u]] for u in units}, [g for g in sccs if len(g) > 1]


def build(kotoba, rev):
    files, by_unit = [], defaultdict(list)
    defined = {}
    raw = {}
    for path, mode in tracked():
        code = ""
        if path.endswith(".cljk") and mode != "160000":
            code = strip_code((ROOT / path).read_text(encoding="utf-8", errors="replace"))
        rec = {"path": path, "unit": unit_of(path), **classify(path, mode, code)}
        if code:
            ns, reqs = ns_info(code)
            if ns:
                rec["ns"] = ns
                defined.setdefault(ns, rec["unit"])
            raw[path] = reqs
            if rec.get("port") in ("b", "c"):
                b = blockers_of(code, reqs)
                if b:
                    rec["blockers"] = b
        files.append(rec)
        by_unit[rec["unit"]].append(rec)

    libs = kotoba_namespaces(kotoba, rev)
    units, deps = [], {}
    for u in sorted(by_unit):
        recs = by_unit[u]
        internal, external = set(), defaultdict(set)
        for r in recs:
            for q in raw.get(r["path"], []):
                if q in defined:
                    if defined[q] != u:
                        internal.add(defined[q])
                elif q in libs:
                    external["kotoba/lib/" + libs[q]["lib"]].add(q)
                else:
                    external["unresolved"].add(q)
        deps[u] = sorted(internal)
        classes = defaultdict(int)
        ports = defaultdict(int)
        blk = defaultdict(int)
        for r in recs:
            classes[r["class"]] += 1
            if "port" in r:
                ports[r["port"]] += 1
            for b in r.get("blockers", []):
                blk[b] += 1
        entry = {"unit": u, "files": len(recs), "classes": dict(sorted(classes.items()))}
        if ports:
            entry["port"] = dict(sorted(ports.items()))
        ns = sorted({r["ns"] for r in recs if "ns" in r})
        if ns:
            entry["namespaces"] = ns
        entry["depends-on"] = deps[u]
        if external:
            entry["external"] = {k: sorted(v) for k, v in sorted(external.items())}
        if blk:
            entry["blockers"] = dict(sorted(blk.items()))
        units.append(entry)

    porting = {e["unit"] for e in units if e.get("port", {}).get("b") or e.get("port", {}).get("c")}
    sub = {u: [w for w in deps[u] if w in porting] for u in porting}
    level, cycles = waves(porting, sub)
    for e in units:
        if e["unit"] in level:
            e["wave"] = level[e["unit"]]
    wave_list = defaultdict(list)
    for u, w in level.items():
        wave_list[w].append(u)

    used = defaultdict(lambda: {"namespaces": set(), "units": set()})
    for e in units:
        for lib, nss in e.get("external", {}).items():
            used[lib]["namespaces"].update(nss)
            used[lib]["units"].add(e["unit"])
    upstream = []
    for lib in sorted(used):
        nss = sorted(used[lib]["namespaces"])
        item = {"lib": lib, "units": len(used[lib]["units"]), "namespaces": nss}
        if lib != "unresolved":
            item["kotoba-ported"] = sorted(n for n in nss if libs[n]["kotoba"])
        upstream.append(item)

    counts = defaultdict(int)
    for r in files:
        counts[r["class"]] += 1
    ported = defaultdict(int)
    for r in files:
        if "port" in r:
            ported[r["port"]] += 1
    blocker_files = defaultdict(int)
    for r in files:
        for b in r.get("blockers", []):
            blocker_files[b] += 1
    blockers = {k: {kk: vv for kk, vv in v.items() if kk != "detect"} | {"files": blocker_files[k]}
                for k, v in BLOCKERS.items()}

    placement = json.loads(PLACEMENT.read_text(encoding="utf-8"))
    decided = placement["units"]
    for e in units:
        key = "kami-engine:" + e["unit"]
        if key not in decided:
            raise SystemExit(f"{key} has no entry in {PLACEMENT.relative_to(ROOT)}; decide its placement (JUN-649)")
        e["placement"] = decided[key]["to"]
    place_counts = defaultdict(lambda: defaultdict(int))
    for key, d in decided.items():
        place_counts[key.split(":")[0]][d["to"]] += 1

    return {
        "schema": SCHEMA,
        "issue": "JUN-641",
        "inputs": {
            "kotoba-lib": {"repo": "kotoba-lang/kotoba", "rev": rev},
            "amu-probe": {"repo": "kotoba-lang/amu", "rev": "11df09b1f60effadbd521c4031defedb12614139",
                          "evidence": "docs/evidence/amu-capability-probe-20261101d.json"},
        },
        "classes": CLASSES,
        "counts": {"tracked": len(files), "class": dict(sorted(counts.items())),
                   "port": dict(sorted(ported.items()))},
        "blockers": blockers,
        "waves": [{"wave": w, "units": sorted(wave_list[w])} for w in sorted(wave_list)],
        "cycles": cycles,
        "upstream": upstream,
        "placement": {"source": str(PLACEMENT.relative_to(ROOT)), "status": placement["status"],
                      "counts": {k: dict(sorted(v.items())) for k, v in sorted(place_counts.items())},
                      "kotoba-lib": {k: d["to"] for k, d in sorted(decided.items()) if k.startswith("kotoba:")}},
        "units": units,
        "files": files,
    }


def summary(ledger):
    """docs/migration-ledger.md: the reviewable view of the JSON ledger."""
    c = ledger["counts"]
    out = [
        "# kami migration ledger",
        "",
        "Generated by `scripts/migration_ledger.py` from `docs/migration-ledger.json`",
        f"(JUN-641). Do not edit by hand. kotoba `lib/` read at `{ledger['inputs']['kotoba-lib']['rev'][:12]}`;",
        f"Amu blockers measured with amu `{ledger['inputs']['amu-probe']['rev'][:12]}`",
        f"(`{ledger['inputs']['amu-probe']['evidence']}`).",
        "",
        f"## Files ({c['tracked']}, the same as `git ls-files`)",
        "",
        "| class | files | meaning |",
        "|---|---:|---|",
    ]
    for k, n in c["class"].items():
        out.append(f"| {k} | {n} | {CLASSES[k]} |")
    out += ["", "Where the `o` (CLJK oracle) files go:", "", "| port | files |", "|---|---:|"]
    for k, n in c["port"].items():
        out.append(f"| {k} | {n} |")
    out += ["", "## Blockers (class e)", "",
            "Files whose CLJK uses a construct the pinned Amu CLI refuses. The detection is a regex",
            "over each `b`/`c` file, so a count is an estimate of the work, not a proof.", "",
            "| blocker | files | refusal | issues |", "|---|---:|---|---|"]
    for k, b in ledger["blockers"].items():
        out.append(f"| {k} | {b['files']} | {b['refusal']} | {', '.join(b['issues'])} |")
    out += ["", "## Port order", "",
            "Waves come from the namespace DAG between units that have `b` or `c` files: a unit",
            "is in wave n when the longest chain of in-repo units it depends on is n long. A",
            "unit also waits for its upstream kotoba libraries (Phase 3, JUN-643) and for the",
            "blockers above, which the waves do not model.", ""]
    for w in ledger["waves"]:
        out.append(f"- **wave {w['wave']}** ({len(w['units'])}): " + ", ".join(f"`{u}`" for u in w["units"]))
    if ledger["cycles"]:
        out += ["", "Cycles (ported together): " + "; ".join(", ".join(g) for g in ledger["cycles"])]
    out += placement_summary()
    out += ["", "## Upstream kotoba libraries", "",
            "`ported` counts the namespaces that already have a `.kotoba` source in kotoba `lib/`.", "",
            "| library | units | namespaces | ported |", "|---|---:|---:|---:|"]
    for u in sorted(ledger["upstream"], key=lambda x: (-x["units"], x["lib"])):
        ported = len(u["kotoba-ported"]) if "kotoba-ported" in u else "-"
        out.append(f"| {u['lib']} | {u['units']} | {len(u['namespaces'])} | {ported} |")
    out += ["", "`unresolved` namespaces are neither defined here nor found in kotoba `lib/`: "
            + ", ".join(f"`{n}`" for n in next((u["namespaces"] for u in ledger["upstream"]
                                                 if u["lib"] == "unresolved"), [])), ""]
    return "\n".join(out)


def placement_summary():
    placement = json.loads(PLACEMENT.read_text(encoding="utf-8"))
    evidence_path = ROOT / placement["evidence"]
    consumers = json.loads(evidence_path.read_text())["units"] if evidence_path.exists() else {}
    out = ["", "## Placement (JUN-649)", "",
           f"From `{PLACEMENT.relative_to(ROOT)}` ({placement['status']}). Rule: {placement['rule']}", "",
           "`outside` lists the consumers that are neither this repository, kotoba `lib/`, an old",
           f"standalone copy nor a mechanical compat twin (`{placement['evidence']}`).", "",
           "| unit | to | reason | outside |", "|---|---|---|---|"]
    for key, d in placement["units"].items():
        reason = d["reason"]
        if "duplicate" in d:
            reason += f" (duplicate of `{d['duplicate']['of']}`; keep `{d['duplicate']['keep']}`)"
        if "split" in d:
            reason += f" (stays in kotoba: {', '.join(d['split']['stay-in-kotoba'])})"
        outside = sorted(r for r, row in consumers.get(key, {}).items() if row["kind"] in ("game", "other"))
        out.append(f"| `{key}` | {d['to']} | {reason.replace('|', '/')} | {', '.join(outside)} |")
    return out


def write(ledger):
    text = json.dumps(ledger, ensure_ascii=False, indent=1) + "\n"
    LEDGER.write_text(text, encoding="utf-8")
    (ROOT / "docs" / "migration-ledger.md").write_text(summary(ledger), encoding="utf-8")


def check():
    ledger = json.loads(LEDGER.read_text(encoding="utf-8"))
    listed = [f["path"] for f in ledger["files"]]
    actual = [p for p, _ in tracked()]
    missing = sorted(set(actual) - set(listed))
    extra = sorted(set(listed) - set(actual))
    dup = len(listed) - len(set(listed))
    ok = not missing and not extra and not dup and ledger["counts"]["tracked"] == len(actual)
    for p in missing[:20]:
        print("not in ledger:", p)
    for p in extra[:20]:
        print("in ledger but not tracked:", p)
    if dup:
        print("duplicate paths:", dup)
    decided = json.loads(PLACEMENT.read_text(encoding="utf-8"))["units"]
    undecided = [u["unit"] for u in ledger["units"] if "kami-engine:" + u["unit"] not in decided]
    bad = [k for k, d in decided.items() if d["to"] not in ("kami-engine", "kotoba", "split", "owner")]
    for u in undecided:
        print("no placement:", u)
    for k in bad:
        print("unknown placement value:", k)
    ok = ok and not undecided and not bad
    print(f"ledger {len(listed)} files, git ls-files {len(actual)}, "
          f"{len(ledger['units'])} units placed: {'ok' if ok else 'MISMATCH'}")
    if not ok:
        print("regenerate: python3 scripts/migration_ledger.py --kotoba PATH --kotoba-rev REV")
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--kotoba", type=Path, help="a kotoba-lang/kotoba checkout")
    ap.add_argument("--kotoba-rev", help="the kotoba revision to read lib/ from")
    a = ap.parse_args()
    if a.check:
        return check()
    if not a.kotoba or not a.kotoba_rev:
        ap.error("--kotoba and --kotoba-rev are required to regenerate")
    rev = subprocess.run(["git", "-C", str(a.kotoba), "rev-parse", a.kotoba_rev], check=True,
                         capture_output=True, text=True).stdout.strip()
    ledger = build(a.kotoba, rev)
    write(ledger)
    print(json.dumps({"counts": ledger["counts"], "waves": len(ledger["waves"]),
                      "cycles": len(ledger["cycles"])}))
    return check()


if __name__ == "__main__":
    sys.exit(main())
