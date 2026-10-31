#!/usr/bin/env python3
"""Inventory the kotoba library APIs kami-engine uses (JUN-643).

For every kotoba `lib/` library kami-engine depends on -- named in a
deps.edn / nbb.edn manifest (`:deps/root "lib/<id>"` on kotoba.git) or reached
by a `(:require ...)` of a namespace that a library defines -- this lists the
kami units using it, the namespaces and vars kami code references, the
library's .cljk / .kotoba state, its kotoba lib dependencies (followed
transitively), a port order over them, the ledger waves each one blocks and the
issue keys kotoba already records for it. Both repositories are read from git
objects at the given revisions, never from a working tree, so the output only
changes when an input revision does.

    python3 scripts/upstream_apis.py --kotoba ../kotoba --kotoba-rev REV [--kami-rev REV]
        write docs/upstream-apis.json and docs/upstream-apis.md
    python3 scripts/upstream_apis.py --kotoba ../kotoba --kotoba-rev REV \\
            --amu-check ../amu/bin/amu --date YYYYMMDD
        run `amu check` on every src .kotoba file of those libraries and write
        docs/evidence/upstream-apis-amu-check-YYYYMMDD.json; the next plain run
        merges the newest such evidence into the inventory

Var references are found syntactically: the ns form's libspecs give each
required namespace its alias and :refer list, and `alias/var` or
`full.ns/var` tokens in the rest of the file (comments and string contents
removed) are the vars used. A :refer list counts as used as written; `:refer
:all` is resolved against the names the library defines. Macros, dynamic
`require` calls and host-specific reader branches are not distinguished.
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

sys.dont_write_bytecode = True  # importing migration_ledger must not leave scripts/__pycache__
sys.path.insert(0, str(Path(__file__).resolve().parent))
from migration_ledger import form_at, strip_code, unit_of, waves  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT_JSON = ROOT / "docs" / "upstream-apis.json"
OUT_MD = ROOT / "docs" / "upstream-apis.md"
EVIDENCE_DIR = ROOT / "docs" / "evidence"
EVIDENCE_GLOB = "upstream-apis-amu-check-*.json"
SCHEMA = "kami.upstream-apis.v1"
EVIDENCE_SCHEMA = "kami.upstream-apis-amu-check.v1"
KOTOBA_URL = "https://github.com/kotoba-lang/kotoba.git"

SRC_EXT = ("cljk", "cljc", "cljs", "clj", "kotoba")
ISSUE = re.compile(r"\b(?:JUN|KOTOBA|AWAI)-\d+\b")
# kotoba-wide documents that track library porting; a line counts for a
# library when it names the library id or one of the namespaces kami uses.
KOTOBA_TRACKING_DOCS = ("AGENTS.md", "README.md", "docs/library-native-compat.md", "docs/library-wave2.md",
                        "docs/library-repository-migration.md", "docs/native-cli-migration.md")
TEXT_EXT = (".md", ".edn", ".json", ".cljk", ".kotoba", ".yml", ".yaml", ".txt")


class Tree:
    """The blobs of one revision of a git repository."""

    def __init__(self, repo, rev):
        self.repo = Path(repo)
        self.rev = self._git("rev-parse", "--verify", rev + "^{commit}").strip()
        self.files = {}
        for line in self._git("ls-tree", "-r", "-z", "--full-tree", self.rev).split("\0"):
            if line:
                meta, path = line.split("\t", 1)
                mode, kind, sha = meta.split()
                if kind == "blob":
                    self.files[path] = (mode, sha)
        self._cache = {}

    def _git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True,
                              capture_output=True, text=True).stdout

    def read_many(self, paths):
        todo = [p for p in paths if p not in self._cache and p in self.files]
        if todo:
            req = "".join(self.files[p][1] + "\n" for p in todo).encode()
            out = subprocess.run(["git", "-C", str(self.repo), "cat-file", "--batch"], input=req,
                                 check=True, capture_output=True).stdout
            i = 0
            for p in todo:
                nl = out.index(b"\n", i)
                size = int(out[i:nl].split()[2])
                self._cache[p] = out[nl + 1:nl + 1 + size]
                i = nl + 1 + size + 1
        return {p: self._cache[p] for p in paths if p in self._cache}

    def text(self, path):
        data = self.read_many([path]).get(path)
        return None if data is None else data.decode("utf-8", errors="replace")


# ---------------------------------------------------------------- parsing

TOKEN = re.compile(r'[\[\](){}]|#\?@?|""|"REGEX"|:[^\s\[\](){},]+|[^\s\[\](){},"]+')
SYMCH = r"[^\s()\[\]{}\"',;@^`~\\]"


def tree_of(tokens, i=0, close=None):
    """Nested lists of tokens; a vector is ('[', items), a list ('(', items), a map ('{', items)."""
    items = []
    while i < len(tokens):
        t = tokens[i]
        if t in "([{":
            sub, i = tree_of(tokens, i + 1, {"(": ")", "[": "]", "{": "}"}[t])
            items.append((t, sub))
            continue
        if t == close:
            return items, i + 1
        if t not in ")]}":
            items.append(t)
        i += 1
    return items, i


def is_sym(t):
    return isinstance(t, str) and not t.startswith((":", '"', "#")) and t not in ("&", "'")


def libspecs(items, out):
    """Collect (ns, alias, refer) from the items of a :require body."""
    for it in items:
        if isinstance(it, str):
            if is_sym(it):
                out.append({"ns": it.lstrip("'"), "alias": None, "refer": []})
            continue
        kind, sub = it
        if kind == "[" and sub and is_sym(sub[0]):
            spec = {"ns": sub[0].lstrip("'"), "alias": None, "refer": []}
            k = 1
            while k + 1 < len(sub):
                key, val = sub[k], sub[k + 1]
                if key in (":as", ":as-alias") and is_sym(val):
                    spec["alias"] = val
                elif key in (":refer", ":refer-macros") and isinstance(val, tuple):
                    spec["refer"] += [v for v in val[1] if is_sym(v)]
                elif key == ":refer" and val == ":all":
                    spec["refer-all"] = True
                elif key == ":rename" and isinstance(val, tuple):
                    pairs = val[1]
                    spec["rename"] = {pairs[j]: pairs[j + 1] for j in range(0, len(pairs) - 1, 2)}
                k += 2
            out.append(spec)
        elif kind == "(":  # reader conditional body: #?(:clj [...] :cljs [...])
            libspecs(sub, out)
    return out


def parse_ns(code):
    """(ns name, [libspec], body without the ns form) of stripped source code."""
    m = re.search(r"\(ns\s+(?:\^\S+\s+)?(" + SYMCH + "+)", code)
    if not m:
        return None, [], code
    form = form_at(code, m.start())
    specs = []
    for r in re.finditer(r"\(:require(?:-macros)?\b", form):
        body = form_at(form, r.start())
        items, _ = tree_of(TOKEN.findall(body[1:-1])[1:])
        libspecs(items, specs)
    return m.group(1), specs, code[:m.start()] + code[m.start() + len(form):]


def qualified_uses(body, prefix):
    pat = r"(?<![\w.\-:/*+!?<>=&$%'#])" + re.escape(prefix) + r"/(" + SYMCH + r"+)"
    return {v for v in re.findall(pat, body) if not v.startswith("/")} - {""}


DEF = re.compile(r"\((def|defn|defn-|defmacro|defonce|defmulti|defprotocol|defrecord|deftype|defimport)"
                 r"\s+(?:\^\S+\s+)*(" + SYMCH + r"+)")


def defined_vars(code):
    """Public names a source defines; a .kotoba (:export [...]) list wins when present."""
    names = set()
    for kind, name in DEF.findall(code):
        if kind == "defn-" or "^:private" in name:
            continue
        names.add(name)
        if kind in ("defrecord", "deftype"):
            names.update({"->" + name, "map->" + name})
    for m in re.finditer(r"\(defprotocol\s+\S+", code):
        for meth in re.findall(r"\(\s*(" + SYMCH + r"+)\s+\[", form_at(code, m.start())):
            names.add(meth)
    exp = re.search(r"\(:export\s+\[([^\]]*)\]", code)
    if exp:
        return set(exp.group(1).split()), True
    return names, False


# ------------------------------------------------------------ kotoba side

def lib_index(kt):
    """Per-library sources, namespaces, manifest deps and text files at the kotoba revision."""
    libs = defaultdict(lambda: {"src": defaultdict(list), "files": []})
    for p in sorted(kt.files):
        parts = p.split("/")
        if parts[0] != "lib" or len(parts) < 3:
            continue
        lib = libs[parts[1]]
        lib["files"].append(p)
        m = re.match(r"lib/[^/]+/src/(.+)\.(" + "|".join(SRC_EXT) + ")$", p)
        if m:
            ns = m.group(1).replace("/", ".").replace("_", "-")
            lib["src"][ns].append({"path": p, "ext": m.group(2)})
    ns_table = defaultdict(list)
    for lid in sorted(libs):
        for ns in libs[lid]["src"]:
            ns_table[ns].append(lid)
    return libs, ns_table


def lib_deps(kt, lid, known):
    """kotoba lib ids a library's deps.edn names (:local/root ../<id> or :deps/root lib/<id>)."""
    text = kt.text(f"lib/{lid}/deps.edn") or ""
    code = strip_code_keep_strings(text)
    found, other = set(), set()
    for kind, val in re.findall(r':(local|deps)/root\s+"([^"]*)"', code):
        if kind == "local":
            name = val[3:] if val.startswith("../") and val.count("/") == 1 else None
            if name and name in known:
                found.add(name)
            elif "kotoba/lib/" in val and val.rsplit("/", 1)[-1] in known:
                found.add(val.rsplit("/", 1)[-1])
            else:
                other.add(val)
        elif val.startswith("lib/") and val[4:] in known:
            found.add(val[4:])
    found.discard(lid)
    return sorted(found), sorted(other)


