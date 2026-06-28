(ns kami.mangaka.scene-test
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.data.json :as json]
            [kami.mangaka.scene :as s]))

(deftest expression-of-test
  (is (= "Happy" (s/expression-of "joy")))
  (is (= "Happy" (s/expression-of "SMILE")))
  (is (= "Determined" (s/expression-of "resolve")))
  (is (= "Neutral" (s/expression-of "whatever"))))

(deftest transform-defaults
  (is (= {:translation [0.0 0.0 0.0] :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}
         (s/transform)))
  (is (= [1.0 2.0 3.0] (:translation (s/transform [1 2 3])))))

(deftest normalize3-test
  (is (= [0.0 1.0 0.0] (s/normalize3 [0 5 0])))
  (let [[x y z] (s/normalize3 [3 4 0])]
    (is (< (abs (- x 0.6)) 1e-9))
    (is (< (abs (- y 0.8)) 1e-9))
    (is (= 0.0 z))))

(deftest enum-validation
  (is (thrown? Exception (s/camera {:eye [0 0 5] :target [0 0 0] :shot "Bogus"})))
  (is (thrown? Exception (s/light {:role "Nope" :direction [0 -1 0]})))
  (testing "an unknown expression word is lenient → Neutral (mirrors Rust expression_preset)"
    (is (= "Neutral" (-> (s/add-character (s/scene) {:id 1 :rkey "x" :expression "Glee"})
                         :characters first :expression)))))

(deftest three-point-presets
  (is (= 3 (count s/three-point)))
  (is (= "Key" (:role s/three-point-key)))
  (is (= [1.0 0.96 0.92] (:colour s/three-point-key)))
  (testing "direction is normalized"
    (let [[x y z] (:direction s/three-point-key)]
      (is (< (abs (- 1.0 (Math/sqrt (+ (* x x) (* y y) (* z z))))) 1e-9)))))

(deftest scene-build-and-jsonld-roundtrips-shape
  (let [sc (-> (s/scene)
               (s/add-character {:id 0 :rkey "nei" :expression "joy"
                                 :pose-label "action.reach"})
               (s/set-camera (s/camera {:eye [0 1.4 3] :target [0 1.4 0]
                                        :shot "Closeup"
                                        :dof (s/dof 3.0 1.8)}))
               (s/set-lights s/three-point)
               (s/set-environment (s/environment {:biome "water-city" :weather "clear"
                                                  :seed 42 :ground-size-m 80
                                                  :layout-anchors [(s/anchor "bench" (s/transform [2 0 1]))]})))
        jl (s/->jsonld sc)]
    (testing "JSON-LD has the exact keys the Rust from_jsonld reads"
      (is (= s/context (get jl "@context")))
      (is (= 1 (count (get jl "characters"))))
      (let [c (first (get jl "characters"))]
        (is (= 0 (get c "id")))
        (is (= "nei" (get c "rkey")))
        (is (= "Happy" (get c "expression")))          ; normalized from "joy"
        (is (= "action.reach" (get c "pose_label")))
        (is (contains? (get c "root_xform") "translation")))
      (let [cam (get jl "camera")]
        (is (= "Closeup" (get cam "shot")))
        (is (= 35.0 (get cam "fov_deg")))
        (is (= {"focus_distance_m" 3.0 "aperture" 1.8} (get cam "dof"))))
      (is (= 3 (count (get jl "lights"))))
      (is (= "Key" (get-in jl ["lights" 0 "role"])))
      (let [env (get jl "environment")]
        (is (= "water-city" (get env "biome")))
        (is (= 42 (get env "seed")))
        (is (= "bench" (get-in env ["layout_anchors" 0 "name"])))))
    (testing "serializes to JSON without error"
      (is (string? (s/->json sc)))
      (is (= "water-city" (get-in (json/read-str (s/->json sc)) ["environment" "biome"]))))))
