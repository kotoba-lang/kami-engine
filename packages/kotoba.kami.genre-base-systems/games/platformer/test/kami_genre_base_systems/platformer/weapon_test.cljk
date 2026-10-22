(ns kami-genre-base-systems.platformer.weapon-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami-genre-base-systems.platformer.weapon :as weapon]))

(def entity {:x 0.0 :y 0.0 :facing :right})

(deftest whip-fsm-timing-and-hitbox-only-during-active
  (let [durations {:windup 0.10 :active 0.20 :recovery 0.10}]
    (testing "idle stays idle without input"
      (let [s (weapon/step-whip (weapon/whip-idle) false 0.5 durations)]
        (is (= :idle (:state s)))
        (is (nil? (weapon/whip-hitbox entity s)))))
    (testing "attack-pressed? moves idle -> windup, no hitbox yet"
      (let [s (weapon/step-whip (weapon/whip-idle) true 0.0 durations)]
        (is (= :windup (:state s)))
        (is (nil? (weapon/whip-hitbox entity s)))))
    (testing "windup expires into active, which exposes a hitbox"
      (let [windup (weapon/step-whip (weapon/whip-idle) true 0.0 durations)
            active (weapon/step-whip windup false 0.10 durations)]
        (is (= :active (:state active)))
        (is (some? (weapon/whip-hitbox entity active)))))
    (testing "active expires into recovery, which has no hitbox"
      (let [windup (weapon/step-whip (weapon/whip-idle) true 0.0 durations)
            active (weapon/step-whip windup false 0.10 durations)
            recovery (weapon/step-whip active false 0.20 durations)]
        (is (= :recovery (:state recovery)))
        (is (nil? (weapon/whip-hitbox entity recovery)))))
    (testing "recovery expires back to idle"
      (let [windup (weapon/step-whip (weapon/whip-idle) true 0.0 durations)
            active (weapon/step-whip windup false 0.10 durations)
            recovery (weapon/step-whip active false 0.20 durations)
            idle (weapon/step-whip recovery false 0.10 durations)]
        (is (= :idle (:state idle)))))
    (testing "holding attack through an in-progress swing does not retrigger it"
      (let [windup (weapon/step-whip (weapon/whip-idle) true 0.0 durations)]
        (is (= :windup (:state (weapon/step-whip windup true 0.01 durations))))))))

(deftest whip-hitbox-extends-toward-facing
  (let [active {:state :active :timer 0.0}
        right (weapon/whip-hitbox {:x 100.0 :y 0.0 :facing :right} active)
        left (weapon/whip-hitbox {:x 100.0 :y 0.0 :facing :left} active)]
    (is (> (:x right) 100.0))
    (is (< (:x left) 100.0))))

(deftest sub-weapon-heart-cost-gating
  (testing "fires when hearts >= cost"
    (let [result (weapon/try-fire-sub-weapon (weapon/sub-weapon-idle) 5 3 :dagger)]
      (is (true? (:fired? result)))
      (is (= 2 (:hearts result)))
      (is (weapon/sub-weapon-active? (:sub-state result)))))
  (testing "refuses when hearts < cost"
    (let [result (weapon/try-fire-sub-weapon (weapon/sub-weapon-idle) 2 3 :dagger)]
      (is (false? (:fired? result)))
      (is (= 2 (:hearts result)))
      (is (= (weapon/sub-weapon-idle) (:sub-state result)))))
  (testing "refuses while already :active (no re-fire mid-lockout)"
    (let [active {:state :active :timer 0.0 :kind :dagger}
          result (weapon/try-fire-sub-weapon active 10 3 :dagger)]
      (is (false? (:fired? result)))
      (is (= 10 (:hearts result))))))

(deftest sub-weapon-returns-to-idle-after-duration
  (let [fired (:sub-state (weapon/try-fire-sub-weapon (weapon/sub-weapon-idle) 10 3 :axe))
        mid (weapon/step-sub-weapon fired 0.1 0.5)
        done (weapon/step-sub-weapon mid 0.5 0.5)]
    (is (weapon/sub-weapon-active? mid))
    (is (= :idle (:state done)))))

(deftest item-crash-requires-both-active-and-enough-hearts
  (let [whip-active {:state :active :timer 0.0}
        whip-idle (weapon/whip-idle)
        sub-active {:state :active :timer 0.0 :kind :cross}
        sub-idle (weapon/sub-weapon-idle)]
    (testing "not reachable unless both are active"
      (is (false? (weapon/item-crash? whip-idle sub-active)))
      (is (false? (weapon/item-crash? whip-active sub-idle)))
      (is (true? (weapon/item-crash? whip-active sub-active))))
    (testing "triggers only with enough hearts"
      (is (= {:triggered? true :hearts 7}
             (weapon/try-item-crash whip-active sub-active 10 3)))
      (is (= {:triggered? false :hearts 2}
             (weapon/try-item-crash whip-active sub-active 2 3))))
    (testing "crash hitbox is centered on the entity and larger than the whip's"
      (let [hb (weapon/item-crash-hitbox {:x 50.0 :y 50.0})]
        (is (< (:x hb) 50.0))
        (is (> (:width hb) (:length weapon/default-whip-reach)))))))