def strip_code_keep_strings(src):
    """Drop ; comments outside strings, keep string contents (manifests need them)."""
    out, i, n, in_str = [], 0, len(src), False
    while i < n:
        c = src[i]
        if in_str:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(src[i + 1])
                i += 1
            elif c == '"':
                in_str = False
        elif c == '"':
            in_str = True
            out.append(c)
        elif c == ";":
            while i < n and src[i] != "\n":
                i += 1
            continue
        else:
            out.append(c)
        i += 1
    return "".join(out)


def manifest_libs(text, known):
    """kotoba lib ids a kami deps.edn / nbb.edn pulls from kotoba.git by :deps/root."""
    code = strip_code_keep_strings(text)
    found = set()
    for m in re.finditer(r':deps/root\s+"lib/([^"]+)"', code):
        if m.group(1) in known:
            found.add(m.group(1))
    return found


def tracking_hints(kt, lid, namespaces):
    """Issue keys kotoba records for a library: in its own files, and in the kotoba-wide porting docs."""
    hits = {}
    paths = [p for p in kt.files_in_lib[lid]
             if p.endswith(TEXT_EXT) and "/provenance/" not in p and "/hermes/" not in p]
    blobs = kt.read_many(paths)
    for p in paths:
        for n, line in enumerate(blobs[p].decode("utf-8", errors="replace").split("\n"), 1):
            for key in ISSUE.findall(line):
                hits.setdefault((key, p), {"key": key, "file": p, "line": n, "text": line.strip()[:160]})
    names = [lid] + sorted(namespaces)
    for p in KOTOBA_TRACKING_DOCS:
        text = kt.text(p)
        if text is None:
            continue
        for n, line in enumerate(text.split("\n"), 1):
            keys = ISSUE.findall(line)
            if keys and any(re.search(r"(?<![\w.\-])" + re.escape(x) + r"(?![\w\-]|\.\w)", line) for x in names):
                for key in keys:
                    hits.setdefault((key, p), {"key": key, "file": p, "line": n, "text": line.strip()[:160]})
    return sorted(hits.values(), key=lambda h: (h["file"], h["line"], h["key"]))


