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
 (None, "let-map", "(defn main [] :i64 (let [m {:a 1}] (:a m)))"),
 (None, "fn-returns-map", "(defn mk [] {:a 1 :b 2})\n(defn main [] :i64 (:a (mk)))"),
 (None, "defrecord-admitted", "(defrecord P [x :i64 y :i64])\n(defn main [] :i64 (:y (->P 1 2)))"),
 # JUN-728 (amu c2186d7, PR #1750): a keyword map literal is a closed record; these record what it still refuses,
 # each under the JUN-732 kwmap/* blocker whose port removes it (if-different-keys: an absent key is an [:option T] field).
 (None, "assoc-present-key-admitted", "(defn main [] :i64 (:a (assoc {:a 1 :b 2} :a 5)))"),
 ("kwmap/row-op", "assoc-new-key", "(defn main [] :i64 (:b (assoc {:a 1} :b 2)))"),
 ("kwmap/runtime-key", "absent-key", "(defn main [] :i64 (:z {:a 1}))"),
 ("kwmap/row-op", "dissoc", "(defn main [] :i64 (:b (dissoc {:a 1 :b 2} :a)))"),
 ("kwmap/row-op", "merge", "(defn main [] :i64 (:b (merge {:a 1} {:b 2})))"),
 ("kwmap/row-op", "update", "(defn main [] :i64 (:a (update {:a 1} :a inc)))"),
 ("kwmap/option-default", "get-default", "(defn main [] :i64 (get {:a 1} :a 0))"),
 ("kwmap/runtime-key", "contains-key", "(defn main [] :i64 (if (contains? {:a 1} :a) 1 0))"),
 ("kwmap/dynamic", "mixed-keys", "(defn main [] :i64 (let [m {:a 1 \"b\" 2}] 0))"),
 ("kwmap/empty", "empty-map", "(defn main [] :i64 (let [m {}] 0))"),
 ("kwmap/nil-field", "nil-value", "(defn main [] :i64 (let [m {:a nil :b 1}] (:b m)))"),
 ("kwmap/fn-value", "fn-value", "(defn main [] :i64 ((:f {:f (fn [x] x)}) 1))"),
 ("kwmap/param-row", "unannotated-param", "(defn f [m] :i64 (:a m))\n(defn main [] :i64 (f {:a 1}))"),
 ("kwmap/param-row", "declared-record-expected", "(defrecord P [a :i64])\n(defn f [p P] :i64 (:a p))\n(defn main [] :i64 (f {:a 1}))"),
 ("kwmap/nil-field", "if-different-keys", "(defn main [] :i64 (let [m (if true {:a 1} {:a 1 :b 2})] (:a m)))"),
 ("kwmap/param-row", "map-over-records", "(defn main [] :i64 (reduce + 0 (map :a [{:a 1} {:a 2}])))"),
 # JUN-732: one probe per kwmap/* blocker not covered above, and the reader shorthands found by its survey.
 (None, "def-f64-map", "(def x {:a 1.5 :n 1})\n(defn main [] :i64 (:n x))"),
 (None, "def-vec-of-maps", "(def x [{:a 1} {:a 2}])\n(defn main [] :i64 (:a (nth x 1)))"),
 (None, "def-ctor-f64-admitted", "(defrecord D [a :f64])\n(def x (->D 0.5))\n(defn main [] :i64 (if (f64-lt 0.1 (:a x)) 1 0))"),
 ("kwmap/table-runtime-key", "table-runtime-key", "(def t {:a 1 :b 2})\n(defn f [k :keyword] :i64 (get t k))\n(defn main [] :i64 (f :b))"),
 (None, "typed-map-runtime-key-admitted", "(def t (typed-map-new [:map :keyword :i64] :a 1 :b 2))\n(defn f [k :keyword] :i64 (get t k -1))\n(defn main [] :i64 (f :b))"),
 ("kwmap/runtime-key", "keys-of-keyword-typed-map", "(defn main [] :i64 (count (keys (typed-map-new [:map :keyword :i64] :a 1 :b 2))))"),
 ("kwmap/option-default", "keys-or-param", "(defn f [{:keys [a] :or {a 1}}] (+ a 1))\n(defn main [] :i64 (f {:a 2}))"),
 ("kwmap/dynamic", "read-string", "(defn main [] :i64 (:a (read-string \"{:a 1}\")))"),
 (None, "assoc-variadic", "(defn main [] :i64 (:b (assoc {:a 1 :b 2} :a 3 :b 4)))"),
 (None, "ex-info-data-admitted", "(defn- f [i :i64] :i64 (if (< i 0) (throw (ex-info \"neg\" {:index i})) i))\n(defn main [] :i64 (try (f -1) (catch e 0)))"),
 ("fn-literal", "hash-fn", "(defn main [] :i64 (reduce + 0 (map #(+ % 1) [1 2])))"),
 ("auto-resolved-keyword", "double-colon", "(defn main [] :i64 (if (= ::a :probe/a) 1 0))"),
 (None, "qualified-keyword-admitted", "(defn main [] :i64 (if (= :probe/a :probe/a) 1 0))"),
 (None, "f64-record-field-admitted", "(defrecord V [x :f64])\n(defn main [] :i64 (if (f64-lt 1.0 (:x (->V 1.5))) 1 0))"),
 ("f64-generic-arith", "infix-mul", "(defn main [] :i64 (let [x (* 1.5 2.0)] 0))"),
 ("f64-generic-arith", "infix-lt", "(defn main [] :i64 (if (< 1.5 2.0) 1 0))"),
 ("f64-generic-arith", "named-ops-admitted", "(defn main [] :i64 (let [x (f64-sqrt (f64-mul 1.5 2.0))] (if (f64-lt 1.5 x) 1 0)))"),
 ("f64-generic-arith", "f64-to-i64", "(defn main [] :i64 (f64-to-i64-checked 3.0))"),
 ("regex-literal", "re-find", "(defn main [] :i64 (if (re-find #\"a\" \"a\") 1 0))"),
 ("dispatch-forms", "defmulti", "(defmulti area :k)\n(defn main [] :i64 0)"),
 ("dispatch-forms", "defprotocol", "(defprotocol Shape (area [s]))\n(defn main [] :i64 0)"),
 (None, "let-vector", "(defn main [] :i64 (let [[a b] [1 2]] (+ a b)))"),
 # JUN-729 (amu 5667e44c, PR #1751): destructuring is reader sugar for the plain temp-bound form in let / loop / defn
 # parameters. What it still refuses, and the admitted forms the ledger's narrowed `destructuring` detect relies on.
 ("destructuring", "fn-param-pattern", "(defn main [] :i64 (reduce (fn [acc [a b]] (+ acc (* a b))) 0 [[1 2] [3 4]]))"),
 ("destructuring", "defn-param-pattern-unannotated", "(defn f [[a b]] (+ a b))\n(defn main [] :i64 (f [1 2]))"),
 ("destructuring", "defn-keys-param-unannotated", "(defn f [{:keys [a]}] :i64 a)\n(defn main [] :i64 (f {:a 3}))"),
 (None, "defn-param-pattern-annotated-admitted", "(defn f [[a b] :vector-i64] :i64 (+ a b))\n(defn main [] :i64 (f [1 2]))"),
 ("destructuring", "keys-strs", "(defn main [] :i64 (let [{:strs [a]} {:a 1}] a))"),
 ("destructuring", "keys-syms", "(defn main [] :i64 (let [{:syms [a]} {:a 1}] a))"),
 ("destructuring", "string-key", "(defn main [] :i64 (let [{a \"a\"} {:a 1}] a))"),
 ("destructuring", "rest-over-list", "(defn main [] :i64 (let [[a & r] (list 1 2 3)] a))"),
 ("destructuring", "keys-or-record", "(defrecord P [a :i64])\n(defn main [] :i64 (let [{:keys [a] :or {a 5}} (->P 1)] (+ a (:a (->P 0)))))"),
 ("destructuring", "keys-absent-key", "(defn main [] :i64 (let [{:keys [a b]} {:a 1}] (+ a (:b {:b 1}))))"),
 ("destructuring", "keys-unspelled-keyword", "(defrecord P [a :i64 b :i64])\n(defn main [] :i64 (let [{:keys [a b]} (->P 1 2)] (+ a b)))"),
 ("destructuring", "doseq-pattern", "(defn main [] :i64 (do (doseq [[a b] [[1 2]]] (+ a b)) 0))"),
 ("destructuring", "for-pattern", "(defn main [] :i64 (count (for [[a b] [[1 2]]] (+ a b))))"),
 ("destructuring", "when-let-pattern", "(defn main [] :i64 (let [r (when-let [[a b] [1 2]] (+ a b))] 0))"),
 (None, "let-rest-as-nested-admitted", "(defn main [] :i64 (let [[x y & more :as all] [1 2 3 4] [[p q] r] [[7 8] [9]]] (+ x y (count more) (count all) p q (nth r 0))))"),
 (None, "let-keys-literal-admitted", "(defn main [] :i64 (let [{:keys [a] b :b :as m} {:a 1 :b 2}] (+ a b (:a m))))"),
 (None, "let-keys-record-admitted", "(defrecord P [a :i64 b :i64])\n(defn main [] :i64 (let [{:keys [a b]} (->P 1 2)] (+ a (+ b (:a (->P 0 0))) (:b (->P 0 0)))))"),
 (None, "keys-or-typed-map-admitted", "(defn main [] :i64 (let [{:keys [a] :or {a 5}} (typed-map-new [:map :keyword :i64] :b 1)] (+ a (count [:a]))))"),
 (None, "loop-pattern-admitted", "(defn main [] :i64 (loop [[a b] [0 1] n 0] (if (< n 10) (recur [b (+ a b)] (+ n 1)) a)))"),
 (None, "let-vector-short-traps", "(defn main [] :i64 (let [[a b c] [1 2]] a))"),
 # JUN-734 (amu 05668c77, PR #1753): D5 def literals, D6 variadic assoc, D10 typed-map get / (:k m d), bare record param type.
 (None, "def-bool-map-admitted", "(def x {:a true :n 1})\n(defn main [] :i64 (if (:a x) (:n x) 0))"),
 (None, "def-vec-f64-admitted", "(def x [0.5 1.5])\n(defn main [] :i64 (count x))"),
 (None, "def-vec-of-vectors-admitted", "(def x [[1 2] [3 4]])\n(defn main [] :i64 (nth (nth x 1) 0))"),
 (None, "def-map-of-vec-maps-admitted", "(def x {:items [{:a 1} {:a 2}] :n 2})\n(defn main [] :i64 (:a (nth (:items x) 1)))"),
 ("kwmap/def-literal", "def-vec-maps-different-keys", "(def x [{:a 1} {:a 2 :b 3}])\n(defn main [] :i64 (count x))"),
 ("kwmap/def-literal", "def-vec-mixed-kinds", "(def x [1 \"b\"])\n(defn main [] :i64 (count x))"),
 ("kwmap/def-literal", "def-vec-over-64-maps", "(def x [" + " ".join("{:a %d}" % i for i in range(65)) + "])\n(defn main [] :i64 (count x))"),
 (None, "assoc-variadic-typed-map-admitted", "(defn main [] :i64 (get (assoc (typed-map-new [:map :keyword :i64] :a 1) :b 2 :c 3) :c 0))"),
 (None, "assoc-variadic-odd", "(defn main [] :i64 (:b (assoc {:a 1 :b 2} :a 3 :b)))"),
 ("kwmap/option-default", "kw-default-record", "(defn main [] :i64 (:a {:a 1} 0))"),
 ("kwmap/option-default", "or-default", "(defn main [] :i64 (or (:a {:a 1}) 0))"),
 (None, "kw-default-typed-map-admitted", "(defn main [] :i64 (:b (typed-map-new [:map :keyword :i64] :a 1) 9))"),
 (None, "get-typed-map-option-admitted", "(defn main [] :i64 (option-or (get (typed-map-new [:map :keyword :i64] :a 1) :a) 0))"),
 (None, "get-typed-map-default-admitted", "(defn main [] :i64 (get (typed-map-new [:map :keyword :i64] :a 1) :z 4))"),
 ("kwmap/param-row", "record-name-param", "(defrecord P [a :i64])\n(defn f [p P] :i64 (:a p))\n(defn main [] :i64 (f (->P 1)))"),
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
