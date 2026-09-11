(ns kami.render.weapon-mesh-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.character-material :as material]
            [kami.render.equipment-kit :as equipment]
            [kami.render.weapon-mesh :as weapon]))

(deftest every-tier-produces-valid-actual-indexed-geometry
  (doseq [tier [:hero :gameplay :crowd]
          :let [resolved (weapon/resolve-weapon {:tier tier :entity-id :operator/alpha})
                mesh (:mesh resolved)
                vertices (quot (count (:positions mesh)) 3)]]
    (is (pos? vertices))
    (is (= (count (:positions mesh)) (count (:normals mesh))))
    (is (= (* vertices 2) (count (:uvs mesh))))
    (is (zero? (mod (count (:indices mesh)) 3)))
    (is (every? #(< -1 % vertices) (:indices mesh)))
    (is (= (count (:indices mesh))
           (reduce + (map :index-count (:material-ranges resolved)))))
    (is (<= (get-in resolved [:budget :triangle-count])
            (get-in resolved [:budget :max-triangles])))))

(deftest hero-rifle-has-all-semantic-components-and-material-ranges
  (let [rifle (weapon/resolve-weapon {:tier :hero})
        ids (set (map :component (:material-ranges rifle)))]
    (is (= #{:receiver :barrel :stock :grip :magazine :optic :muzzle :handguard} ids))
    (is (= #{:weapon :accent :emissive} (set (keys (:materials rifle)))))
    (doseq [[role m] (:materials rifle)]
      (is (= role (:semantic-role m)))
      (is (= material/contract (:contract m))))))

(deftest crowd-is-a-real-reduced-rifle-not-a-stick
  (let [rifle (weapon/resolve-weapon {:tier :crowd})
        enabled (set (for [[id c] (:components rifle) :when (:enabled? c)] id))]
    (is (= #{:receiver :barrel :stock :magazine} enabled))
    (is (> (count (get-in rifle [:mesh :positions])) 24))
    (is (< (get-in rifle [:budget :triangle-count])
           (get-in (weapon/resolve-weapon {:tier :gameplay}) [:budget :triangle-count])))
    (is (= :lod-merged-or-culled (get-in rifle [:components :optic :reason])))))

(deftest primary-and-support-grips-use-portable-semantics
  (let [rifle (weapon/resolve-weapon {})]
    (is (= :humanoid/right-hand (get-in rifle [:attachment :semantic-id])))
    (is (= :weapon/grip-primary (get-in rifle [:sockets :primary-grip :semantic-id])))
    (is (= :weapon/grip-support (get-in rifle [:sockets :support-grip :semantic-id])))
    (is (not= (get-in rifle [:sockets :primary-grip :position])
              (get-in rifle [:sockets :support-grip :position])))))

(deftest authored-rifle-has-curved-silhouette-surface-sockets-and-fit-bounds
  (let [rifle (weapon/resolve-weapon {:tier :hero})
        normals (partition 3 (get-in rifle [:mesh :normals]))
        curved (filter #(>= (count (filter (fn [v] (> (Math/abs (double v)) 0.15)) %)) 2)
                       normals)
        primary (get-in rifle [:sockets :primary-grip])
        support (get-in rifle [:sockets :support-grip])]
    (is (> (/ (count curved) (double (count normals))) 0.45))
    (is (= :grip (get-in primary [:contact-surface :component])))
    (is (= :handguard (get-in support [:contact-surface :component])))
    ;; Primary is authored on the ellipsoid grip; support is on handguard underside.
    (let [[x y z] (:position primary)]
      (is (< (Math/abs (- 1.0 (+ (Math/pow (/ (- x 0.045) 0.055) 2.0)
                                  (Math/pow (/ (+ y 0.23) 0.16) 2.0)
                                  (Math/pow (/ (+ z 0.01) 0.08) 2.0)))) 0.01)))
    (is (= -0.10 (second (:position support))))
    (is (= :capsule (get-in rifle [:fit-volume :kind])))
    (is (= #{:weapon :accent :emissive} (set (map :material-role (:material-ranges rifle)))))
    (doseq [r (:material-ranges rifle)]
      (is (every? pos? (get-in r [:bounds :half]))))))

(deftest equipment-primary-weapon-points-to-the-actual-mesh-contract
  (let [part (equipment/part (equipment/resolve-kit {}) :equipment/weapon-primary)]
    (is (= :character.weapon/stylized-rifle (:mesh-semantic part)))
    (is (= weapon/contract (:mesh-contract part)))
    (is (= weapon/contract (get-in part [:mesh :mesh-contract])))))

(deftest variants-are-deterministic-and-change-silhouette-parameters
  (let [a (weapon/resolve-weapon {:entity-id :weapon/a})
        a2 (weapon/resolve-weapon {:entity-id :weapon/a})
        b (weapon/resolve-weapon {:entity-id :weapon/b})]
    (is (= (:variation a) (:variation a2)))
    (is (= (:mesh a) (:mesh a2)))
    (is (not= (:variation a) (:variation b)))
    (is (not= (:mesh a) (:mesh b)))))

(deftest team-palette-reaches-accent-only-and-photoreal-rejects
  (let [accent [0.9 0.15 0.08 1.0]
        rifle (weapon/resolve-weapon {:team-palette {:accent accent}})]
    (is (= accent (get-in rifle [:materials :accent :base-color])))
    (is (not= accent (get-in rifle [:materials :weapon :base-color]))))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (weapon/resolve-weapon {:family :photoreal}))))
