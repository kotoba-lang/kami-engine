(ns kami.render.character-material-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.character-material :as material]
            [kami.render.character-preset :as character]
            [kami.render.equipment-kit :as equipment]))

(def hero (character/resolve-character {:preset :hero-balanced}))

(deftest every-semantic-role-lowers-to-an-executor-ready-material
  (let [registry (material/lower-library hero)]
    (is (= #{:skin :cloth :metal :visor :emissive :accent :weapon}
           (set (keys registry))))
    (doseq [[role m] registry]
      (is (= material/contract (:contract m)))
      (is (= role (:semantic-role m)))
      (is (= :mtoon (get-in m [:executor :shader])))
      (is (= 4 (count (get-in m [:executor :uniforms :albedo]))))
      (is (= 4 (count (get-in m [:executor :uniforms :subsurface-color]))))
      (is (= 4 (count (get-in m [:executor :uniforms :hair-scatter]))))
      (is (number? (get-in m [:executor :uniforms :emission-r])))
      (is (map? (:highlight m)))
      (is (map? (:outline m))))))

(deftest roles-differ-by-full-shader-values-not-only-color
  (let [registry (material/lower-library hero)
        fingerprint (fn [m] [(:metallic m) (:roughness m) (:shade m)
                             (:rim m) (:highlight m) (:emissive m)])
        fingerprints (map (comp fingerprint val) registry)]
    (is (= (count registry) (count (set fingerprints))))))

(deftest team-palette-is-limited-to-cloth-and-accent
  (let [colors {:cloth [0.1 0.7 0.2 1.0] :accent [0.95 0.8 0.1 1.0]}
        registry (material/lower-library hero {:team-palette colors})]
    (is (= (:cloth colors) (get-in registry [:cloth :base-color])))
    (is (= (:accent colors) (get-in registry [:accent :base-color])))
    (is (= (get-in (material/lower-library hero) [:skin :base-color])
           (get-in registry [:skin :base-color])))
    (is (= (get-in (material/lower-library hero) [:weapon :base-color])
           (get-in registry [:weapon :base-color]))))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (material/lower-library hero {:team-palette {:skin [1 0 0 1]}})))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (material/lower-library hero {:team-palette {:weapon [1 0 0 1]}}))))

(deftest equipment-enabled-parts-reference-lowered-materials-at-every-tier
  (doseq [tier [:hero :gameplay :crowd]
          :let [kit (equipment/resolve-kit {:tier tier})]]
    (doseq [[_ part] (:parts kit) :when (:enabled? part)]
      (is (= (get-in kit [:material-registry (:material-role part) :id])
             (get-in part [:material :id])))
      (is (= (:material-role part) (get-in part [:mesh :material])))))
  (let [crowd (equipment/resolve-kit {:tier :crowd})]
    (is (not (contains? (:material-registry crowd) :visor)))
    (is (contains? (:material-registry crowd) :weapon))))

(deftest equipment-kit-forwards-valid-team-palette
  (let [cloth [0.15 0.62 0.22 1.0]
        accent [0.94 0.72 0.08 1.0]
        kit (equipment/resolve-kit {:tier :gameplay
                                    :team-palette {:cloth cloth :accent accent}})]
    (is (= cloth (get-in kit [:material-registry :cloth :base-color])))
    (is (= accent (get-in kit [:material-registry :accent :base-color])))))

(deftest photoreal-and-unknown-roles-fail-honestly
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (material/lower-material (assoc hero :family :photoreal) :skin)))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (material/lower-material hero :glass))))
