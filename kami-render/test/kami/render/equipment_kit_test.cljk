(ns kami.render.equipment-kit-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.character-preset :as character]
            [kami.render.equipment-kit :as equipment]))

(deftest gameplay-kit-resolves-all-required-semantic-parts
  (let [kit (equipment/resolve-kit {:tier :gameplay :entity-id :operator/one})]
    (is (= equipment/contract (:contract kit)))
    (is (= character/contract (:character-contract kit)))
    (is (= 8 (count (:parts kit))))
    (is (= 8 (count (:meshes kit))))
    (is (= :humanoid/head
           (get-in (equipment/part kit :equipment/helmet) [:attachment :semantic-id])))
    (is (= :weapon/grip-primary
           (get-in (equipment/part kit :equipment/weapon-primary) [:attachment :socket])))
    (is (= :character/rig
           (get-in (equipment/part kit :equipment/chest-armour) [:mesh :skin])))
    (is (pos? (get-in kit [:budget :headroom-triangles])))))

(deftest every-part-has-portable-material-outline-and-attachment-contract
  (let [kit (equipment/resolve-kit {:tier :hero :entity-id :operator/hero})]
    (doseq [[part-id part] (:parts kit)]
      (is (= part-id (:part/id part)))
      (is (contains? (set (vals equipment/attachment-semantics))
                     (get-in part [:attachment :semantic-id])))
      (is (contains? #{:metal :cloth :visor :accent :weapon} (:material-role part)))
      (is (= :kotoba.render/portable-material-v1 (get-in part [:material :contract])))
      (is (boolean? (get-in part [:outline-policy :participates?])))
      (is (nat-int? (get-in part [:variation :variant-index]))))))

(deftest tier-resolution-is-budgeted-and-keeps-explicit-cull-evidence
  (doseq [tier [:hero :gameplay :crowd]]
    (let [kit (equipment/resolve-kit {:tier tier :entity-id :operator/tier-test})]
      (is (<= (get-in kit [:budget :resolved-triangles])
              (get-in kit [:budget :max-equipment-triangles])))
      (is (= (count (:meshes kit))
             (count (filter (comp :enabled? val) (:parts kit)))))))
  (let [crowd (equipment/resolve-kit {:tier :crowd})]
    (is (false? (:enabled? (equipment/part crowd :equipment/visor))))
    (is (= :subpixel-detail
           (get-in (equipment/part crowd :equipment/visor) [:geometry :reason])))
    (is (false? (:enabled? (equipment/part crowd :equipment/belt))))))

(deftest variation-is-deterministic-and-entity-specific
  (let [a (equipment/resolve-kit {:entity-id :operator/a})
        a2 (equipment/resolve-kit {:entity-id :operator/a})
        b (equipment/resolve-kit {:entity-id :operator/b})]
    (is (= (mapv :variation (vals (:parts a)))
           (mapv :variation (vals (:parts a2)))))
    (is (not= (mapv #(get-in % [:variation :seed]) (vals (:parts a)))
              (mapv #(get-in % [:variation :seed]) (vals (:parts b)))))))

(deftest photoreal-sibling-and-invalid-semantics-fail-honestly
  (is (:same-api? equipment/family-boundary))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (equipment/resolve-kit {:family :photoreal})))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (equipment/resolve-kit {:tier :cinematic-ultra})))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (equipment/part (equipment/resolve-kit {}) :equipment/cape))))
