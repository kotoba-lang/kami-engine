(ns verify.core
  "Real compile+execute verification for every games/{genre}/logic.clj.
  Iteration-4 lesson (this session): JVM `codegen/compile` succeeding is NOT
  sufficient evidence of correctness -- the i64.const 32-bit truncation bug
  (kotoba-lang/kami-engine#95) only manifested under CLJS/WASM numeric
  semantics, not JVM's. This script compiles through the SAME CLJS path
  production uses, so this class of gap can't silently recur. Run via
  `run-verify.sh` (compiles this, then executes each .wasm through
  kami-script-runtime-rs's `kami-host` for real WASM execution, not just
  `wasm-tools validate` structural checks)."
  (:require [cljs.reader :as edn]
            [kotoba.engine-clj.ast :as kast]
            [kotoba.engine-clj.codegen :as kcodegen]
            [kotoba.engine-clj.wasm-bytes :as kwasm]
            ["fs" :as fs]
            ["path" :as path]))

(defn compile-genre [src]
  (-> (str "[" src "]") edn/read-string kast/parse-program kcodegen/compile kwasm/emit-module-bytes))

(defn -main [& _args]
  (let [games-dir (path/join js/__dirname ".." ".." ".." "games")
        out-dir   js/__dirname
        genres    (-> (.readdirSync fs games-dir) js->clj)
        results
        (mapv (fn [genre]
                (let [src-path (path/join games-dir genre "logic.clj")]
                  (if (.existsSync fs src-path)
                    (try
                      (let [src   (.readFileSync fs src-path "utf8")
                            bytes (compile-genre src)
                            out   (path/join out-dir (str genre ".wasm"))]
                        (.mkdirSync fs out-dir #js {:recursive true})
                        (.writeFileSync fs out (js/Buffer.from bytes))
                        (println (str "OK   " genre " -> " out " (" (.-length bytes) " bytes)"))
                        {:genre genre :ok true})
                      (catch :default e
                        (println (str "FAIL " genre ": " (.-message e)))
                        {:genre genre :ok false}))
                    {:genre genre :ok :no-logic-clj})))
              genres)
        failed (filter #(false? (:ok %)) results)]
    (println (str (count (filter :ok results)) "/" (count results) " compiled"))
    (when (seq failed)
      (println (str "FAILED: " (mapv :genre failed)))
      (js/process.exit 1))))