# -------------------------------------------------------------- kami side

def kami_sources(km):
    out = {}
    paths = [p for p in sorted(km.files) if p.endswith((".cljk", ".kotoba")) and km.files[p][0] != "160000"]
    for p, data in km.read_many(paths).items():
        out[p] = strip_code(data.decode("utf-8", errors="replace"))
    return out


def build(kotoba, kotoba_rev, kami_rev):
    km = Tree(ROOT, kami_rev)
    kt = Tree(kotoba, kotoba_rev)
    libs, ns_table = lib_index(kt)
    kt.files_in_lib = {lid: libs[lid]["files"] for lid in libs}
    known = set(libs)
    ledger = json.loads(km.text("docs/migration-ledger.json"))
    unit_wave = {u["unit"]: u.get("wave") for u in ledger["units"]}
    placement = json.loads(km.text("docs/migration-placement.json"))["units"]

    # manifests
    manifest_units = defaultdict(set)
    mpaths = [p for p in km.files if p.rsplit("/", 1)[-1] in ("deps.edn", "nbb.edn")]
    for p, data in km.read_many(mpaths).items():
        for lid in manifest_libs(data.decode("utf-8", errors="replace"), known):
            manifest_units[lid].add(unit_of(p))

    # requires and var references
    srcs = kami_sources(km)
    parsed = {p: parse_ns(code) for p, code in srcs.items()}
    own = {ns for ns, _, _ in parsed.values() if ns}
    uses = defaultdict(lambda: defaultdict(lambda: {"vars": set(), "units": set(), "files": set(),
                                                    "refer-all": set()}))
    unresolved = defaultdict(set)
    ambiguous = {}
    for p in sorted(parsed):
        ns, specs, body = parsed[p]
        unit = unit_of(p)
        for s in specs:
            q = s["ns"]
            if q in own:
                continue
            cands = ns_table.get(q, [])
            if not cands:
                unresolved[q].add(unit)
                continue
            declared = [c for c in cands if unit in manifest_units.get(c, ())]
            with_cljk = [c for c in cands if any(f["ext"] != "kotoba" for f in libs[c]["src"][q])]
            lid = (declared or with_cljk or cands)[0]
            if len(cands) > 1:
                ambiguous[q] = cands
            rec = uses[lid][q]
            rec["units"].add(unit)
            rec["files"].add(p)
            vs = set(s["refer"]) | qualified_uses(body, q)
            if s["alias"]:
                vs |= qualified_uses(body, s["alias"])
            rec["vars"] |= vs
            if s.get("refer-all"):
                rec["refer-all"].add(p)
                rec.setdefault("_bodies", []).append(body)

    # library records
    def ns_sources(lid, q):
        return {f["ext"]: f["path"] for f in libs[lid]["src"].get(q, [])}

    def defs_of(path):
        code = strip_code(kt.text(path))
        return defined_vars(code)

    direct = set(uses) | set(manifest_units)
    deps_of, other_deps = {}, {}
    todo = sorted(direct)
    while todo:
        lid = todo.pop()
        if lid in deps_of:
            continue
        deps_of[lid], other_deps[lid] = lib_deps(kt, lid, known)
        todo += [d for d in deps_of[lid] if d not in deps_of]
    closure = sorted(deps_of)

    level, cycles = waves(closure, deps_of)
    dependents = defaultdict(set)  # lib -> libs that (transitively) need it, itself included

    def reach(lid, seen):
        for d in deps_of[lid]:
            if d not in seen:
                seen.add(d)
                reach(d, seen)
        return seen

    for lid in closure:
        for d in reach(lid, {lid}):
            dependents[d].add(lid)

    evidence = latest_evidence()
    if evidence and evidence["inputs"]["kotoba-lib"]["rev"] != kt.rev:
        print(f"{evidence['_path']} checked kotoba {evidence['inputs']['kotoba-lib']['rev'][:12]}, not "
              f"{kt.rev[:12]}; not merged (re-run with --amu-check)", file=sys.stderr)
        evidence = None
    checks = {}
    if evidence:
        for row in evidence["checks"]:
            checks[row["path"]] = row

    records = []
    for lid in closure:
        src = libs[lid]["src"]
        counts = defaultdict(int)
        for files in src.values():
            for f in files:
                counts[f["ext"]] += 1
        nss = []
        for q in sorted(uses.get(lid, {})):
            r = uses[lid][q]
            have = ns_sources(lid, q)
            cljk_path = next((have[e] for e in ("cljk", "cljc", "cljs", "clj") if e in have), None)
            cljk_defs = defs_of(cljk_path)[0] if cljk_path else set()
            vars_ = set(r["vars"])
            for body in r.get("_bodies", []):
                vars_ |= {v for v in cljk_defs if re.search(r"(?<![\w.\-:/*+!?<>=&$%])" + re.escape(v)
                                                            + r"(?![\w.\-/*+!?<>=&$%])", body)}
            row = {"ns": q, "vars": sorted(vars_), "units": sorted(r["units"]), "files": len(r["files"]),
                   "cljk": cljk_path, "kotoba": have.get("kotoba")}
            if r["refer-all"]:
                row["refer-all-files"] = sorted(r["refer-all"])
            if cljk_path:
                row["vars-not-defined-in-cljk"] = sorted(vars_ - cljk_defs)
            if have.get("kotoba"):
                kdefs, exported = defs_of(have["kotoba"])
                row["kotoba-missing-vars"] = sorted(vars_ - kdefs)
                row["kotoba-export-list"] = exported
                if have["kotoba"] in checks:
                    row["amu-check"] = status_of(checks[have["kotoba"]])
            elsewhere = [c for c in ns_table[q] if c != lid and "kotoba" in ns_sources(c, q)]
            if elsewhere:
                row["kotoba-in-other-libs"] = [
                    dict({"lib": c, "path": ns_sources(c, q)["kotoba"]},
                         **({"amu-check": status_of(checks[ns_sources(c, q)["kotoba"]])}
                            if ns_sources(c, q)["kotoba"] in checks else {}))
                    for c in elsewhere]
            if q in ambiguous:
                row["defined-in-libs"] = ambiguous[q]
            nss.append(row)
        users = sorted({u for r in uses.get(lid, {}).values() for u in r["units"]})
        need_units = sorted({u for d in dependents[lid] for r in uses.get(d, {}).values() for u in r["units"]}
                            | {u for d in dependents[lid] for u in manifest_units.get(d, ())})
        wave_set = sorted({unit_wave[u] for u in need_units if unit_wave.get(u) is not None})
        kot_paths = sorted(f["path"] for files in src.values() for f in files if f["ext"] == "kotoba")
        checked = [status_of(checks[p]) for p in kot_paths if p in checks]
        rec = {
            "lib": lid,
            "transitive": lid not in direct,
            "manifest-units": sorted(manifest_units.get(lid, ())),
            "require-units": users,
            "required-by-libs": sorted(dependents[lid] - {lid}),
            "src": {"cljk": sum(counts[e] for e in ("cljk", "cljc", "cljs", "clj")),
                    "kotoba": counts["kotoba"]},
            "vars-used": sum(len(n["vars"]) for n in nss),
            "namespaces": nss,
            "deps": deps_of[lid],
            "port-level": level[lid],
            "blocks-units": need_units,
            "blocks-waves": wave_set,
            "tracking": tracking_hints(kt, lid, [n["ns"] for n in nss]),
        }
        if other_deps[lid]:
            rec["deps-outside-lib"] = other_deps[lid]
        if checked:
            rec["amu-check"] = {"files": len(kot_paths), "checked": len(checked),
                                "pass": sum(1 for c in checked if c == "pass"),
                                "refusals": sorted({c for c in checked if c != "pass"})}
        key = "kotoba:lib/" + lid
        if key in placement:
            rec["placement"] = placement[key]["to"]
        records.append(rec)

    order = defaultdict(list)
    for lid in closure:
        order[level[lid]].append(lid)
    out = {
        "schema": SCHEMA,
        "issue": "JUN-643",
        "inputs": {
            "kami-engine": {"repo": "kotoba-lang/kami-engine", "rev": km.rev},
            "kotoba-lib": {"repo": "kotoba-lang/kotoba", "rev": kt.rev},
            "ledger": {"path": "docs/migration-ledger.json", "kotoba-lib-rev": ledger["inputs"]["kotoba-lib"]["rev"]},
        },
        "method": (__doc__ or "").split("\n\n")[-1].replace("\n", " ").strip(),
        "counts": {"libs": len(closure), "direct": len(direct), "transitive": len(closure) - len(direct),
                   "namespaces": sum(len(r["namespaces"]) for r in records),
                   "vars": sum(r["vars-used"] for r in records)},
        "port-order": [{"level": k, "libs": sorted(order[k])} for k in sorted(order)],
        "cycles": cycles,
        "libs": records,
        "unresolved-namespaces": {q: sorted(u) for q, u in sorted(unresolved.items())},
    }
    if evidence:
        out["inputs"]["amu-check"] = {"evidence": evidence["_path"], "amu-rev": evidence["amu"]["rev"],
                                      "bin-amu-sha256": evidence["amu"]["bin-amu-sha256"]}
    return out


