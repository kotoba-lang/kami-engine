(ns kami.render.character-preset-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.render.character-preset :as character]
            [kami.render.style :as style]))

(deftest hero-resolves-complete-stylized-role-library
  (let [hero (character/resolve-character {:family :stylized :preset :hero-balanced})]
    (is (= character/contract (:contract hero)))
    (is (= style/contract (get-in hero [:render/style :contract])))
    (is (= :hero (get-in hero [:silhouette :tier])))
    (is (= #{:skin :cloth :metal} (set (keys (:materials hero)))))
    (doseq [[role preset] (:materials hero)]
      (is (= character/material-contract (:contract preset)))
      (is (= :character (:domain preset)))
      (is (= role (:role preset)))
      (is (= :mtoon (get-in preset [:material :model])))
      (is (map? (get-in preset [:material :highlight])))
      (is (true? (get-in preset [:outline-policy :participates?]))))))

(deftest silhouette-tiers-preserve-readability-at-distinct-budgets
  (let [crowd (:silhouette (character/resolve-character {:preset :crowd-efficient}))
        gameplay (:silhouette (character/resolve-character {:preset :combat-readable}))
        hero (:silhouette (character/resolve-character {:preset :hero-balanced}))]
    (is (< (get-in crowd [:mesh-budget :target-triangles])
           (get-in gameplay [:mesh-budget :target-triangles])
           (get-in hero [:mesh-budget :target-triangles])))
    (is (contains? (get-in crowd [:readability :required-landmarks]) :weapon))
    (is (contains? (get-in gameplay [:lod-policy :preserve]) :hands))
    (is (contains? (get-in hero [:lod-policy :preserve]) :face))))

(deftest semantic-role-overrides-do-not-break-the-envelope
  (let [resolved (character/resolve-character
                  {:preset :hero-balanced
                   :palette {:cloth [0.8 0.2 0.1 1.0]}
                   :materials {:metal {:roughness 0.12}}
                   :outline-policy {:skin {:weight 0.5}}})]
    (is (= [0.8 0.2 0.1 1.0]
           (get-in resolved [:materials :cloth :material :base])))
    (is (= 0.12 (get-in resolved [:materials :metal :material :roughness])))
    (is (= 0.5 (get-in resolved [:materials :skin :outline-policy :weight])))
    (is (= :skin (:role (character/material-for resolved :skin))))))

(deftest photoreal-sibling-boundary-is-stable-but-not-faked
  (is (= :pbr (get-in character/family-boundary
                       [:model-by-family :photoreal])))
  (is (= #{:skin :cloth :metal}
         (:stable-roles character/family-boundary)))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (character/resolve-character
                {:family :photoreal :preset :hero-balanced}))))

(deftest invalid-preset-tier-and-role-fail-loudly
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (character/resolve-character {:preset :missing})))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (character/resolve-character
                {:preset :hero-balanced :silhouette-tier :cinematic-ultra})))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (character/material-for
                (character/resolve-character {:preset :hero-balanced}) :glass))))
