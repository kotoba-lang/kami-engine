(ns kami-genre-base-systems.platformer.boss-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami-genre-base-systems.platformer.boss :as boss]))

(def phases
  [{:id :phase-1 :hp-at-or-below 1.0 :attacks [:slash :lunge] :attack-interval 1.0}
   {:id :phase-2 :hp-at-or-below 0.5 :attacks [:fireball :slam :fireball] :attack-interval 0.5}
   {:id :phase-3 :hp-at-or-below 0.2 :attacks [:enrage] :attack-interval 0.2}])

(deftest phase-for-hp-picks-highest-threshold-still-covering-hp
  (is (= :phase-1 (:id (boss/phase-for-hp phases 1.0))))
  (is (= :phase-1 (:id (boss/phase-for-hp phases 0.51))))
  (is (= :phase-2 (:id (boss/phase-for-hp phases 0.5))))
  (is (= :phase-2 (:id (boss/phase-for-hp phases 0.21))))
  (is (= :phase-3 (:id (boss/phase-for-hp phases 0.2))))
  (is (= :phase-3 (:id (boss/phase-for-hp phases 0.0)))
      "falls back to the last (final) phase below every threshold"))

(deftest step-boss-phase-first-tick-starts-at-attack-zero
  (let [step (boss/step-boss-phase phases 1.0 nil 0.0)]
    (is (= :phase-1 (:phase step)))
    (is (true? (:phase-changed? step)) "nil current-phase counts as a change")
    (is (= 0 (:attack-index step)))
    (is (= :slash (:next-attack step)))))

(deftest step-boss-phase-cycles-attacks-within-a-phase
  (let [t0 (boss/step-boss-phase phases 1.0 nil 0.0)
        t1 (boss/step-boss-phase phases 1.0 t0 0.6)
        t2 (boss/step-boss-phase phases 1.0 t1 0.6)
        t3 (boss/step-boss-phase phases 1.0 t2 1.2)]
    (is (false? (:phase-changed? t1)))
    (is (= :slash (:next-attack t1)) "0.6s elapsed hasn't yet crossed the 1.0s interval")
    (is (= :lunge (:next-attack t2)) "0.6+0.6=1.2s crosses the 1.0s interval -> advances to index 1")
    (is (= :slash (:next-attack t3)) "advancing past the last attack wraps back around to index 0")))

(deftest step-boss-phase-transitions-on-hp-threshold-crossing
  (let [t0 (boss/step-boss-phase phases 1.0 nil 0.0)
        t1 (boss/step-boss-phase phases 0.6 t0 0.3)
        t2 (boss/step-boss-phase phases 0.4 t1 0.3)]
    (is (false? (:phase-changed? t1)) "0.6 is still within phase-1's threshold (1.0)")
    (is (true? (:phase-changed? t2)) "0.4 crosses into phase-2 (0.5)")
    (is (= :phase-2 (:phase t2)))
    (is (= 0 (:attack-index t2)) "attack table restarts on phase change")
    (is (= :fireball (:next-attack t2)))))
