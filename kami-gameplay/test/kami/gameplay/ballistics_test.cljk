(ns kami.gameplay.ballistics-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.ballistics :as ball]
            [kami.gameplay.vec3 :as v]
            [kami.gameplay.rng :as rng]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.support :as s]))

(defn- target-at [pos] (second (s/actor 1 pos {:height 1.9 :radius 0.55})))

(deftest falloff-is-full-then-tapered-then-nothing
  (testing "an AR: full to 50, tapering to 200, dead beyond"
    (is (= 1.0 (ball/falloff-multiplier s/ar 10.0)))
    (is (= 1.0 (ball/falloff-multiplier s/ar 50.0)))
    (is (< (ball/falloff-multiplier s/ar 125.0) 1.0))
    (is (> (ball/falloff-multiplier s/ar 125.0) 0.35))
    (is (= 0.0 (ball/falloff-multiplier s/ar 250.0))
        "a weapon with a stated range should have one"))
  (testing "a shotgun falls off an order of magnitude sooner"
    (is (< (ball/falloff-multiplier s/shotgun 25.0)
           (ball/falloff-multiplier s/ar 25.0)))
    (is (= 0.0 (ball/falloff-multiplier s/shotgun 40.0)))))

(deftest falloff-is-monotonic
  (let [ms (map #(ball/falloff-multiplier s/ar %) (range 0 260 5))]
    (is (every? (fn [[a b]] (>= a b)) (partition 2 1 ms)))))

(deftest spread-responds-to-ads-movement-and-stance
  (let [hip (ball/spread-radians s/ar {})
        ads (ball/spread-radians s/ar {:ads 1.0})
        moving (ball/spread-radians s/ar {:speed 8.0})
        crouched (ball/spread-radians s/ar {:stance 1})]
    (is (< ads hip) "aiming down sight has to buy accuracy, not just zoom")
    (is (> moving hip) "so that stopping to shoot is a decision")
    (is (< crouched hip))
    (is (zero? (ball/spread-radians s/sniper {})) "a 0-spread row means 0")))

(deftest scatter-stays-inside-the-cone
  (let [dir [0.0 0.0 -1.0] half 0.1]
    (loop [rs (rng/seed 1) i 0 worst 0.0]
      (if (= i 2000)
        (do (is (<= worst (+ half 1e-9)) (str "worst deviation " worst " exceeded the cone"))
            (is (> worst (* 0.5 half)) "and the cone is actually used, not hugged at the centre"))
        (let [[d rs'] (ball/scatter rs dir half)]
          (recur rs' (inc i) (max worst (Math/acos (min 1.0 (v/dot d (v/normalize dir)))))))))))

(deftest zero-spread-does-not-scatter
  (let [[d _] (ball/scatter (rng/seed 5) [0.0 0.0 -1.0] 0.0)]
    (is (< (v/dist d [0.0 0.0 -1.0]) 1e-9))))

(deftest capsule-hit-distinguishes-head-from-body
  (let [origin [0.0 1.0 0.0] tgt [0.0 0.0 -10.0]]
    (testing "level shot at chest height is a body hit"
      (is (= :body (:zone (ball/capsule-hit origin [0.0 0.0 -1.0] tgt 0.55 1.9)))))
    (testing "a shot angled up into the top of the capsule is a headshot"
      (let [dir (v/normalize [0.0 0.075 -1.0])]
        (is (= :head (:zone (ball/capsule-hit origin dir tgt 0.55 1.9))))))
    (testing "a shot passing over the head misses entirely"
      (is (nil? (ball/capsule-hit origin (v/normalize [0.0 0.4 -1.0]) tgt 0.55 1.9))))
    (testing "a shot wide of the capsule misses"
      (is (nil? (ball/capsule-hit origin [0.0 0.0 -1.0] [3.0 0.0 -10.0] 0.55 1.9))))
    (testing "a target behind the shooter is never hit"
      (is (nil? (ball/capsule-hit origin [0.0 0.0 1.0] tgt 0.55 1.9))))))

(deftest first-hit-picks-the-nearest-and-skips-the-shooter
  (let [cands [[1 (target-at [0.0 0.0 -30.0])]
               [2 (target-at [0.0 0.0 -10.0])]
               [3 (target-at [0.0 0.0 -5.0])]]]
    (is (= 3 (:id (ball/first-hit [0.0 1.0 0.0] [0.0 0.0 -1.0] cands {}))))
    (is (= 2 (:id (ball/first-hit [0.0 1.0 0.0] [0.0 0.0 -1.0] cands {:exclude 3}))))
    (is (nil? (ball/first-hit [0.0 1.0 0.0] [0.0 0.0 -1.0] cands {:max-distance 2.0})))))

(deftest first-hit-ignores-the-dead
  (let [dead (attr/set (target-at [0.0 0.0 -5.0]) :alive 0)]
    (is (nil? (ball/first-hit [0.0 1.0 0.0] [0.0 0.0 -1.0] [[1 dead]] {})))))

(deftest resolve-shot-reports-hit-miss-and-projectile-distinctly
  (let [cands [[1 (target-at [0.0 0.0 -20.0])]]
        base {:origin [0.0 1.0 0.0] :dir [0.0 0.0 -1.0] :shooter 0 :candidates cands}]
    (testing "a hit carries damage, zone and distance"
      (let [[r _] (ball/resolve-shot (rng/seed 1) (assoc base :weapon s/sniper))]
        (is (= :hit (:kind r)))
        (is (= 1 (:target r)))
        (is (pos? (:damage r)))))
    (testing "a miss is reported, not swallowed as nil"
      (let [[r _] (ball/resolve-shot (rng/seed 1)
                                     (assoc base :weapon s/sniper :dir [1.0 0.0 0.0]))]
        (is (= :miss (:kind r)))))
    (testing "a slow weapon launches a projectile instead of resolving"
      (let [[r _] (ball/resolve-shot (rng/seed 1) (assoc base :weapon s/rocket))]
        (is (= :projectile (:kind r)))
        (is (= 100.0 (:speed r)))))))

(deftest headshots-use-the-multiplier-the-table-has-always-carried
  (let [tgt (target-at [0.0 0.0 -20.0])
        body (ball/resolve-shot (rng/seed 1)
                                {:weapon s/sniper :origin [0.0 1.0 0.0] :dir [0.0 0.0 -1.0]
                                 :shooter 0 :candidates [[1 tgt]]})
        head (ball/resolve-shot (rng/seed 1)
                                {:weapon s/sniper :origin [0.0 1.0 0.0]
                                 :dir (v/normalize [0.0 0.0375 -1.0])
                                 :shooter 0 :candidates [[1 tgt]]})]
    (is (= :body (:zone (first body))))
    (is (= :head (:zone (first head))))
    (is (< (Math/abs (- (/ (:damage (first head)) (:damage (first body))) 2.5)) 1e-6)
        "2.5x is exactly what battle_royale_weapons.edn says, and it now decides something")))

(deftest resolve-shot-is-deterministic-for-a-seed
  (let [args {:weapon s/ar :origin [0.0 1.0 0.0] :dir [0.0 0.0 -1.0] :shooter 0
              :candidates [[1 (target-at [0.0 0.0 -40.0])]]}
        a (ball/resolve-shot (rng/seed 77) args)
        b (ball/resolve-shot (rng/seed 77) args)]
    (is (= (first a) (first b)))
    (is (= (second a) (second b)))))

(deftest projectiles-travel-and-can-be-outrun
  (let [p {:origin [0.0 1.0 0.0] :dir [0.0 0.0 -1.0] :speed 100.0 :shooter 0
           :weapon 3 :max-distance 300.0 :travelled 0.0}
        step1 (ball/step-projectile p 0.1 [])]
    (is (:projectile step1))
    (is (< (Math/abs (- (v/z (:origin (:projectile step1))) -10.0)) 1e-9)
        "100 u/s for 0.1s is 10 units"))
  (testing "it expires at max range instead of flying forever"
    (let [p {:origin [0.0 1.0 0.0] :dir [0.0 0.0 -1.0] :speed 100.0 :shooter 0
             :weapon 3 :max-distance 5.0 :travelled 0.0}]
      (is (:expired (ball/step-projectile p 1.0 []))))))

(deftest fast-projectiles-do-not-tunnel
  (testing "the tick sweeps the segment travelled, not just its endpoint"
    (let [p {:origin [0.0 1.0 0.0] :dir [0.0 0.0 -1.0] :speed 900.0 :shooter 0
             :weapon 3 :max-distance 900.0 :travelled 0.0}
          ;; target sits 20 units away; a 900 u/s projectile crosses it entirely
          ;; within one 1/30s tick
          r (ball/step-projectile p (/ 1.0 30.0) [[1 (target-at [0.0 0.0 -20.0])]])]
      (is (:hit r) "a point test at the new position would have missed this")
      (is (= 1 (:id (:hit r)))))))
