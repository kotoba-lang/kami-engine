(require '[clojure.edn :as edn])

(def registry (edn/read-string (slurp "docs/adapter-registry.edn")))

(defn fail! [message data]
  (binding [*out* *err*]
    (println message (pr-str data)))
  (System/exit 1))

(def contracts (:kami.adapter.registry/contracts registry))

(when-not (= 1 (:kami.adapter.registry/version registry))
  (fail! "adapter registry version must be 1" registry))

(when-not (false? (get-in registry [:kami.adapter.registry/policy :rust-in-default-repo?]))
  (fail! "default repo must not own native Rust implementations" registry))

(when-not (and (vector? contracts) (seq contracts))
  (fail! "adapter registry requires non-empty contracts vector" registry))

(doseq [contract contracts]
  (doseq [k [:id :authority :check :adapters]]
    (when-not (contains? contract k)
      (fail! "adapter contract is missing required key" {:key k :contract contract})))
  (when-not (and (vector? (:adapters contract)) (seq (:adapters contract)))
    (fail! "adapter contract must name at least one adapter" contract)))

(println "ok docs/adapter-registry.edn contracts" (count contracts))
