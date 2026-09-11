(ns kami.gameplay.zone
  "The storm — a shrinking safe circle that forces a match to end.

  `kami-game-scene/data/battle_royale_storm.edn` has described eight phases
  (wait, shrink, end radius, damage per second) since ADR-0046, and, like the
  weapon table, nothing has been able to run it since its Rust oracle was
  removed. Without it a battle royale has no clock: the shipped royale spawns
  bots forever on a fixed ring and nothing ever brings the fight together.

  The phase model is a two-part cycle per phase — hold, then contract — with
  the centre drifting to a new point that stays inside the *old* circle. That
  containment rule is the one thing a storm must guarantee: a circle that
  wandered outside its predecessor would put players who correctly moved to the
  centre into the damage zone through no error of their own.

  Radius is a pure function of elapsed time, so the storm is identical for every
  client without being replicated, and a replay lands on the same circle."
  (:require [kami.gameplay.vec3 :as v]
            [kami.gameplay.rng :as rng]
            [kami.gameplay.attributes :as attr]))

(def default-phases
  "Fallback schedule, used only when a scene supplies none. Mirrors the shipped
  `battle_royale_storm.edn` so the fallback and the data agree — a fallback that
  quietly differs from the file is worse than no fallback."
  [{:phase 0 :wait 120.0 :shrink 90.0 :end-radius 700.0 :dps 1.0}
   {:phase 1 :wait 90.0 :shrink 75.0 :end-radius 450.0 :dps 2.0}
   {:phase 2 :wait 75.0 :shrink 60.0 :end-radius 280.0 :dps 5.0}
   {:phase 3 :wait 60.0 :shrink 45.0 :end-radius 150.0 :dps 8.0}
   {:phase 4 :wait 45.0 :shrink 30.0 :end-radius 70.0 :dps 10.0}
   {:phase 5 :wait 30.0 :shrink 20.0 :end-radius 25.0 :dps 15.0}
   {:phase 6 :wait 20.0 :shrink 15.0 :end-radius 5.0 :dps 20.0}
   {:phase 7 :wait 15.0 :shrink 10.0 :end-radius 0.0 :dps 25.0}])

(defn load-phases
  "Normalise a parsed `battle_royale_storm.edn` (or a bare vector) into a
  sorted, double-coerced phase vector."
  [data]
  (let [rows (cond (map? data) (:battle-royale/storm-phases data)
                   (sequential? data) data
                   :else nil)]
    (if (seq rows)
      (vec (sort-by :phase (map (fn [p]
                                  (-> p
                                      (update :wait double)
                                      (update :shrink double)
                                      (update :end-radius double)
                                      (update :dps double)))
                                rows)))
      default-phases)))

(defn- phase-duration [p] (+ (double (:wait p)) (double (:shrink p))))

(defn plan
  "Precompute per-phase start radius, centre and absolute time window.

  Doing this once at match start, rather than integrating each tick, is what
  makes `state-at` a pure function of elapsed seconds — and therefore what makes
  the storm replayable and identical across clients.

  Returns `{:phases [...] :total-seconds n :start-radius r :rng rng'}`."
  [{:keys [phases start-radius center rng-seed]
    :or {start-radius 1000.0 center [0.0 0.0 0.0] rng-seed 7}}]
  (let [phases (load-phases phases)]
    (loop [ps phases, r (double start-radius), c center, t 0.0, rs (rng/seed rng-seed), out []]
      (if-let [p (first ps)]
        (let [end-r (double (:end-radius p))
              ;; the new centre must sit inside the *old* circle with the new
              ;; circle wholly contained: offset is capped at (r - end-r).
              max-off (max 0.0 (- r end-r))
              [ang rs1] (rng/range-f rs 0.0 (* 2.0 Math/PI))
              [frac rs2] (rng/unit rs1)
              off (* max-off (Math/sqrt frac))
              c' [(+ (v/x c) (* off (Math/cos ang)))
                  (v/y c)
                  (+ (v/z c) (* off (Math/sin ang)))]
              wait (double (:wait p))
              shrink (double (:shrink p))]
          (recur (rest ps) end-r c' (+ t wait shrink) rs2
                 (conj out {:phase (:phase p)
                            :dps (double (:dps p))
                            :start-radius r
                            :end-radius end-r
                            :from-center c
                            :to-center c'
                            :hold-start t
                            :shrink-start (+ t wait)
                            :phase-end (+ t wait shrink)})))
        {:phases out
         :start-radius (double start-radius)
         :start-center center
         :total-seconds (reduce + 0.0 (map phase-duration phases))
         :rng rs}))))

(defn state-at
  "The storm circle at `t` seconds into the match.

  Returns `{:phase :radius :center :dps :shrinking?}`. Before the first phase
  ends the circle is the starting one; after the last, it is the final one held
  forever — a match that outlives its schedule should be lethal everywhere, not
  suddenly safe."
  [{:keys [phases start-radius start-center] :as _plan} t]
  (let [t (double t)]
    (if-let [cur (or (first (filter #(< t (:phase-end %)) phases)) nil)]
      (if (< t (:shrink-start cur))
        {:phase (:phase cur) :radius (:start-radius cur) :center (:from-center cur)
         :dps (:dps cur) :shrinking? false}
        (let [span (- (:phase-end cur) (:shrink-start cur))
              k (if (pos? span) (/ (- t (:shrink-start cur)) span) 1.0)
              k (max 0.0 (min 1.0 k))]
          {:phase (:phase cur)
           :radius (+ (:start-radius cur) (* k (- (:end-radius cur) (:start-radius cur))))
           :center (v/lerp (:from-center cur) (:to-center cur) k)
           :dps (:dps cur)
           :shrinking? true}))
      (if-let [last-p (last phases)]
        {:phase (:phase last-p) :radius (:end-radius last-p) :center (:to-center last-p)
         :dps (:dps last-p) :shrinking? false}
        {:phase -1 :radius (double start-radius) :center start-center :dps 0.0 :shrinking? false}))))

(defn inside?
  "Whether a world position is within the safe circle. Ground-plane only —
  jumping does not save you from the storm."
  [zone pos]
  (<= (v/dist-xz pos (:center zone)) (:radius zone)))

(defn distance-outside
  "How far outside the circle a position is; 0.0 when safe. Used by the AI to
  decide how urgently to rotate."
  [zone pos]
  (max 0.0 (- (v/dist-xz pos (:center zone)) (:radius zone))))

(defn safe-point
  "A point inside the circle to move toward, biased `inset` in from the edge.

  Returns the centre when already inside — a bot that is safe should be free to
  fight rather than pointlessly walking to the middle."
  [zone pos inset]
  (if (inside? zone pos)
    pos
    (let [c (:center zone)
          dir (v/normalize (v/with-y (v/sub pos c) 0.0))
          r (max 0.0 (- (:radius zone) (double inset)))]
      (v/add c (v/scale dir r)))))

(defn tick-damage
  "Damage descriptors for every alive entity outside the circle this tick.

  Feeds `damage/apply-many` rather than touching health, so a storm death shows
  up in the kill feed and in the placement record like any other."
  [world zone dt]
  (let [dps (double (:dps zone))]
    (if-not (pos? dps)
      []
      (keep (fn [[id e]]
              (when (and (attr/alive? e) (not (inside? zone (:pos e))))
                {:target id :amount (* dps (double dt)) :source :storm}))
            (:entities world)))))
