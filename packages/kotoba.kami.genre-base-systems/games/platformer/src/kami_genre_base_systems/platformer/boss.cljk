(ns kami-genre-base-systems.platformer.boss
  "Boss phase framework: an ordered list of HP-threshold-keyed phases,
   each cycling through its own attack-pattern table on a fixed interval.
   Pure step function, self-threaded like the other systems here (each
   tick's return value is next tick's `current-phase` argument).")

(defn phase-for-hp
  "`phases` is an ordered coll of {:id :hp-at-or-below :attacks
   :attack-interval}, authored descending by :hp-at-or-below (e.g. 1.0,
   0.66, 0.33 for thirds of max HP, or absolute HP values -- caller's
   choice, as long as `hp` uses the same scale). Returns the phase with
   the SMALLEST :hp-at-or-below that is still >= hp -- i.e. the tightest
   threshold hp still qualifies for, not merely the first one in list
   order (every phase after the first typically also satisfies `hp <=
   1.0`, so 'first match' would wrongly always pick phase 1). hp above
   every threshold defaults to the first (highest/initial) phase; hp
   below every threshold settles on the last (lowest/final) phase."
  [phases hp]
  (reduce (fn [current phase]
            (if (<= hp (:hp-at-or-below phase)) phase current))
          (first phases)
          phases))

(defn step-boss-phase
  "Pure boss-phase step: (phases, hp, current-phase, elapsed) -> next
   state, where `current-phase` is the previous tick's return value of
   this same function (or nil on the very first tick) and `elapsed` is
   this tick's dt in seconds.

   On a phase change (hp crosses into a new phase's threshold), the
   attack table restarts at index 0 rather than preserving position in
   the old phase's table. Returns:
     {:phase          <phase :id>
      :phase-changed? <bool, true the tick the phase first changes>
      :attack-index   <index into this phase's :attacks>
      :next-attack    <(:attacks phase) at attack-index>
      :elapsed-in-attack <seconds spent on the current attack-index>}"
  [phases hp current-phase elapsed]
  (let [phase (phase-for-hp phases hp)
        phase-changed? (or (nil? current-phase) (not= (:phase current-phase) (:id phase)))
        attack-index (if phase-changed? 0 (:attack-index current-phase 0))
        elapsed-in-attack (if phase-changed?
                             0.0
                             (+ (:elapsed-in-attack current-phase 0.0) elapsed))
        attacks (:attacks phase)
        advance? (>= elapsed-in-attack (:attack-interval phase))
        attack-index' (if advance? (mod (inc attack-index) (count attacks)) attack-index)
        elapsed-in-attack' (if advance? 0.0 elapsed-in-attack)]
    {:phase (:id phase)
     :phase-changed? phase-changed?
     :attack-index attack-index'
     :next-attack (nth attacks attack-index')
     :elapsed-in-attack elapsed-in-attack'}))
