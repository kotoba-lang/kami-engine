(ns kami.physics.runtime-test
  (:require [clojure.test :refer [deftest is]]
            [kami.physics.runtime :as runtime]
            [physics_2d :as p2]))

(deftest realtime-world-is-routed-through-shared-scene
  (let [body (fn [id x mass] {:entity/id id :transform/position [x 0.0] :physics/velocity [0.0 0.0]
                               :physics/body {:mass mass :collider (p2/make-circle-collider 1.0)}})
        scene (assoc (runtime/make-scene {:id :space :dimensions 2
                                          :entities [(body :player 0.0 1.0) (body :wall 1.5 0.0)]})
                     :scene/forces {:gravity [0.0 0.0]})
        next-scene (runtime/step-scene :kotoba/rigid-body-2d scene (/ 1.0 60.0))]
    (is (= 1 (count (:physics/contacts next-scene))))
    (is (= [:player :wall] (mapv :entity/id (:scene/entities next-scene))))))

(deftest fidelity-cannot-be-silently-downgraded
  (let [scene (runtime/make-scene {:id :space :dimensions 2 :entities []})
        case (runtime/make-case {:id :bad :scene scene :domain :rigid-body-2d
                                 :backend-kind :kotoba/rigid-body-2d :fidelity :high-fidelity :controls {}})]
    (is (thrown? #?(:clj Exception :cljs js/Error) (runtime/solve-case case)))))
