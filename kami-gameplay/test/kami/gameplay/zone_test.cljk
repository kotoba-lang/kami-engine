(ns kami.gameplay.zone-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.zone :as zone]
            [kami.gameplay.vec3 :as v]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.support :as s]))

(def plan (zone/plan {:phases s/storm-raw :start-radius 1000.0 :center [0.0 0.0 0.0] :rng-seed 4}))

(deftest load-phases-falls-back-to-the-shipped-schedule
  (is (= zone/default-phases (zone/load-phases nil)))
  (is (= 8 (count (zone/load-phases []))))
  (is (= 3 (count (zone/load-phases s/storm-raw)))))

(deftest plan-covers-the-whole-schedule
  (is (= (+ 120.0 90.0 90.0 75.0 75.0 60.0) (:total-seconds plan))))

(deftest radius-holds-then-shrinks
  (is (= 1000.0 (:radius (zone/state-at plan 0.0))))
  (is (= 1000.0 (:radius (zone/state-at plan 119.0))) "still holding at the end of the wait")
  (is (not (:shrinking? (zone/state-at plan 119.0))))
  (is (:shrinking? (zone/state-at plan 150.0)))
  (is (< (:radius (zone/state-at plan 150.0)) 1000.0))
  (is (< (Math/abs (- (:radius (zone/state-at plan 209.9)) 700.0)) 1.0)))

(deftest radius-is-monotonically-non-increasing
  (let [rs (map #(:radius (zone/state-at plan %)) (range 0 430 2))]
    (is (every? (fn [[a b]] (>= (+ a 1e-9) b)) (partition 2 1 rs))
        "a storm that grows is not a storm")))

(deftest damage-escalates-with-the-phase
  (is (= 1.0 (:dps (zone/state-at plan 10.0))))
  (is (= 2.0 (:dps (zone/state-at plan 250.0))))
  (is (= 5.0 (:dps (zone/state-at plan 400.0)))))

(deftest after-the-schedule-the-final-circle-is-held
  (let [late (zone/state-at plan 100000.0)]
    (is (= 280.0 (:radius late)))
    (is (pos? (:dps late)) "an outlived match must stay lethal, not become safe")))

(deftest every-circle-stays-inside-its-predecessor
  ;; Swept over many seeds on purpose. Containment is a property of the plan
  ;; generator, not of one lucky draw, and a single-seed check passes even when
  ;; the offset cap is removed entirely — the drift just happens to come out
  ;; small for that seed.
  (testing "a circle that wandered outside would punish players who rotated correctly"
    (doseq [seed (range 1 60)]
      (let [p (zone/plan {:phases (zone/load-phases nil) :start-radius 1000.0
                          :center [0.0 0.0 0.0] :rng-seed seed})]
        (doseq [b (:phases p)]
          (let [drift (v/dist-xz (:from-center b) (:to-center b))
                slack (- (:start-radius b) (:end-radius b))]
            (is (<= drift (+ 1e-6 slack))
                (str "seed " seed " phase " (:phase b) " centre drifted " drift
                     " with only " slack " of slack"))))))))

(deftest the-safe-circle-never-leaves-the-map-it-started-in
  ;; The containment invariant compounds: every circle inside its predecessor
  ;; means every circle inside the first one.
  (doseq [seed (range 1 40)]
    (let [start-r 1000.0
          p (zone/plan {:phases (zone/load-phases nil) :start-radius start-r
                        :center [0.0 0.0 0.0] :rng-seed seed})]
      (doseq [b (:phases p)]
        (is (<= (+ (v/dist-xz (:to-center b) [0.0 0.0 0.0]) (:end-radius b))
                (+ 1e-6 start-r))
            (str "seed " seed " phase " (:phase b) " left the starting circle"))))))

(deftest plan-is-deterministic-for-a-seed
  (is (= (:phases plan) (:phases (zone/plan {:phases s/storm-raw :start-radius 1000.0
                                             :center [0.0 0.0 0.0] :rng-seed 4}))))
  (is (not= (:phases plan) (:phases (zone/plan {:phases s/storm-raw :start-radius 1000.0
                                                :center [0.0 0.0 0.0] :rng-seed 5})))))

(deftest inside-is-ground-plane-only
  (let [z {:center [0.0 0.0 0.0] :radius 100.0 :dps 1.0}]
    (is (zone/inside? z [50.0 0.0 50.0]))
    (is (zone/inside? z [50.0 900.0 50.0]) "jumping does not save you")
    (is (not (zone/inside? z [200.0 0.0 0.0])))
    (is (= 100.0 (zone/distance-outside z [200.0 0.0 0.0])))
    (is (= 0.0 (zone/distance-outside z [10.0 0.0 0.0])))))

(deftest safe-point-leaves-the-safe-alone
  (let [z {:center [0.0 0.0 0.0] :radius 100.0 :dps 1.0}]
    (is (= [10.0 0.0 0.0] (zone/safe-point z [10.0 0.0 0.0] 12.0))
        "a bot that is already safe should be free to fight")
    (let [p (zone/safe-point z [300.0 0.0 0.0] 12.0)]
      (is (zone/inside? z p))
      (is (< (Math/abs (- (v/x p) 88.0)) 1e-6)))))

(deftest tick-damage-only-hits-the-living-outside
  (let [world (s/world-of (s/actor 1 [0.0 0.0 0.0])
                          (s/actor 2 [500.0 0.0 0.0])
                          (s/actor 3 [500.0 0.0 0.0] {:alive 0}))
        z {:center [0.0 0.0 0.0] :radius 100.0 :dps 4.0}
        ds (zone/tick-damage world z 0.5)]
    (is (= 1 (count ds)))
    (is (= 2 (:target (first ds))))
    (is (= 2.0 (:amount (first ds))) "dps x dt")
    (is (= :storm (:source (first ds))))))

(deftest a-zero-dps-phase-produces-nothing
  (is (empty? (zone/tick-damage (s/world-of (s/actor 1 [999.0 0.0 0.0]))
                                {:center [0.0 0.0 0.0] :radius 1.0 :dps 0.0} 1.0))))
