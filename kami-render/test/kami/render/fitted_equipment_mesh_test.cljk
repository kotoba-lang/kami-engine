(ns kami.render.fitted-equipment-mesh-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.fitted-equipment-mesh :as fitted]
            [kami.render.operator-fit :as fit]))

(defn- close? [a b] (< (#?(:clj Math/abs :cljs js/Math.abs) (- a b)) 1.0e-6))

(deftest authored-bounds-exactly-match-fit-validation-volumes
  (doseq [tier [:hero :gameplay :crowd]
          :let [resolved (fitted/resolve-meshes {:tier tier})]
          [_ part] (:equipment resolved)]
    (is (every? true? (map close? (get-in part [:bounds :center])
                                  (get-in part [:fit-volume :center]))))
    (is (every? true? (map close? (get-in part [:bounds :half])
                                  (get-in part [:fit-volume :half]))))))

(deftest all-enabled-parts-and-hands-have-real-indexed-meshes
  (let [resolved (fitted/resolve-meshes {:tier :hero})]
    (is (= 7 (count (:equipment resolved))))
    (is (= #{:hand-left :hand-right} (set (keys (:hands resolved)))))
    (doseq [[_ part] (concat (:equipment resolved) (:hands resolved))
            :let [mesh (:mesh part) vertices (quot (count (:positions mesh)) 3)]]
      (is (pos? vertices))
      (is (= (count (:positions mesh)) (count (:normals mesh))))
      (is (= (* 2 vertices) (count (:uvs mesh))))
      (is (every? #(< -1 % vertices) (:indices mesh))))))

(deftest silhouettes-have-curved-normal-evidence-not-box-only-faces
  (let [resolved (fitted/resolve-meshes {:tier :hero})
        ratios (for [[_ part] (concat (:equipment resolved) (:hands resolved))
                     :let [normals (partition 3 (get-in part [:mesh :normals]))
                           curved (count (filter #(>= (count (filter (fn [x] (> (#?(:clj Math/abs :cljs js/Math.abs) x) 0.15)) %)) 2)
                                                 normals))]]
                 (/ curved (double (count normals))))]
    (doseq [[_ part] (concat (:equipment resolved) (:hands resolved))
            :let [normals (partition 3 (get-in part [:mesh :normals]))
                  curved (count (filter #(>= (count (filter (fn [x] (> (#?(:clj Math/abs :cljs js/Math.abs) x) 0.15)) %)) 2)
                                        normals))]]
      (is (seq (:forms part)))
      ;; Closed cylinders include axial cap normals, but still retain radial curved sides.
      (is (> (/ curved (double (count normals))) 0.10)))
    (is (>= (count (filter #(> % 0.50) ratios)) 8))))

(deftest consumer-contract-declares-bind-space-lod-materials-and-grips
  (let [resolved (fitted/resolve-meshes {:tier :gameplay})]
    (is (= :operator-bind-world (:space resolved)))
    (is (= :gameplay (get-in resolved [:lod :tier])))
    (doseq [[_ part] (concat (:equipment resolved) (:hands resolved))]
      (is (= :operator-bind-world (:space part)))
      (is (= :gameplay (:lod part)))
      (is (map? (:material part)))
      (is (= (count (get-in part [:mesh :indices]))
             (get-in part [:material-ranges 0 :index-count]))))
    (is (= :weapon/grip-support (get-in resolved [:hands :hand-left :socket])))
    (is (= :weapon/grip-primary (get-in resolved [:hands :hand-right :socket])))
    (doseq [[_ hand] (:hands resolved)]
      (is (= (:socket hand) (get-in hand [:contact :socket])))
      (is (= (get-in hand [:volume :center]) (get-in hand [:contact :position]))))))

(deftest backpack-and-shoulder-dominance-is-reduced-and-within-occupancy
  (let [resolved-fit (fit/resolve-fit {:tier :hero})]
    (is (< (get-in fit/equipment-layout [:equipment/backpack :silhouette-area]) 0.12))
    (is (< (get-in fit/equipment-layout [:equipment/shoulder-left :silhouette-area]) 0.04))
    (is (< (get-in resolved-fit [:validation :silhouette-occupancy]) 0.45))))

(deftest lod-triangle-budgets-decrease-and-fit-contract-persists
  (let [hero (fitted/resolve-meshes {:tier :hero})
        gameplay (fitted/resolve-meshes {:tier :gameplay})
        crowd (fitted/resolve-meshes {:tier :crowd})]
    (is (> (get-in hero [:budget :triangle-count])
           (get-in gameplay [:budget :triangle-count])
           (get-in crowd [:budget :triangle-count])))
    (doseq [resolved [hero gameplay crowd]]
      (is (<= (get-in resolved [:budget :triangle-count])
              (get-in resolved [:budget :max-triangles])))
      (is (= fit/contract (:fit-contract resolved))))))

(deftest hand-centers-and-arm-sockets-preserve-two-bone-ik
  (let [resolved (fitted/resolve-meshes {})]
    (is (= (get-in resolved [:arm-chains :left :centers :hand])
           (get-in resolved [:hands :hand-left :volume :center])))
    (is (= (get-in resolved [:arm-chains :right :centers :hand])
           (get-in resolved [:hands :hand-right :volume :center])))
    (is (< (get-in resolved [:arm-chains :left :metrics :target-error]) 1.0e-9))
    (is (< (get-in resolved [:arm-chains :right :metrics :target-error]) 1.0e-9))))

(deftest photoreal-boundary-remains-honestly-unsupported
  (is (:same-api? fitted/family-boundary))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (fitted/resolve-meshes {:family :photoreal}))))
