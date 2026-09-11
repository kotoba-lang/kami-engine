(ns kami.physics.runtime
  "KAMI Engine physics router: one scene contract, explicit fidelity backends."
  (:require [cae.backend :as cae]
            [kotoba.physics.contract :as contract]
            [physics-2d.backend :as rigid-2d]
            [vphysics.backend :as vehicle]))

(def default-backends {rigid-2d/backend-id rigid-2d/backend
                       vehicle/backend-id vehicle/backend
                       cae/backend-id cae/backend})

(defn backend
  ([id] (backend default-backends id))
  ([registry id] (or (get registry id)
                     (throw (ex-info "unknown KAMI physics backend"
                                     {:backend id :available (set (keys registry))})))))

(defn step-scene
  ([backend-id scene dt-s] (step-scene default-backends backend-id scene dt-s))
  ([registry backend-id scene dt-s]
   (let [implementation (backend registry backend-id)]
     (contract/require-backend! implementation :realtime #{:rigid-body-2d})
     (contract/step implementation scene dt-s))))

(defn solve-case
  ([case] (solve-case default-backends case))
  ([registry case]
   (let [implementation (backend registry (:case/backend case))]
     (contract/require-backend! implementation (:case/fidelity case) #{(:case/domain case)})
     (contract/solve implementation case))))

(def make-scene contract/make-scene)
(def make-case contract/make-case)
(def qualified? contract/qualified?)
