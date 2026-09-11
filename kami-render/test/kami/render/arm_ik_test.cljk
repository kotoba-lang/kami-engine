(ns kami.render.arm-ik-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.arm-ik :as ik]
            [kami.render.operator-fit :as fit]))

(defn- distance [a b]
  (#?(:clj Math/sqrt :cljs js/Math.sqrt)
   (reduce + (map (fn [x y] (let [d (- x y)] (* d d))) a b))))

(deftest resolved-fit-has-continuous-two-hand-arm-chains
  (let [resolved (fit/resolve-fit {:tier :gameplay})]
    (doseq [[side chain] (:arm-chains resolved)]
      (is (:valid? chain))
      (is (false? (get-in chain [:metrics :hyperextended?])))
      (is (< (get-in chain [:metrics :target-error]) 0.025))
      (is (<= (get-in chain [:continuity :elbow-gap]) 1.0e-6))
      (is (<= (get-in chain [:continuity :hand-gap]) 1.0e-6))
      (is (= 16 (count (get-in chain [:palette-deltas :upper]))))
      (is (= 16 (count (get-in chain [:palette-deltas :lower]))))
      (is (= 16 (count (get-in chain [:palette-deltas :hand]))))
      (is (= side (:side chain))))))

(deftest solved-segments-preserve-authored-lengths
  (let [resolved (fit/resolve-fit {})]
    (doseq [[_ chain] (:arm-chains resolved)
            :let [{:keys [shoulder elbow hand]} (:centers chain)]]
      (is (< (#?(:clj Math/abs :cljs js/Math.abs)
              (- 0.58 (distance shoulder elbow))) 1.0e-6))
      (is (< (#?(:clj Math/abs :cljs js/Math.abs)
              (- 0.54 (distance elbow hand))) 1.0e-6)))))

(deftest poles-bend-elbows-outward-and-away-from-hyperextension
  (let [resolved (fit/resolve-fit {})
        left-x (get-in resolved [:arm-chains :left :centers :elbow 0])
        right-x (get-in resolved [:arm-chains :right :centers :elbow 0])]
    (is (neg? left-x))
    (is (pos? right-x))
    (is (< (get-in resolved [:arm-chains :left :metrics :elbow-angle])
           (- #?(:clj Math/PI :cljs js/Math.PI) 0.10)))
    (is (< (get-in resolved [:arm-chains :right :metrics :elbow-angle])
           (- #?(:clj Math/PI :cljs js/Math.PI) 0.10)))))

(deftest unreachable-target-is-clamped-with-continuity-preserved
  (let [chain (ik/solve {:side :right :shoulder [0.0 1.0 0.0]
                         :elbow [0.0 0.5 0.0] :hand [0.0 0.0 0.0]
                         :target [0.0 1.0 -10.0] :pole [1.0 0.0 0.0]
                         :upper-length 0.5 :lower-length 0.5})]
    (is (true? (get-in chain [:metrics :clamped?])))
    (is (< (get-in chain [:metrics :solved-reach]) 1.0))
    (is (> (get-in chain [:metrics :target-error]) 9.0))
    (is (:valid? chain))
    (is (zero? (get-in chain [:continuity :elbow-gap])))
    (is (false? (get-in chain [:metrics :hyperextended?])))))

(deftest photoreal-boundary-is-honestly-unsupported
  (is (:same-api? ik/family-boundary))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (ik/solve {:family :photoreal :shoulder [0 0 0] :elbow [0 -1 0]
                          :hand [0 -2 0] :target [0 0 -1] :pole [1 0 0]
                          :upper-length 1 :lower-length 1}))))
