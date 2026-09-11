(ns kami.gameplay.data-test
  "Ties this framework to the EDN tables it claims to run.

  `battle_royale_weapons.edn`, `battle_royale_storm.edn` and
  `battle_royale_consumables.edn` each say in their header that a Rust function
  `stays the builtin fallback AND the parity oracle`. That Rust left the
  repository with the rest of the workspace, so for the last several months the
  tables have had no runtime and no oracle — they describe a game nothing plays.

  These tests are the replacement claim: the tables load, they are internally
  coherent, and the fixtures the rest of this suite tests against are the real
  rows rather than something convenient.

  They read the files rather than embedding them, and they refuse to pass when a
  file cannot be read — an unreadable table must not look the same as a table
  that was checked and found correct."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.edn :as edn]
            [kami.gameplay.weapon :as weapon]
            [kami.gameplay.zone :as zone]
            [kami.gameplay.loot :as loot]
            [kami.gameplay.support :as s]
            #?(:cljs ["fs" :as fs])))

(def data-dirs
  "Where the shared tables might be, depending on where the suite was started.

  `../kami-game-scene/data/` when run from `kami-gameplay/`, and
  `kami-game-scene/data/` when run from the repository root — which is where
  the workspace's `:jvm-test` fleet gate runs everything. A suite that only
  worked from one of the two would pass locally and report an unreadable table
  in CI, and an unreadable table must not look like a table that was checked."
  ["../kami-game-scene/data/" "kami-game-scene/data/"])

(defn- slurp-edn [path]
  (try
    #?(:clj (edn/read-string (slurp path))
       :cljs (edn/read-string (.readFileSync fs path "utf8")))
    (catch #?(:clj Exception :cljs :default) _ nil)))

(defn- read-edn
  "Read a data file from whichever directory has it, or nil. The nil is not
  swallowed — every caller below fails explicitly on it."
  [f]
  (some #(slurp-edn (str % f)) data-dirs))

(def weapons-file (read-edn "battle_royale_weapons.edn"))
(def storm-file (read-edn "battle_royale_storm.edn"))
(def consumables-file (read-edn "battle_royale_consumables.edn"))

(deftest the-tables-are-actually-present
  (testing "an unreadable table must not report the same green as a verified one"
    (is (some? weapons-file) "battle_royale_weapons.edn did not load")
    (is (some? storm-file) "battle_royale_storm.edn did not load")
    (is (some? consumables-file) "battle_royale_consumables.edn did not load")))

(deftest the-weapon-table-loads-and-is-coherent
  (let [t (weapon/load-table weapons-file)]
    (is (= 25 (count t)) "the shipped pool is 25 weapons")
    (is (= (range 25) (map :index t)) "indices are dense and ordered")
    (doseq [w t]
      (is (pos? (:damage w)) (str (:name w) " has no damage"))
      (is (pos? (:fire-rate w)) (str (:name w) " has no fire rate"))
      (is (pos? (:magazine w)) (str (:name w) " has no magazine"))
      (is (>= (:headshot-mult w) 1.0) (str (:name w) " punishes a headshot"))
      (is (< (:damage-falloff w) (:range w))
          (str (:name w) " falls off at or past its own maximum range, so the taper is dead")))
    (testing "every rarity in the table has a drop weight, or its loot never spawns"
      (doseq [r (distinct (map :rarity t))]
        (is (contains? loot/rarity-weight r) (str "no drop weight for rarity " r))))
    (testing "the archetypes are actually distinct"
      (let [by-type (group-by :type t)
            sniper (first (by-type :sniper-rifle))
            shotgun (first (by-type :shotgun))]
        (is (> (:range sniper) (* 5.0 (:range shotgun))))
        (is (> (:fire-rate shotgun) 0.0))
        (is (> (:damage shotgun) (:damage (first (by-type :assault-rifle)))))))))

(deftest the-storm-schedule-loads-and-actually-closes
  (let [ps (zone/load-phases storm-file)]
    (is (= 8 (count ps)))
    (is (= (range 8) (map :phase ps)))
    (testing "radii shrink monotonically to zero — otherwise the match never ends"
      (is (apply > (map :end-radius (butlast ps))))
      (is (zero? (:end-radius (last ps)))))
    (testing "damage escalates, so late phases actually threaten"
      (is (apply < (map :dps ps))))
    (is (every? pos? (map :wait ps)))
    (is (every? pos? (map :shrink ps)))))

(deftest the-storm-fallback-matches-the-file
  (testing "a fallback that quietly differs from its data is worse than none"
    (is (= (map #(select-keys % [:phase :wait :shrink :end-radius :dps])
                zone/default-phases)
           (map #(select-keys % [:phase :wait :shrink :end-radius :dps])
                (zone/load-phases storm-file))))))

(deftest the-consumable-table-loads-and-is-coherent
  (let [cs (loot/load-consumables consumables-file)]
    (is (= 11 (count cs)))
    (doseq [c cs]
      (is (pos? (:use-time c)) (str (:name c) " is instant"))
      (is (pos? (+ (:hp-restore c) (:shield-restore c))) (str (:name c) " does nothing"))
      (is (pos? (:stack c))))
    (testing "caps are what make the table mean something"
      (let [bandage (first (filter #(= :mini-hp (:type %)) cs))]
        (is (= 75.0 (:hp-cap bandage)) "a Bandage stops short of full health")))))

(deftest the-suite-fixtures-are-the-real-rows
  (testing "so the rest of the suite is not testing convenient numbers"
    (let [shipped (weapon/load-table weapons-file)
          find-row (fn [nm] (first (filter #(= nm (:name %)) shipped)))
          compare-keys [:type :damage :headshot-mult :fire-rate :magazine
                        :reload-time :spread :damage-falloff :range :projectile-speed]]
      (doseq [[fixture nm] [[s/ar "Assault Rifle"] [s/shotgun "Pump Shotgun"]
                            [s/sniper "Bolt-Action Sniper"] [s/rocket "Rocket Launcher"]]]
        (is (= (select-keys (find-row nm) compare-keys)
               (select-keys fixture compare-keys))
            (str "fixture for " nm " has drifted from battle_royale_weapons.edn"))))
    (let [shipped (zone/load-phases storm-file)]
      (is (= (take 3 shipped) (zone/load-phases s/storm-raw))
          "storm fixture has drifted from battle_royale_storm.edn"))))
