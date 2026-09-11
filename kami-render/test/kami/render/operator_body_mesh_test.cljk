(ns kami.render.operator-body-mesh-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.equipment-kit :as equipment]
            [kami.render.operator-body-mesh :as operator]
            [kotoba.render.character :as render-character]))

(deftest every-tier-produces-valid-indexed-skinned-geometry
  (doseq [tier [:hero :gameplay :crowd]
          :let [body (operator/resolve-operator {:tier tier})
                mesh (:mesh body)
                vertex-count (quot (count (:positions mesh)) 3)]]
    (is (pos? vertex-count))
    (is (= (count (:positions mesh)) (count (:normals mesh))))
    (is (= (* vertex-count 2) (count (:uvs mesh))))
    (is (zero? (mod (count (:indices mesh)) 3)))
    (is (every? (fn [[x y z]]
                  (< (#?(:clj Math/abs :cljs js/Math.abs)
                      (- 1.0 (#?(:clj Math/sqrt :cljs js/Math.sqrt)
                                (+ (* x x) (* y y) (* z z)))))
                     1.0e-5))
                (partition 3 (:normals mesh))))
    (is (= vertex-count (count (:joints mesh)) (count (:weights mesh))))
    (is (every? #(= 4 (count %)) (:joints mesh)))
    (is (every? #(= 4 (count %)) (:weights mesh)))
    (is (every? #(< -1 % vertex-count) (:indices mesh)))
    (is (every? #(< -1 % (count render-character/joint-order))
                (mapcat identity (:joints mesh))))
    (is (<= (get-in body [:budget :triangle-count])
            (get-in body [:budget :max-triangles])))))

(deftest body-has-complete-semantic-anatomy-and-non-cuboid-forms
  (let [body (operator/resolve-operator {:tier :hero})
        components (:components body)]
    (is (= 16 (count components)))
    (doseq [id [:head :neck :torso :pelvis
                :upper-arm-left :lower-arm-left :hand-left
                :upper-arm-right :lower-arm-right :hand-right
                :upper-leg-left :lower-leg-left :boot-left
                :upper-leg-right :lower-leg-right :boot-right]]
      (is (contains? components id)))
    (is (= #{:sphere :cylinder :capsule :beveled-form}
           (set (map :shape (vals components)))))))

(deftest skinning-contains-rigid-and-two-bone-components
  (let [body (operator/resolve-operator {})
        components (:components body)]
    (is (= :rigid (get-in components [:head :skinning])))
    (is (= :rigid (get-in components [:hand-right :skinning])))
    (is (= :two-bone-blend (get-in components [:torso :skinning])))
    (is (= :two-bone-blend (get-in components [:lower-leg-left :skinning])))
    (doseq [weights (get-in body [:mesh :weights])]
      (is (< (#?(:clj Math/abs :cljs js/Math.abs) (- 1.0 (reduce + weights))) 1.0e-6)))))

(deftest materials-ranges-and-outline-are-executor-ready
  (let [body (operator/resolve-operator {:team-palette {:cloth [0.12 0.52 0.8 1.0]
                                                        :accent [0.9 0.3 0.08 1.0]}})]
    (is (= #{:skin :cloth :metal :accent} (set (keys (:materials body)))))
    (is (= (count (get-in body [:mesh :indices]))
           (reduce + (map :index-count (:material-ranges body)))))
    (doseq [[role material] (:materials body)]
      (is (= role (:semantic-role material)))
      (is (= :mtoon (get-in material [:executor :shader]))))
    (is (= :screen-space (get-in body [:outline-policy :mode])))))

(deftest equipment-attachment-semantics-are-compatible
  (let [body (operator/resolve-operator {})
        body-semantics (set (keys (:attachments body)))
        equipment-semantics (set (vals equipment/attachment-semantics))]
    (is (every? body-semantics equipment-semantics))
    (is (= :hand-right (get-in body [:attachments :humanoid/right-hand :joint])))
    (is (:equipment-compatible? body))))

(deftest lods-decrease-actual-triangles-and-remain-full-bodies
  (let [hero (operator/resolve-operator {:tier :hero})
        gameplay (operator/resolve-operator {:tier :gameplay})
        crowd (operator/resolve-operator {:tier :crowd})]
    (is (> (get-in hero [:budget :triangle-count])
           (get-in gameplay [:budget :triangle-count])
           (get-in crowd [:budget :triangle-count])))
    (is (= 16 (count (:material-ranges crowd))))
    (is (= (set (keys (:components hero))) (set (keys (:components crowd)))))))

(deftest deterministic-variants-and-honest-photoreal-boundary
  (let [a (operator/resolve-operator {:entity-id :operator/a})
        a2 (operator/resolve-operator {:entity-id :operator/a})
        b (operator/resolve-operator {:entity-id :operator/b})]
    (is (= (:variation a) (:variation a2)))
    (is (= (:mesh a) (:mesh a2)))
    (is (not= (:mesh a) (:mesh b))))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (operator/resolve-operator {:family :photoreal}))))
