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
docs/evidence/amu-capability-probe-20261031.json were run against it.
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

# Measured with the pinned Amu macOS CLI (kotoba-lang/amu f7c07dff86d7, the
# one CI uses) on 2026-10-31. `detect` is a regex over a CLJK file with its
# comments and string contents removed.
BLOCKERS = {
    "multi-module": {
        "refusal": "E6002 required module ... is missing from the explicit source paths",
        "what": "a source that requires another namespace; the CLI compiles one self-contained file",
        "issues": ["AWAI-50", "JUN-384"],
    },
    "keyword-map-literal": {
        "refusal": "E2105 unsupported type ':a'",
        "what": "a keyword map literal such as {:a 1}; defrecord is admitted",
        "issues": ["JUN-651"],
        "detect": r"\{\s*:",
    },
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
    "dispatch-forms": {
        "refusal": "E2003 unsupported top-level form 'defmulti' / 'defprotocol'",
        "what": "defmulti, defmethod, defprotocol, extend-protocol, extend-type, reify",
        "issues": ["JUN-651"],
        "detect": r"\((defmulti|defmethod|defprotocol|extend-protocol|extend-type|reify)\b",
    },
    "destructuring": {
        "refusal": "E2111 malformed binding vector",
        "what": "sequential or map destructuring in let / fn / loop bindings",
        "issues": ["JUN-651"],
        "detect": r"\((let|loop|fn|defn-?|when-let|if-let|doseq|for)\b[^\[\]]*\[\s*[\[{]|\(fn\s+\[[^\]]*[\[{]|\(defn-?\s+\S+\s+(\"\"\s+)?\[[^\]]*[\[{]",
    },
    "atom-escape": {
        "refusal": "E2140 atom escapes its let scope (straight-line swap!/reset!/deref only)",
        "what": "an atom stored, returned, defined at top level or mutated inside a loop or closure",
        "issues": ["JUN-651"],
        "detect": r"\(def\s+\S+\s+\(atom\b|\(defonce\b|\(swap!|\(reset!",
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
                ".github/workflows/ci.yml"}
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


def ns_info(code):
    m = re.search(r"\(ns\s+(?:\^\S+\s+)?(" + SYM + ")", code)
    if not m:
        return None, []
    form = form_at(code, m.start())
    reqs = set()
    for r in re.finditer(r"\(:require\b", form):
        body = form_at(form, r.start())[len("(:require"):-1]
        depth = 0
        tok = re.compile(r"[\[\](){}]|#\?@?|" + SYM + r"|:[\w.\-/]+|\S")
        prev = None
        for t in tok.findall(body):
            if t in "([{":
                depth += 1
            elif t in ")]}":
                depth -= 1
            elif re.fullmatch(SYM, t) and "." in t:
                if depth == 0 or prev in ("[", "("):
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
    if parts[:2] == ["docs", "evidence"] or path in ("docs/migration-ledger.json", "docs/migration-ledger.md"):
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


def blockers_of(code, reqs):
    found = []
    for bid, b in BLOCKERS.items():
        if bid == "multi-module":
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
    """namespace -> {lib, kotoba?} from the kotoba monorepo's lib/ at rev."""
    names = subprocess.run(["git", "-C", str(kotoba), "ls-tree", "-r", "--name-only", rev, "lib"],
                           check=True, capture_output=True, text=True).stdout.split("\n")
    table = {}
    for p in names:
        m = re.match(r"lib/([^/]+)/src/(.+)\.(cljk|cljc|cljs|clj|kotoba)$", p)
        if not m:
            continue
        ns = m.group(2).replace("/", ".").replace("_", "-")
        e = table.setdefault(ns, {"lib": m.group(1), "kotoba": False})
        if m.group(3) == "kotoba":
            e["kotoba"] = True
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

    return {
        "schema": SCHEMA,
        "issue": "JUN-641",
        "inputs": {
            "kotoba-lib": {"repo": "kotoba-lang/kotoba", "rev": rev},
            "amu-probe": {"repo": "kotoba-lang/amu", "rev": "f7c07dff86d7fb5e89219a0f9ad4a83b2a83c82e",
                          "evidence": "docs/evidence/amu-capability-probe-20261031.json"},
        },
        "classes": CLASSES,
        "counts": {"tracked": len(files), "class": dict(sorted(counts.items())),
                   "port": dict(sorted(ported.items()))},
        "blockers": blockers,
        "waves": [{"wave": w, "units": sorted(wave_list[w])} for w in sorted(wave_list)],
        "cycles": cycles,
        "upstream": upstream,
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
    print(f"ledger {len(listed)} files, git ls-files {len(actual)}: {'ok' if ok else 'MISMATCH'}")
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
