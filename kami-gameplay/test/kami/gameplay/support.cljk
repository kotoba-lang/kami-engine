(ns kami.gameplay.support
  "Fixtures shared by the suite.

  The weapon and storm rows here are copied verbatim from
  `kami-game-scene/data/battle_royale_*.edn`. `kami.gameplay.data-test` asserts
  that they still match the shipped files, so a fixture that drifts from the
  data it stands for fails loudly instead of quietly testing something else."
  (:require [kami.gameplay.attributes :as attr]
            [kami.gameplay.weapon :as weapon]))

(def weapons-raw
  [{:type :assault-rifle :name "Assault Rifle" :rarity :common :damage 30 :headshot-mult 1.5
    :fire-rate 5.5 :magazine 30 :reload-time 2.3 :spread 2.5 :damage-falloff 50 :range 200
    :projectile-speed 500}
   {:type :shotgun :name "Pump Shotgun" :rarity :common :damage 80 :headshot-mult 2
    :fire-rate 0.7 :magazine 5 :reload-time 4.5 :spread 6 :damage-falloff 10 :range 30
    :projectile-speed 400}
   {:type :sniper-rifle :name "Bolt-Action Sniper" :rarity :rare :damage 105 :headshot-mult 2.5
    :fire-rate 0.33 :magazine 1 :reload-time 3 :spread 0 :damage-falloff 200 :range 500
    :projectile-speed 800}
   {:type :rocket-launcher :name "Rocket Launcher" :rarity :epic :damage 110 :headshot-mult 1
    :fire-rate 0.75 :magazine 1 :reload-time 3 :spread 0 :damage-falloff 0 :range 300
    :projectile-speed 100}])

(def table (weapon/load-table weapons-raw))

(def ar (weapon/by-index table 0))
(def shotgun (weapon/by-index table 1))
(def sniper (weapon/by-index table 2))
(def rocket (weapon/by-index table 3))

(def storm-raw
  [{:phase 0 :wait 120.0 :shrink 90.0 :end-radius 700.0 :dps 1.0}
   {:phase 1 :wait 90.0 :shrink 75.0 :end-radius 450.0 :dps 2.0}
   {:phase 2 :wait 75.0 :shrink 60.0 :end-radius 280.0 :dps 5.0}])

(defn actor
  "A test entity at `pos` with `attrs` applied through the clamp."
  ([id pos] (actor id pos {}))
  ([id pos attrs]
   [id (-> {:tag (or (:tag attrs) "bot") :pos (vec (map double pos))
            :vel [0.0 0.0 0.0] :attrs {} :backpack {}}
           (attr/set-many (dissoc attrs :tag)))]))

(defn world-of
  "A minimal world value: just an entity table."
  [& pairs]
  {:entities (into {} pairs)})
