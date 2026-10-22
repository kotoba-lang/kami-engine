(ns kami-genre-base-systems.platformer.style
  "Per-encounter combo counter and decaying style meter -> letter rank.
   No precedent elsewhere in this workspace; kept deliberately simple:
   hits without taking damage build the meter and combo, taking damage
   knocks the meter down and resets the combo, and the meter also decays
   continuously (and the combo times out) so standing still doesn't
   preserve an old rank forever. Pure data in, data out.")

(def default-meter-max 100.0)
(def default-decay-per-second
  "Passive meter decay, style points/second, applied every tick regardless
   of hits (a 'use it or lose it' clock, distinct from the damage penalty)."
  8.0)
(def default-hit-gain "Style points awarded per un-punished hit." 6.0)
(def default-damage-penalty
  "Style points removed the instant the player takes damage (harsher than
   passive decay, but not a full reset -- a single hit shouldn't erase an
   entire run's build-up as harshly as zeroing it would)."
  30.0)
(def default-combo-timeout
  "Seconds since the last landed hit after which the combo counter (not
   the meter) resets to zero."
  1.5)

(def rank-thresholds
  "Meter value at/above which each rank applies, checked highest-first."
  [[:s 90.0] [:a 65.0] [:b 40.0] [:c 20.0] [:d 0.0]])

(defn meter->rank [meter]
  (or (some (fn [[rank threshold]] (when (>= meter threshold) rank)) rank-thresholds)
      :d))

(defn style-idle []
  {:combo 0 :meter 0.0 :rank :d :since-last-hit 0.0})

(defn register-hit
  "Call when the player lands a hit without having taken damage since the
   previous one. Increments combo, raises the meter (clamped to
   meter-max), and recomputes rank."
  ([style-state] (register-hit style-state default-hit-gain default-meter-max))
  ([{:keys [combo meter] :as style-state} gain meter-max]
   (let [meter' (min meter-max (+ meter gain))]
     (assoc style-state
            :combo (inc combo)
            :meter meter'
            :rank (meter->rank meter')
            :since-last-hit 0.0))))

(defn register-damage-taken
  "Call when the player takes damage: resets the combo counter to zero and
   knocks the meter down by `penalty` (floored at zero)."
  ([style-state] (register-damage-taken style-state default-damage-penalty))
  ([{:keys [meter] :as style-state} penalty]
   (let [meter' (max 0.0 (- meter penalty))]
     (assoc style-state :combo 0 :meter meter' :rank (meter->rank meter')))))

(defn step-style
  "Advances style-state by dt: the meter decays continuously toward zero,
   and the combo resets to zero once default-combo-timeout seconds have
   passed without a landed hit."
  ([style-state dt] (step-style style-state dt default-decay-per-second default-combo-timeout))
  ([{:keys [meter combo since-last-hit] :or {since-last-hit 0.0}} dt decay-per-second combo-timeout]
   (let [since' (+ since-last-hit dt)
         meter' (max 0.0 (- meter (* decay-per-second dt)))
         combo' (if (>= since' combo-timeout) 0 combo)]
     {:meter meter' :combo combo' :rank (meter->rank meter') :since-last-hit since'})))