# ----------------------------------------------------------- amu evidence

def status_of(row):
    if row["exit"] == 0:
        return "pass"
    return ",".join(row["codes"]) or f"exit {row['exit']}"


def latest_evidence():
    found = sorted(EVIDENCE_DIR.glob(EVIDENCE_GLOB))
    if not found:
        return None
    doc = json.loads(found[-1].read_text(encoding="utf-8"))
    doc["_path"] = str(found[-1].relative_to(ROOT))
    return doc


def amu_pin(km):
    ci = km.text(".github/workflows/ci.yml") or ""
    m = re.search(r"repository:\s*kotoba-lang/amu\s*\n\s*ref:\s*([0-9a-f]{40})", ci)
    return m.group(1) if m else None


def amu_check(kotoba, kotoba_rev, kami_rev, amu, date):
    inv = build(kotoba, kotoba_rev, kami_rev)
    km = Tree(ROOT, kami_rev)
    kt = Tree(kotoba, kotoba_rev)
    amu = Path(amu).resolve()
    seed = amu.parent  # bin/amu resolves into seed/bootstrap/aarch64-macos
    meta = json.loads((seed / "cli.json").read_text())
    image_sha = hashlib.sha256(amu.read_bytes()).hexdigest()
    compiler_sha = hashlib.sha256((seed / Path(meta["compiler"]["path"]).name).read_bytes()).hexdigest()
    if image_sha != meta["image"]["sha256"] or compiler_sha != meta["compiler"]["sha256"]:
        raise SystemExit(f"{amu}: sha256 does not match its cli.json; refusing to record evidence")
    paths = set()
    libs, _ = lib_index(kt)
    for r in inv["libs"]:
        for files in libs[r["lib"]]["src"].values():
            paths.update(f["path"] for f in files if f["ext"] == "kotoba")
        for n in r["namespaces"]:
            for o in n.get("kotoba-in-other-libs", []):
                paths.add(o["path"])
    rows = []
    with tempfile.TemporaryDirectory() as d:
        for p in sorted(paths):
            f = Path(d) / p
            f.parent.mkdir(parents=True, exist_ok=True)
            f.write_bytes(kt.read_many([p])[p])
            try:
                res = subprocess.run([str(amu), "check", str(f)], capture_output=True, text=True, timeout=300)
                exit_code, output = res.returncode, (res.stdout + res.stderr).replace(d + "/", "")
            except subprocess.TimeoutExpired:
                exit_code, output = None, "timeout after 300 s"
            lines = [x for x in output.strip().splitlines() if x.strip()]
            rows.append({"lib": p.split("/")[1], "path": p, "exit": exit_code,
                         "codes": sorted(set(re.findall(r"\bE\d{4}\b", output))),
                         "output": lines[-3:]})
            print(p, exit_code, lines[-1:] if lines else "")
    doc = {
        "schema": EVIDENCE_SCHEMA,
        "issue": "JUN-643",
        "date": f"{date[:4]}-{date[4:6]}-{date[6:]}",
        "inputs": {"kotoba-lib": {"repo": "kotoba-lang/kotoba", "rev": kt.rev},
                   "kami-engine": {"repo": "kotoba-lang/kami-engine", "rev": km.rev}},
        "amu": {"repo": "kotoba-lang/amu", "rev": amu_pin(km), "pinned-in": ".github/workflows/ci.yml",
                "entry": "bin/amu (macOS arm64)", "bin-amu-sha256": image_sha, "compiler-sha256": compiler_sha,
                "cli-json": {"image": meta["image"]["sha256"], "compiler": meta["compiler"]["sha256"]}},
        "method": ("each src .kotoba file of every library in docs/upstream-apis.json (and each .kotoba twin "
                   "of a kami-used namespace found in another library) is written from the kotoba revision's "
                   "git object to a temporary directory at its lib/ path and checked on its own: "
                   "`amu check <file>`. The CLI compiles one self-contained file, so a file that requires "
                   "another module is refused with E6002 rather than checked."),
        "command": "amu check lib/<id>/src/<path>.kotoba",
        "checks": rows,
    }
    out = EVIDENCE_DIR / f"upstream-apis-amu-check-{date}.json"
    out.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT)}: {sum(1 for r in rows if r['exit'] == 0)}/{len(rows)} pass")


