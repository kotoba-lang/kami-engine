(ns kami.gameplay.rng-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.rng :as rng]))

(defn- draws [seed n bound]
  (loop [s (rng/seed seed) i 0 out []]
    (if (= i n) out
        (let [[v s'] (rng/int s bound)] (recur s' (inc i) (conj out v))))))

(deftest seed-normalisation
  (is (= 1.0 (rng/seed 0)) "a 0 seed folds to 1 rather than looking deterministic by accident")
  (is (= 1.0 (rng/seed nil)))
  (is (pos? (rng/seed -99)) "negative seeds are accepted, not reflected into the state"))

(deftest state-advance-is-exact-across-platforms
  ;; The pinned values below were produced by arbitrary-precision integer
  ;; arithmetic (BigInt), not by this implementation. If a platform's doubles
  ;; ever round differently, these are what catch it.
  (let [s0 (rng/seed 1)
        s1 (rng/next-state s0)
        s2 (rng/next-state s1)
        s3 (rng/next-state s2)]
    (is (= 1103527590.0 s1))
    (is (= 377401575.0 s2))
    (is (= 662824084.0 s3))))

(deftest long-run-stays-in-range
  (loop [s (rng/seed 42) i 0]
    (when (< i 50000)
      (is (and (>= s 0.0) (< s 2147483648.0))
          (str "state left [0, 2^31) at draw " i))
      (recur (rng/next-state s) (inc i))))
  (is true))

(deftest bounded-draws-use-high-bits
  (testing "the shipped host's modulo extraction collapses"
    ;; This is the defect, pinned. (rand-int 4) in the browser today returns a
    ;; constant after the first draw, which is why every royale bot spawns at
    ;; the same corner.
    (let [legacy (loop [s 1.0 i 0 out []]
                   (if (= i 8) out
                       (let [[v s'] (rng/legacy-host-int s 4)]
                         (recur s' (inc i) (conj out v)))))]
      (is (= [2 0 0 0 0 0 0 0] legacy)
          "if this changes, kami.host's generator changed and the fix landed upstream")))
  (testing "high-bit extraction does not"
    (let [xs (draws 1 24 4)]
      (is (= 4 (count (distinct xs))) "all four spawn corners are reachable")
      (is (not= 1 (count (distinct (drop 1 xs)))) "does not collapse to a constant"))))

(deftest bounded-draws-are-not-short-period
  ;; The reason `int` takes the high bits rather than `mod`. An LCG with a
  ;; power-of-two modulus has low bits whose period is only 2^k, so the *exact*
  ;; stream taken modulo 4 is the 4-cycle 2 3 0 1 2 3 0 1 ... forever. Bots
  ;; spawning in a fixed rotation is barely better than all spawning in one
  ;; corner, and neither reads as random.
  ;;
  ;; Distinct-value and uniformity checks both pass on that 4-cycle, so they
  ;; cannot be the test that guards this. Periodicity is.
  (let [xs (draws 1 240 4)]
    (doseq [p (range 1 13)]
      (is (not (every? true? (map = xs (drop p xs))))
          (str "the bounded stream repeats with period " p
               ": " (vec (take 12 xs)))))))

(deftest consecutive-pairs-cover-the-space
  ;; A short-period stream visits only as many ordered pairs as its period. A
  ;; healthy one visits all sixteen.
  (let [pairs (set (partition 2 1 (draws 3 4000 4)))]
    (is (= 16 (count pairs))
        (str "only " (count pairs) " of 16 ordered pairs occur"))))

(deftest bounded-draws-are-uniform
  (let [n 100000
        counts (frequencies (draws 12345 n 4))]
    (is (= 4 (count counts)))
    (doseq [[k c] counts]
      (is (< (Math/abs (- (/ (double c) n) 0.25)) 0.01)
          (str "bucket " k " deviates more than 1 point from uniform: " c "/" n)))))

(deftest degenerate-bounds
  (let [[v _] (rng/int (rng/seed 5) 0)] (is (= 0 v) "bound 0 yields 0, matching the host import"))
  (let [[v _] (rng/int (rng/seed 5) -3)] (is (= 0 v))))

(deftest unit-is-half-open
  (loop [s (rng/seed 7) i 0]
    (when (< i 20000)
      (let [[u s'] (rng/unit s)]
        (is (and (>= u 0.0) (< u 1.0)))
        (recur s' (inc i)))))
  (is true))

(deftest same-seed-same-stream
  (is (= (draws 99 200 1000) (draws 99 200 1000)))
  (is (not= (draws 99 200 1000) (draws 100 200 1000))))

(deftest shuffle-is-a-permutation
  (let [[v _] (rng/shuffle-v (rng/seed 3) (range 50))]
    (is (= (set (range 50)) (set v)))
    (is (= 50 (count v)))
    (is (not= (vec (range 50)) v) "a shuffle that returns the input unchanged is not a shuffle"))
  (let [[a _] (rng/shuffle-v (rng/seed 3) (range 50))
        [b _] (rng/shuffle-v (rng/seed 3) (range 50))]
    (is (= a b) "deterministic for a given seed")))

(deftest pick-handles-empty
  (let [[v _] (rng/pick (rng/seed 1) [])] (is (nil? v)))
  (let [[v _] (rng/pick (rng/seed 1) [:only])] (is (= :only v))))
