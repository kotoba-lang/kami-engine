(ns vehicle-scene.integration
  "Bridge from the authored Vehicle Scene garage to the shared vehicle document."
  (:require [kotoba.physics.vehicle :as shared]
            [vehicle.backend :as realtime]
            [vehicle-scene.garage :as authored]))

(defn document-for [kind]
  (let [kind-name (name kind)
        plan (authored/build-from-edn kind-name)]
    (when (:error plan) (throw (ex-info "unknown authored vehicle" plan)))
    (shared/document
     {:id kind :name kind-name :preset (keyword kind-name)
      :spec (:ok plan)
      :systems {:powertrain (get-in plan [:ok :engine])
                :gearbox (get-in plan [:ok :gearbox])
                :tire (get-in plan [:ok :tire])}
      :provenance {:authority :kotoba-lang/kami-vehicle-scene
                   :source :shipped-garage-edn}})))

(defn realized-document [kind]
  (let [doc (document-for kind) state (realtime/instantiate-document doc)]
    (realtime/document-with-structure doc state)))
