(ns kami.gameplay.weapon-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.weapon :as weapon]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.support :as s]))

(def blank {:tag "p" :attrs {} :backpack {}})

(deftest load-table-normalises
  (is (= 4 (count s/table)))
  (is (= 0 (:index s/ar)))
  (testing "int stats become doubles — an integer fire rate is how a division truncates"
    (is (double? (:damage s/ar)))
    (is (double? (:spread s/shotgun)))))

(deftest load-table-accepts-either-shape
  (is (= (weapon/load-table s/weapons-raw)
         (weapon/load-table {:battle-royale/weapons s/weapons-raw}))))

(deftest by-index-bounds
  (is (nil? (weapon/by-index s/table -1)) "-1 is unarmed, a legitimate state")
  (is (nil? (weapon/by-index s/table 99)))
  (is (some? (weapon/by-index s/table 0))))

(deftest hitscan-classification
  (testing "bullets resolve instantly"
    (is (weapon/hitscan? s/sniper) "an 800 u/s round crosses its range inside a frame")
    (is (weapon/hitscan? s/ar))
    (is (weapon/hitscan? s/shotgun)))
  (testing "launchers travel"
    (is (not (weapon/hitscan? s/rocket)) "a 100 u/s rocket must be dodgeable"))
  (testing "the threshold sits inside the order-of-magnitude gap in the table"
    (is (< 125.0 weapon/hitscan-speed-threshold 400.0))))

(deftest equip-fills-the-magazine
  (let [e (weapon/equip s/table blank 0)]
    (is (= 0.0 (attr/get e :weapon)))
    (is (= 30.0 (attr/get e :ammo)))))

(deftest equip-cancels-a-reload-in-flight
  (let [e (-> (weapon/equip s/table blank 0)
              (attr/set :ammo 0)
              (attr/set :reserve-ammo 90))
        reloading (weapon/begin-reload s/table e 1000)
        swapped (weapon/equip s/table reloading 1)]
    (is (weapon/reloading? reloading 1000))
    (is (not (weapon/reloading? swapped 1000))
        "otherwise the old reload finishing refills the new gun")
    (is (= 5.0 (attr/get swapped :ammo)))))

(deftest fire-rate-is-enforced
  (let [e (weapon/equip s/table blank 0)]
    (is (weapon/can-fire? s/table e 0))
    (let [after (weapon/consume-shot s/table e 0)]
      (is (not (weapon/can-fire? s/table after 100))
          "5.5 rounds/sec means 181ms between shots")
      (is (weapon/can-fire? s/table after 200))
      (is (= 29.0 (attr/get after :ammo))))))

(deftest fire-interval-guards-a-malformed-row
  (is (= 1000.0 (weapon/fire-interval-ms {:fire-rate 0.0}))
      "a bad row gives a slow weapon, not an un-representable next-fire time"))

(deftest empty-magazine-blocks-firing-and-auto-reloads
  (let [e (-> (weapon/equip s/table blank 2) (attr/set :reserve-ammo 10))
        fired (weapon/consume-shot s/table e 0)]
    (is (= 0.0 (attr/get fired :ammo)))
    (is (weapon/reloading? fired 0) "running dry starts the reload for you")
    (is (not (weapon/can-fire? s/table fired 10000))
        "and no shot escapes while it is running")))

(deftest reload-moves-rounds-from-the-reserve
  (let [e (-> (weapon/equip s/table blank 0) (attr/set :ammo 0) (attr/set :reserve-ammo 90))
        r (weapon/begin-reload s/table e 0)
        mid (weapon/step s/table r 1000)
        done (weapon/step s/table r 2300)]
    (is (= 0.0 (attr/get mid :ammo)) "still reloading at 1.0s of a 2.3s reload")
    (is (= 30.0 (attr/get done :ammo)))
    (is (= 60.0 (attr/get done :reserve-ammo)))))

(deftest partial-reload-is-honoured
  (let [e (-> (weapon/equip s/table blank 0) (attr/set :ammo 0) (attr/set :reserve-ammo 7))
        done (weapon/step s/table (weapon/begin-reload s/table e 0) 2300)]
    (is (= 7.0 (attr/get done :ammo)) "7 rounds is 7 rounds, not 30 and not none")
    (is (= 0.0 (attr/get done :reserve-ammo)))))

(deftest reload-spam-does-not-restart-the-timer
  (let [e (-> (weapon/equip s/table blank 0) (attr/set :ammo 0) (attr/set :reserve-ammo 90))
        r1 (weapon/begin-reload s/table e 0)
        r2 (weapon/begin-reload s/table r1 500)]
    (is (= (attr/get r1 :reload-until) (attr/get r2 :reload-until)))))

(deftest reload-refused-without-reserve
  (let [e (-> (weapon/equip s/table blank 0) (attr/set :ammo 0) (attr/set :reserve-ammo 0))]
    (is (= e (weapon/begin-reload s/table e 0)))))

(deftest reload-refused-when-already-full
  (let [e (-> (weapon/equip s/table blank 0) (attr/set :reserve-ammo 90))]
    (is (= e (weapon/begin-reload s/table e 0)))))

(deftest the-dead-and-the-unarmed-cannot-fire
  (is (not (weapon/can-fire? s/table (attr/set (weapon/equip s/table blank 0) :alive 0) 0)))
  (is (not (weapon/can-fire? s/table (attr/set (weapon/equip s/table blank 0) :downed 1) 0)))
  (is (not (weapon/can-fire? s/table blank 0)) "weapon -1 is unarmed"))
