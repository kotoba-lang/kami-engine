(ns kami.gameplay.match-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.match :as match]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.support :as s]))

(defn- w3 []
  (-> (s/world-of (s/actor 1 [0.0 0.0 0.0] {:tag "player"})
                  (s/actor 2 [10.0 0.0 0.0])
                  (s/actor 3 [20.0 0.0 0.0]))
      (assoc :clock {:now-ms 0})
      match/begin))

(defn- kill [w id] (update-in w [:entities id] #(-> % (attr/set :health 0) (attr/set :alive 0))))

(deftest begin-fixes-the-entrant-count
  (let [w (w3)]
    (is (= 3 (get-in w [:match :entrants])))
    (is (= :live (get-in w [:match :state])))
    (is (match/live? w))))

(deftest placements-count-down-so-the-survivor-is-first
  (let [w (w3)
        alive0 (set (match/alive-ids w))
        w1 (match/record-deaths (kill w 3) alive0)
        alive1 (set (match/alive-ids w1))
        w2 (match/record-deaths (kill w1 2) alive1)]
    (is (= 3.0 (attr/get (get-in w2 [:entities 3]) :place)) "first out places last")
    (is (= 2.0 (attr/get (get-in w2 [:entities 2]) :place)))
    (let [w3' (match/resolve-end w2)]
      (is (match/ended? w3'))
      (is (= 1 (get-in w3' [:match :winner])))
      (is (= 1.0 (attr/get (get-in w3' [:entities 1]) :place))))))

(deftest deaths-are-detected-by-comparison-not-by-event
  (testing "so a death from a source nobody has written yet is still placed"
    (let [w (w3)
          alive0 (set (match/alive-ids w))
          ;; killed by nothing in particular — no damage event was emitted
          w' (match/record-deaths (assoc-in w [:entities 2 :attrs :alive] 0.0) alive0)]
      (is (= 3.0 (attr/get (get-in w' [:entities 2]) :place))))))

(deftest a-simultaneous-wipe-ends-the-match-rather-than-hanging
  (let [w (w3)
        alive0 (set (match/alive-ids w))
        w' (-> w (kill 1) (kill 2) (kill 3) (match/record-deaths alive0) match/resolve-end)]
    (is (match/ended? w'))
    (is (nil? (get-in w' [:match :winner])) "zero survivors is a real outcome")))

(deftest a-three-way-match-does-not-end-early
  (is (match/live? (match/resolve-end (w3))))
  (is (match/live? (match/resolve-end (kill (w3) 3)))))

(deftest standings-put-the-living-first-then-by-place-then-by-kills
  (let [w (-> (w3)
              (update-in [:entities 2] attr/set :kills 5)
              (update-in [:entities 3] attr/set :kills 1)
              (kill 3))
        w (match/record-deaths w #{1 2 3})
        rows (match/standings w)]
    (is (= [2 1 3] (map :id rows))
        "both survivors outrank the dead; between them the one with 5 kills leads")
    (is (true? (:alive? (first rows))))
    (is (= 5 (:kills (first rows))))
    (is (= 3 (:place (last rows))))))

(deftest alive-count-tracks-deaths
  (let [w (w3)]
    (is (= 3 (match/alive-count w)))
    (is (= 2 (match/alive-count (kill w 1))))))
