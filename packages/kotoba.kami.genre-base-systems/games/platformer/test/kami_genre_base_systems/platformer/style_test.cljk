(ns kami-genre-base-systems.platformer.style-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami-genre-base-systems.platformer.style :as style]))

(deftest meter->rank-thresholds
  (is (= :d (style/meter->rank 0.0)))
  (is (= :d (style/meter->rank 19.9)))
  (is (= :c (style/meter->rank 20.0)))
  (is (= :c (style/meter->rank 39.9)))
  (is (= :b (style/meter->rank 40.0)))
  (is (= :b (style/meter->rank 64.9)))
  (is (= :a (style/meter->rank 65.0)))
  (is (= :a (style/meter->rank 89.9)))
  (is (= :s (style/meter->rank 90.0)))
  (is (= :s (style/meter->rank 100.0))))

(deftest register-hit-builds-combo-and-meter
  (let [s (-> (style/style-idle)
              (style/register-hit 10.0 100.0)
              (style/register-hit 10.0 100.0)
              (style/register-hit 10.0 100.0))]
    (is (= 3 (:combo s)))
    (is (= 30.0 (:meter s)))
    (is (= :c (:rank s)))))

(deftest register-hit-clamps-to-meter-max
  (let [s (style/register-hit (assoc (style/style-idle) :meter 95.0) 20.0 100.0)]
    (is (= 100.0 (:meter s)))
    (is (= :s (:rank s)))))

(deftest register-damage-taken-resets-combo-and-penalizes-meter
  (let [built (-> (style/style-idle)
                  (style/register-hit 50.0 100.0)
                  (style/register-hit 50.0 100.0))
        after-damage (style/register-damage-taken built 30.0)]
    (is (= 100.0 (:meter built)) "sanity check on the fixture before damage")
    (is (= 0 (:combo after-damage)))
    (is (= 70.0 (:meter after-damage)))
    (testing "penalty floors at zero, never goes negative"
      (is (= 0.0 (:meter (style/register-damage-taken (assoc built :meter 10.0) 30.0)))))))

(deftest step-style-decays-meter-and-times-out-combo
  (let [hit (style/register-hit (style/style-idle) 50.0 100.0)
        decayed (style/step-style hit 1.0 8.0 1.5)]
    (is (= 42.0 (:meter decayed)))
    (is (= 1 (:combo decayed)) "combo survives -- under the timeout")
    (let [timed-out (style/step-style decayed 1.0 8.0 1.5)]
      (is (= 0 (:combo timed-out)) "combo resets once since-last-hit crosses the timeout"))))
