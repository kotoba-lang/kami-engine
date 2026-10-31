#!/usr/bin/env python3
"""Probe which CLJK constructs the pinned Amu CLI admits (JUN-641).

    python3 scripts/amu_capability_probe.py ../amu/bin/amu docs/evidence/amu-capability-probe-YYYYMMDD.json

Each probe is one canonical .kotoba file. `blocker` names the entry of
BLOCKERS in scripts/migration_ledger.py that the probe measures (None for
probes that only record a rewrite rule or an admitted form). Re-run this when
the Amu pin in .github/workflows/ci.yml moves, and update BLOCKERS and the
evidence path in scripts/migration_ledger.py with it. The recorded amu revision
is the pin in ci.yml and the date comes from the YYYYMMDD in the output name.
"""
import hashlib, json, re, subprocess, sys, tempfile
from pathlib import Path
amu = sys.argv[1]; out = sys.argv[2]
CI = (Path(__file__).resolve().parent.parent / ".github/workflows/ci.yml").read_text()
PIN = re.search(r"repository:\s*kotoba-lang/amu\s*\n\s*ref:\s*([0-9a-f]{40})", CI).group(1)
_d = re.search(r"(\d{4})(\d{2})(\d{2})", Path(out).name)
DATE = "-".join(_d.groups()) if _d else None
P = [
 ("multi-module", "require", "(ns probe (:require [kotoba.lang.coll :as c]))\n\n(defn main [] :i64 0)\n"),
 ("keyword-map-literal", "let-map", "(defn main [] :i64 (let [m {:a 1}] (:a m)))"),
 ("keyword-map-literal", "fn-returns-map", "(defn mk [] {:a 1 :b 2})\n(defn main [] :i64 (:a (mk)))"),
 ("keyword-map-literal", "defrecord-admitted", "(defrecord P [x :i64 y :i64])\n(defn main [] :i64 (:y (->P 1 2)))"),
 ("f64-generic-arith", "infix-mul", "(defn main [] :i64 (let [x (* 1.5 2.0)] 0))"),
 ("f64-generic-arith", "infix-lt", "(defn main [] :i64 (if (< 1.5 2.0) 1 0))"),
 ("f64-generic-arith", "named-ops-admitted", "(defn main [] :i64 (let [x (f64-sqrt (f64-mul 1.5 2.0))] (if (f64-lt 1.5 x) 1 0)))"),
 ("f64-generic-arith", "f64-to-i64", "(defn main [] :i64 (f64-to-i64-checked 3.0))"),
 ("regex-literal", "re-find", "(defn main [] :i64 (if (re-find #\"a\" \"a\") 1 0))"),
 ("dispatch-forms", "defmulti", "(defmulti area :k)\n(defn main [] :i64 0)"),
 ("dispatch-forms", "defprotocol", "(defprotocol Shape (area [s]))\n(defn main [] :i64 0)"),
 ("destructuring", "let-vector", "(defn main [] :i64 (let [[a b] [1 2]] (+ a b)))"),
 ("atom-escape", "swap-in-loop", "(defn main [] :i64 (let [a (atom 0)] (dotimes [i 3] (swap! a + i)) @a))"),
 ("atom-escape", "straight-line-admitted", "(defn main [] :i64 (let [a (atom 0)] (swap! a inc) @a))"),
 ("host-interop", "js-global", "(defn main [] :i64 (js/Date.now))"),
 ("host-interop", "import-declaration", "(defimport now \"env\" \"now\" [] :i64)\n(defn main [] :i64 (now))"),
 (None, "set-literal-numbers", "(defn main [] :i64 (if (contains? #{1 2} 1) 1 0))"),
 (None, "defn-as-value", "(defn ap [f x :i64] :i64 (f x))\n(defn inc1 [x :i64] :i64 (+ x 1))\n(defn main [] :i64 (ap inc1 1))"),
 (None, "fn-literal-and-hof-admitted", "(defn main [] :i64 (reduce + 0 (map (fn [x] (+ x 1)) [1 2 3])))"),
 (None, "strings-and-vectors-admitted", "(defn main [] :i64 (+ (count [\"a\" \"b\"]) (count [[1 2] [3]]) (string-byte-length (str \"a\" \"b\"))))"),
 (None, "mixed-vector-literal", "(defn main [] :i64 (count [\"a\" [1 2]]))"),
]
rows = []
with tempfile.TemporaryDirectory() as d:
    for blocker, name, body in P:
        src = body if body.startswith("(ns ") else "(ns probe)\n\n" + body + "\n"
        f = Path(d) / f"{name}.kotoba"; f.write_text(src)
        chk = subprocess.run([amu, "check", str(f)], capture_output=True, text=True)
        row = {"blocker": blocker, "name": name, "source": src,
               "check": {"exit": chk.returncode, "output": (chk.stdout + chk.stderr).strip().splitlines()[-1:]}}
        if chk.returncode == 0:
            run = subprocess.run([amu, "run", str(f)], capture_output=True, text=True)
            row["run"] = {"exit": run.returncode, "output": (run.stdout + run.stderr).strip().splitlines()[-1:]}
        rows.append(row)
cli = Path(amu).parent.parent / "seed/bootstrap/aarch64-macos/amu-cli.macho"
doc = {"schema": "kami.amu-capability-probe.v1", "issue": "JUN-641", "date": DATE,
       "amu": {"repo": "kotoba-lang/amu", "rev": PIN,
               "entry": "bin/amu (macOS arm64)",
               "bin-amu-sha256": hashlib.sha256(Path(amu).read_bytes()).hexdigest(),
               "amu-cli-macho-sha256": hashlib.sha256(cli.read_bytes()).hexdigest()},
       "method": "each source is a single canonical .kotoba file; `amu check` then, when it passes, `amu run` (main prints its i64)",
       "probes": rows}
Path(out).write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n")
for r in rows: print(r["name"], r["check"]["exit"], r["check"]["output"], r.get("run", {}).get("output"))