# ---------------------------------------------------------------- summary

def summary(inv):
    k = inv["inputs"]
    ev = k.get("amu-check")
    out = [
        "# Upstream kotoba library APIs used by kami-engine",
        "",
        "Generated by `scripts/upstream_apis.py` from `docs/upstream-apis.json` (JUN-643). Do not",
        f"edit by hand. kami-engine read at `{k['kami-engine']['rev'][:12]}`, kotoba `lib/` at",
        f"`{k['kotoba-lib']['rev'][:12]}`; waves from `docs/migration-ledger.json`.",
        ("`amu check` from `" + ev["evidence"] + f"` (amu `{(ev['amu-rev'] or '?')[:12]}`, bin/amu sha256 "
         f"`{ev['bin-amu-sha256'][:16]}…`).") if ev else "No `amu check` evidence merged.",
        "",
        f"{inv['counts']['libs']} libraries ({inv['counts']['direct']} direct, {inv['counts']['transitive']} "
        f"transitive), {inv['counts']['namespaces']} namespaces, {inv['counts']['vars']} vars.",
        "",
        "Columns: `kami users` are the kami units that require the library (manifest-only users in",
        "parentheses); `vars` the distinct vars referenced; `src cljk/kotoba` the files in the library's",
        "`src/`; `ns .kotoba` how many of the used namespaces have a `.kotoba` in that library; `amu check`",
        "passing src `.kotoba` files / checked, with the refusal codes; `level` the port order level.",
        "",
        "| lib | transitive | kami users | vars | src cljk/kotoba | ns .kotoba | amu check | level | waves | issues |",
        "|---|---|---|---:|---|---|---|---:|---|---|",
    ]
    for r in sorted(inv["libs"], key=lambda r: (r["port-level"], r["lib"])):
        users = len(r["require-units"])
        extra = len(set(r["manifest-units"]) - set(r["require-units"]))
        u = f"{users}" + (f" (+{extra})" if extra else "")
        nk = f"{sum(1 for n in r['namespaces'] if n['kotoba'])}/{len(r['namespaces'])}" if r["namespaces"] else "-"
        a = r.get("amu-check")
        amu = "-" if not a else f"{a['pass']}/{a['checked']}" + (f" {' '.join(a['refusals'])}" if a["refusals"] else "")
        keys = sorted({h["key"] for h in r["tracking"]}, key=lambda x: (x.split("-")[0], int(x.split("-")[1])))
        out.append(f"| {r['lib']} | {'yes' if r['transitive'] else ''} | {u} | {r['vars-used']} | "
                   f"{r['src']['cljk']}/{r['src']['kotoba']} | {nk} | {amu} | {r['port-level']} | "
                   f"{','.join(map(str, r['blocks-waves']))} | {', '.join(keys)} |")
    out += ["", "## Port order", "",
            "Level n libraries depend only on libraries of lower levels (kotoba `deps.edn` `:local/root`",
            "or `:deps/root` edges inside `lib/`)."]
    out += [""] + [f"- **level {o['level']}**: " + ", ".join(f"`{x}`" for x in o["libs"]) for o in inv["port-order"]]
    if inv["cycles"]:
        out += ["", "Cycles (ported together): " + "; ".join(", ".join(g) for g in inv["cycles"])]
    w0 = [r["lib"] for r in inv["libs"] if r["src"]["kotoba"] == 0 and 0 in r["blocks-waves"]]
    out += ["", "## Needed by wave 0 with no `.kotoba` source", "", ", ".join(f"`{x}`" for x in w0) or "none"]
    out += ["", "## Namespaces and vars", ""]
    for r in sorted(inv["libs"], key=lambda r: r["lib"]):
        for n in r["namespaces"]:
            k = "no .kotoba"
            if n["kotoba"]:
                miss = n.get("kotoba-missing-vars", [])
                k = f"`.kotoba` {n.get('amu-check', 'unchecked')}" + (
                    f", missing {', '.join(miss)}" if miss else ", covers every used var")
            elif n.get("kotoba-in-other-libs"):
                k += " here; .kotoba in " + ", ".join(
                    f"`{o['lib']}` ({o.get('amu-check', 'unchecked')})" for o in n["kotoba-in-other-libs"])
            vs = " ".join(n["vars"]) or "(namespace only)"
            out.append(f"- `{n['ns']}` ({r['lib']}; {len(n['units'])} units) {k}: {vs}")
    out += ["", "## Tracking keys in kotoba", ""]
    for r in sorted(inv["libs"], key=lambda r: r["lib"]):
        for h in r["tracking"]:
            out.append(f"- {r['lib']}: {h['key']} — `{h['file']}:{h['line']}`")
    out += ["", "## Unresolved namespaces", "",
            "Required by kami code but neither defined in this repository nor in kotoba `lib/` src:", ""]
    out += [f"- `{q}`: {', '.join(u)}" for q, u in inv["unresolved-namespaces"].items()]
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    ap.add_argument("--kotoba", type=Path, required=True, help="a kotoba-lang/kotoba checkout")
    ap.add_argument("--kotoba-rev", required=True, help="the kotoba revision to read lib/ from")
    ap.add_argument("--kami-rev", default="HEAD", help="the kami-engine revision to read (default HEAD)")
    ap.add_argument("--amu-check", metavar="AMU", help="run `amu check` with this pinned CLI and write evidence")
    ap.add_argument("--date", help="YYYYMMDD for the evidence file name (with --amu-check)")
    a = ap.parse_args()
    if a.amu_check:
        if not a.date or not re.fullmatch(r"\d{8}", a.date):
            ap.error("--amu-check needs --date YYYYMMDD")
        amu_check(a.kotoba, a.kotoba_rev, a.kami_rev, a.amu_check, a.date)
        return 0
    inv = build(a.kotoba, a.kotoba_rev, a.kami_rev)
    OUT_JSON.write_text(json.dumps(inv, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    OUT_MD.write_text(summary(inv), encoding="utf-8")
    print(json.dumps(inv["counts"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
