#!/usr/bin/env python3
"""Survey who requires each kami unit's namespaces (JUN-649).

    python3 scripts/placement_consumers.py --forest ~/github --kotoba-rev origin/main \
        --output docs/evidence/placement-consumers-YYYYMMDD.json

A unit is a kami-engine ledger unit or a kotoba `lib/kotoba.kami*` library
(plus `lib/kotoba.host`). A consumer is a file whose `(ns ... (:require ...))`
names one of the unit's namespaces, found in three places:

- this repository (its CLJK oracle files, from docs/migration-ledger.json);
- the kotoba monorepo `lib/` at --kotoba-rev (read from git, not the work tree);
- every other checkout under --forest, as it is on disk. Advisory forks
  (`*-ghsa-*`), worktree copies, wt/ and workspaces/ are skipped.

Consumer repositories are then labelled: `copy` for an old standalone repo that
kotoba `lib/` or `packages/` imported (its code is the unit itself or a lib),
`compat-twin` for kotoba-lang/kotoba-lang `lang/compat` (bulk mechanical
.kotoba copies), `game` for the game repositories, `kami-engine` and
`kotoba-lib` for those monorepos, and `other` for the rest (non-game uses).
"""

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import migration_ledger as ml  # noqa: E402

GAMES = {"network-awai/network-isekai", "com-junkawasaki/ghosthacker"}
# Local checkouts under an old repository name, whose imports.json entry uses
# the newer one. kotoba-lang/engine's README says it moved; it is the
# kami-engine-guest compiler.
ALIASES = {"kotoba-lang/engine": "kotoba:lib/kotoba.kamiEngineGuest"}
SKIP = re.compile(r"-ghsa-|worktrees|-wt$")
SOURCE = re.compile(r"\.(cljk|cljc|cljs|clj|kotoba)$")


def git(*args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout


def lib_files(kotoba, rev):
    return [p for p in git("ls-tree", "-r", "--name-only", rev, "lib", cwd=kotoba).split("\n") if p]


def blobs(kotoba, rev, paths):
    proc = subprocess.Popen(["git", "-C", str(kotoba), "cat-file", "--batch"],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    for p in paths:
        proc.stdin.write(f"{rev}:{p}\n".encode())
        proc.stdin.flush()
        head = proc.stdout.readline().split()
        if head[-1] == b"missing":
            continue
        data = proc.stdout.read(int(head[2]))
        proc.stdout.read(1)
        yield p, data.decode("utf-8", "replace")
    proc.stdin.close()
    proc.wait()


def ns_of_path(p):
    m = re.match(r"lib/([^/]+)/(?:src|test)/(.+)\.(cljk|cljc|cljs|clj|kotoba)$", p)
    return (m.group(1), m.group(2).replace("/", ".").replace("_", "-")) if m else (None, None)


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    ap.add_argument("--forest", type=Path, required=True)
    ap.add_argument("--kotoba", type=Path, help="default: FOREST/kotoba-lang/kotoba")
    ap.add_argument("--kotoba-rev", default="origin/main")
    ap.add_argument("--output", type=Path, required=True)
    a = ap.parse_args()
    kotoba = a.kotoba or a.forest / "kotoba-lang" / "kotoba"
    rev = git("rev-parse", a.kotoba_rev, cwd=kotoba).strip()

    ledger = json.loads((ROOT / "docs" / "migration-ledger.json").read_text())
    units = {}
    for u in ledger["units"]:
        units["kami-engine:" + u["unit"]] = set(u.get("namespaces", []))
    paths = lib_files(kotoba, rev)
    for p in paths:
        lib, ns = ns_of_path(p)
        if lib and (lib.startswith("kotoba.kami") or lib == "kotoba.host"):
            units.setdefault("kotoba:lib/" + lib, set()).add(ns)
    owner = defaultdict(set)
    for u, nss in units.items():
        for n in nss:
            owner[n].add(u)

    # Old standalone repos imported into lib/ or packages/: their files are copies.
    imported = {}
    for p in paths:
        if re.fullmatch(r"lib/[^/]+/imports\.json", p):
            for i in json.loads(git("show", f"{rev}:{p}", cwd=kotoba)).get("imports", []):
                imported["kotoba-lang/" + i["repo"]] = "kotoba:lib/" + p.split("/")[1]
    for f in ROOT.glob("packages/*/imports.json"):
        for i in json.loads(f.read_text()).get("imports", []):
            imported["kotoba-lang/" + i["repo"]] = "kami-engine:packages/" + f.parent.name

    imported.update(ALIASES)
    hits = defaultdict(lambda: defaultdict(list))

    def add(repo, path, text, self_unit=None):
        _, reqs = ml.ns_info(ml.strip_code(text))
        for q in reqs:
            for u in owner.get(q, ()):
                if u != self_unit and path not in hits[u][repo]:
                    hits[u][repo].append(path)

    for f in ledger["files"]:
        if f["path"].endswith(".cljk") and f["class"] == "o":
            add("kami-engine:" + f["unit"], f["path"],
                (ROOT / f["path"]).read_text(encoding="utf-8", errors="replace"), "kami-engine:" + f["unit"])
    for p, text in blobs(kotoba, rev, [p for p in paths if SOURCE.search(p) and ns_of_path(p)[0]]):
        add("kotoba:lib/" + p.split("/")[1], p, text, "kotoba:lib/" + p.split("/")[1])

    pattern = "\n".join(sorted(owner)) + "\n"
    rg = subprocess.run(["rg", "-l", "-F", "-f", "-", "-g", "*.{cljk,cljc,cljs,clj,kotoba}",
                         "-g", "!wt/**", "-g", "!workspaces/**", "-g", "!**/node_modules/**",
                         "-g", "!kotoba-lang/kotoba/**", "-g", "!kotoba-lang/kami-engine/**", "."],
                        cwd=a.forest, input=pattern, capture_output=True, text=True)
    skipped = defaultdict(int)
    for p in rg.stdout.split("\n"):
        if not p:
            continue
        p = p.removeprefix("./")
        repo = "/".join(p.split("/")[:2])
        if SKIP.search(repo):
            skipped[repo] += 1
            continue
        add(repo, p, (a.forest / p).read_text(encoding="utf-8", errors="replace"))

    def kind(repo):
        if repo.startswith("kami-engine:"):
            return "kami-engine"
        if repo.startswith("kotoba:lib/"):
            return "kotoba-lib"
        if repo == "kotoba-lang/kotoba-lang":
            return "compat-twin"
        if repo in imported:
            return "copy"
        return "game" if repo in GAMES else "other"

    out = {}
    for u in sorted(units):
        rows = {}
        for repo, files in sorted(hits[u].items()):
            row = {"kind": kind(repo), "files": len(files), "sample": sorted(files)[:3]}
            if row["kind"] == "copy":
                row["copy-of"] = imported[repo]
            rows[repo] = row
        out[u] = rows
    doc = {"schema": "kami.placement-consumers.v1", "issue": "JUN-649",
           "kotoba": {"repo": "kotoba-lang/kotoba", "rev": rev},
           "method": (__doc__ or "").split("\n\n", 2)[2].strip(),
           "skipped-repositories": dict(sorted(skipped.items())),
           "units": out}
    a.output.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{len(out)} units, {sum(len(v) for v in out.values())} consumer rows -> {a.output}")


if __name__ == "__main__":
    main()
