(ns kami.gameplay.perception-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.perception :as perc]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.support :as s]))

(def senses (perc/senses {}))

(defn- observer-facing [yaw] (second (s/actor 1 [0.0 0.0 0.0] {:yaw yaw})))

(deftest sight-is-a-cone-not-a-sphere
  (testing "the shipped bot knows where you are from anywhere; this one does not"
    (let [o (observer-facing 0.0)                      ;; looking down -Z
          front (second (s/actor 2 [0.0 0.0 -20.0]))
          behind (second (s/actor 3 [0.0 0.0 20.0]))]
      (is (perc/can-see? o front senses))
      (is (not (perc/can-see? o behind senses))
          "which is what lets a player flank"))))

(deftest sight-has-a-range
  (let [o (observer-facing 0.0)]
    (is (perc/can-see? o (second (s/actor 2 [0.0 0.0 -100.0])) senses))
    (is (not (perc/can-see? o (second (s/actor 2 [0.0 0.0 -5000.0])) senses))
        "the shipped hunt-range is 3000 units, i.e. the whole map")))

(deftest occlusion-is-injected-not-assumed
  (let [o (observer-facing 0.0)
        t (second (s/actor 2 [0.0 0.0 -20.0]))]
    (is (perc/can-see? o t senses (constantly false)))
    (is (not (perc/can-see? o t senses (constantly true)))
        "a wall between them blocks the contact")))

(deftest the-dead-are-not-perceived
  (is (not (perc/can-see? (observer-facing 0.0)
                          (attr/set (second (s/actor 2 [0.0 0.0 -10.0])) :alive 0)
                          senses))))

(deftest hearing-ignores-facing-but-not-distance
  (let [o (observer-facing 0.0)
        behind (second (s/actor 2 [0.0 0.0 20.0]))
        far (second (s/actor 3 [0.0 0.0 200.0]))]
    (is (perc/can-hear? o behind senses false))
    (is (not (perc/can-hear? o far senses false)))
    (is (perc/can-hear? o far senses true) "a gunshot carries much further")))

(deftest sense-targets-ranks-sight-over-hearing-and-nearest-first
  (let [world (s/world-of (s/actor 1 [0.0 0.0 0.0] {:yaw 0.0 :tag "bot"})
                          (s/actor 2 [0.0 0.0 -50.0] {:tag "player"})
                          (s/actor 3 [0.0 0.0 -20.0] {:tag "player"})
                          (s/actor 4 [0.0 0.0 30.0] {:tag "player"}))
        seen (perc/sense-targets world 1 senses {})]
    (is (= [3 2 4] (map :id seen)) "nearest first")
    (is (= :sight (:how (first seen))))
    (is (= :hearing (:how (last seen))) "the one behind is heard, not seen")))

(deftest teammates-are-not-targets
  (let [world (s/world-of (s/actor 1 [0.0 0.0 0.0] {:yaw 0.0 :team 2})
                          (s/actor 2 [0.0 0.0 -20.0] {:team 2}))]
    (is (empty? (perc/sense-targets world 1 senses {}))))
  (testing "but on team 0 everyone is"
    (let [world (s/world-of (s/actor 1 [0.0 0.0 0.0] {:yaw 0.0})
                            (s/actor 2 [0.0 0.0 -20.0]))]
      (is (= [2] (map :id (perc/sense-targets world 1 senses {})))))))

(deftest memory-outlives-the-contact-then-lapses
  (let [o (perc/remember (observer-facing 0.0) 7 1000 senses)]
    (is (= 7 (perc/remembered o 1000)))
    (is (= 7 (perc/remembered o 4000)) "still tracking someone who stepped behind a crate")
    (is (nil? (perc/remembered o 9000)) "but not forever")))

(deftest a-fresh-entity-remembers-nothing
  (is (nil? (perc/remembered (second (s/actor 1 [0.0 0.0 0.0])) 0))))

(deftest remember-falls-back-to-the-declared-memory-duration
  ;; A senses map without :memory-seconds used to produce a NaN deadline under
  ;; ClojureScript — a bot that tracks its target forever, with no error
  ;; anywhere. On the JVM the same call threw. Neither is acceptable; both
  ;; platforms now use the declared default.
  (let [o (perc/remember (observer-facing 0.0) 7 1000 {})]
    (is (= 7 (perc/remembered o 1000)))
    (is (nil? (perc/remembered o 99999))
        "memory still lapses rather than lasting forever"))
  (testing "a nil now-ms is treated as time zero, not as NaN"
    (is (= 7 (perc/remembered (perc/remember (observer-facing 0.0) 7 nil senses) 0)))))
