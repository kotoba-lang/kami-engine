(ns kami.gameplay.attributes-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.attributes :as attr]))

(def blank {:tag "actor" :attrs {}})

(deftest unset-attributes-return-declared-defaults
  (is (= 100.0 (attr/get blank :health))
      "a spawn that forgot to set health must not be born dead")
  (is (= 1.0 (attr/get blank :alive)))
  (is (= -1.0 (attr/get blank :weapon)) "unarmed, not weapon 0")
  (is (= 0.0 (attr/get blank :unregistered-scratch)) "free-form keys default to 0"))

(deftest writes-are-clamped
  (is (= 0.0 (attr/get (attr/set blank :health -50) :health)))
  (is (= 100.0 (attr/get (attr/set blank :health 999) :health)))
  (is (= 0.95 (attr/get (attr/set blank :armor 5.0) :armor))))

(deftest max-may-be-another-attribute
  (let [tank (-> blank (attr/set :health-max 2000) (attr/set :health 1800))]
    (is (= 1800.0 (attr/get tank :health)))
    (is (= 2000.0 (attr/get (attr/set tank :health 5000) :health))
        "the ceiling follows the entity, not the registry constant")))

(deftest yaw-wraps-and-pitch-clamps
  (testing "yaw is periodic"
    (is (< (Math/abs (- (attr/get (attr/set blank :yaw (* 3.0 Math/PI)) :yaw) Math/PI)) 1e-9))
    (is (< (Math/abs (attr/get (attr/set blank :yaw (* 4.0 Math/PI)) :yaw)) 1e-9)))
  (testing "pitch is not — a camera that rolls over the top is a bug"
    (is (= 1.4 (attr/get (attr/set blank :pitch 99.0) :pitch)))
    (is (= -1.4 (attr/get (attr/set blank :pitch -99.0) :pitch)))))

(deftest alive-requires-both-flag-and-health
  (is (attr/alive? blank))
  (is (not (attr/alive? (attr/set blank :health 0))))
  (is (not (attr/alive? (attr/set blank :alive 0))))
  (is (not (attr/alive? nil))))

(deftest effective-health-includes-shield
  (is (= 150.0 (attr/effective-health (attr/set blank :shield 50)))))

(deftest respawn-restores-life-but-keeps-the-record
  (let [dead (-> blank (attr/set :health 0) (attr/set :alive 0)
                 (attr/set :kills 4) (attr/set :damage-dealt 812.5))
        back (attr/respawn dead)]
    (is (attr/alive? back))
    (is (= 100.0 (attr/get back :health)))
    (is (= 4.0 (attr/get back :kills)) "kills survive a life")
    (is (= 812.5 (attr/get back :damage-dealt)))))

(deftest teams
  (let [a (attr/set blank :team 1) b (attr/set blank :team 1) c (attr/set blank :team 2)]
    (is (attr/same-team? a b))
    (is (not (attr/same-team? a c))))
  (testing "team 0 is free-for-all"
    (is (not (attr/same-team? blank blank)))))

(deftest update-attr-goes-through-the-clamp
  (is (= 0.0 (attr/get (attr/update-attr blank :health - 500) :health))
      "a caller subtracting more than the pool has cannot drive health negative"))

(deftest set-many
  (let [e (attr/set-many blank {:health 55 :shield 20 :team 3})]
    (is (= 55.0 (attr/get e :health)))
    (is (= 20.0 (attr/get e :shield)))
    (is (= 3.0 (attr/get e :team)))))

(deftest non-finite-writes-are-refused-on-both-platforms
  ;; ClojureScript would otherwise store NaN silently: every comparison against
  ;; it is false, so the clamp falls through to :else. A NaN deadline never
  ;; expires and NaN health is neither alive nor dead.
  (is (thrown? #?(:clj clojure.lang.ExceptionInfo :cljs js/Error)
               (attr/set blank :health nil)))
  (is (thrown? #?(:clj clojure.lang.ExceptionInfo :cljs js/Error)
               (attr/set blank :health (/ 0.0 0.0))))
  (is (thrown? #?(:clj clojure.lang.ExceptionInfo :cljs js/Error)
               (attr/set blank :yaw (/ 1.0 0.0))))
  (testing "and a nil arriving through update-attr is refused too"
    (is (thrown? #?(:clj clojure.lang.ExceptionInfo :cljs js/Error)
                 (attr/update-attr blank :health (fn [_ _] nil) 1)))))
