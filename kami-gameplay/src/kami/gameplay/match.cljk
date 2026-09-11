(ns kami.gameplay.match
  "GameMode and GameState — who is still alive, who placed where, when it ends.

  The shipped royale has no match: bots respawn on a ring forever, nothing is
  counted, and there is no state in which the player has won. A battle royale
  without a terminal state is a screensaver.

  Placement is assigned on death, counting down from the number of entrants, so
  the last survivor is #1 without anyone having to compute it afterwards. It is
  written onto the dying entity, which means a spectating player can be shown
  their placement immediately and a match record is just the entity table."
  (:require [kami.gameplay.attributes :as attr]))

(defn alive-ids [world]
  (->> (:entities world)
       (keep (fn [[id e]] (when (attr/alive? e) id)))
       sort vec))

(defn alive-count [world] (count (alive-ids world)))

(defn begin
  "Initialise the match record. `:entrants` is fixed at the start so placements
  stay stable even if late spawns are added (they place last, correctly)."
  [world]
  (assoc world :match {:state :live
                       :entrants (alive-count world)
                       :next-place (alive-count world)
                       :started-at-ms (get-in world [:clock :now-ms] 0)
                       :ended-at-ms nil
                       :winner nil}))

(defn record-deaths
  "Assign placements to everyone who died since the last call.

  Detects deaths by comparing against the previous alive set rather than by
  listening to kill events, so a death from any source — bullet, storm, fall,
  a future one nobody has written yet — is placed. Coupling this to the damage
  pipeline instead would silently miss the source added last."
  [world previous-alive]
  (let [now-alive (set (alive-ids world))
        died (remove now-alive previous-alive)]
    (reduce (fn [w id]
              (let [place (get-in w [:match :next-place])]
                (-> w
                    (update-in [:entities id] attr/set :place (double place))
                    (update-in [:match :next-place] #(max 1 (dec (long %)))))))
            world
            (sort died))))

(defn resolve-end
  "Close the match when one or zero entrants remain.

  Zero is a real outcome, not an error: a final storm tick can kill the last
  two players in the same frame, and a mode that refuses to end there hangs."
  [world]
  (let [alive (alive-ids world)]
    (if (and (= :live (get-in world [:match :state])) (<= (count alive) 1))
      (let [winner (first alive)
            now (get-in world [:clock :now-ms] 0)]
        (cond-> (-> world
                    (assoc-in [:match :state] :ended)
                    (assoc-in [:match :ended-at-ms] now)
                    (assoc-in [:match :winner] winner))
          winner (update-in [:entities winner] attr/set :place 1.0)))
      world)))

(defn live? [world] (= :live (get-in world [:match :state])))
(defn ended? [world] (= :ended (get-in world [:match :state])))

(defn standings
  "Final table, best placement first. Entities still alive sort ahead of the
  dead, so calling this mid-match gives a sensible leaderboard too."
  [world]
  (->> (:entities world)
       (map (fn [[id e]]
              {:id id
               :tag (:tag e)
               :place (long (attr/get e :place))
               :kills (long (attr/get e :kills))
               :damage (attr/get e :damage-dealt)
               :alive? (attr/alive? e)}))
       (sort-by (fn [r] [(if (:alive? r) 0 1)
                         (if (zero? (:place r)) 9999 (:place r))
                         (- (:kills r))]))
       vec))
