(ns kami.gameplay.damage-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.damage :as dmg]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.support :as s]))

(defn- w2 [& {:keys [a b]}]
  (s/world-of (s/actor 1 [0.0 0.0 0.0] (merge {:tag "player"} a))
              (s/actor 2 [0.0 0.0 10.0] (merge {:tag "bot"} b))))

(deftest shield-absorbs-before-health
  (let [world (w2 :b {:shield 50})
        [w' ev] (dmg/apply-damage world 2 {:amount 30 :instigator 1 :source :bullet})]
    (is (= :hit (:kind ev)))
    (is (= 30.0 (:shield-loss ev)))
    (is (= 0.0 (:health-loss ev)))
    (is (= 100.0 (attr/get (get-in w' [:entities 2]) :health)))))

(deftest damage-spills-from-shield-into-health
  (let [[_ ev] (dmg/apply-damage (w2 :b {:shield 20}) 2 {:amount 50 :instigator 1})]
    (is (= 20.0 (:shield-loss ev)))
    (is (= 30.0 (:health-loss ev)))))

(deftest armor-reduces-only-what-reaches-health
  (let [[_ ev] (dmg/apply-damage (w2 :b {:shield 20 :armor 0.5}) 2 {:amount 60 :instigator 1})]
    (is (= 20.0 (:shield-loss ev)) "shield takes its share at face value")
    (is (= 20.0 (:health-loss ev)) "the 40 that got through is halved")))

(deftest a-kill-is-reported-as-a-kill-and-credited
  (let [[w' ev] (dmg/apply-damage (w2 :b {:health 20}) 2 {:amount 50 :instigator 1
                                                          :source :bullet :zone :head})]
    (is (= :kill (:kind ev)))
    (is (= :head (:zone ev)))
    (is (not (attr/alive? (get-in w' [:entities 2]))))
    (is (= 1.0 (attr/get (get-in w' [:entities 1]) :kills)))
    (is (= 20.0 (attr/get (get-in w' [:entities 1]) :damage-dealt))
        "credit is the damage actually absorbed, not the raw number")))

(deftest overkill-is-not-credited
  (let [[w' _] (dmg/apply-damage (w2 :b {:health 5}) 2 {:amount 500 :instigator 1})]
    (is (= 5.0 (attr/get (get-in w' [:entities 1]) :damage-dealt)))))

(deftest refusals-are-reported-not-swallowed
  (testing "a shot that silently does nothing is indistinguishable from a bug"
    (is (= :no-target (:reason (second (dmg/apply-damage (w2) 99 {:amount 10})))))
    (is (= :already-dead
           (:reason (second (dmg/apply-damage (w2 :b {:alive 0}) 2 {:amount 10})))))
    (is (= :zero-damage (:reason (second (dmg/apply-damage (w2) 2 {:amount 0})))))))

(deftest friendly-fire-is-blocked-for-real-squads
  (let [world (w2 :a {:team 3} :b {:team 3})
        [w' ev] (dmg/apply-damage world 2 {:amount 40 :instigator 1})]
    (is (= :friendly-fire (:reason ev)))
    (is (= 100.0 (attr/get (get-in w' [:entities 2]) :health)))))

(deftest team-zero-is-free-for-all
  (let [[_ ev] (dmg/apply-damage (w2) 2 {:amount 40 :instigator 1})]
    (is (= :hit (:kind ev)) "solo battle royale needs no special case")))

(deftest apply-many-preserves-order
  (let [world (w2 :b {:health 60})
        [w' evs] (dmg/apply-many world [{:target 2 :amount 40 :instigator 1}
                                        {:target 2 :amount 40 :instigator 1}])]
    (is (= [:hit :kill] (map :kind evs)) "the second shot gets the kill, not the first")
    (is (not (attr/alive? (get-in w' [:entities 2]))))))

(deftest heal-and-shield-respect-caps-and-refuse-the-dead
  (let [world (w2 :b {:health 40})
        [w' ev] (dmg/heal world 2 1000)]
    (is (= 60.0 (:amount ev)) "gain is what was actually restored")
    (is (= 100.0 (attr/get (get-in w' [:entities 2]) :health))))
  (let [[_ ev] (dmg/add-shield (w2) 2 500)]
    (is (= 100.0 (:amount ev))))
  (let [[_ ev] (dmg/heal (w2 :b {:alive 0}) 2 50)]
    (is (= :blocked (:kind ev)))
    (is (= :not-alive (:reason ev)))))

(deftest a-storm-death-is-a-death-like-any-other
  (let [[w' ev] (dmg/apply-damage (w2 :b {:health 1}) 2 {:amount 5 :source :storm})]
    (is (= :kill (:kind ev)))
    (is (= :storm (:source ev)) "so the kill feed can say how they died")
    (is (nil? (:instigator ev)))
    (is (not (attr/alive? (get-in w' [:entities 2]))))))
