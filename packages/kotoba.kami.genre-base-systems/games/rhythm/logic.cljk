;; kami-genre-base-systems / rhythm -- gameplay, in the kami-clj subset.
;; Reuses the exact beat? tick-gated pattern proven in
;; gftdcojp/isekai-network games/01-netsurvivors/logic.clj (iteration 7).

(def beat-period   (f32 60.0))
(def beat-window   (f32 4.0))
(def target-period 20)
(def hit-range     (f32 40.0))
(def max-targets   20)

;; maturity pass: a real scoring dimension beyond bare beat-detection --
;; combo rises on consecutive on-beat hits, resets on any off-beat hit.
(defatom combo 0)
(defatom best-combo 0)

(defn beat? []
  (< (mod (tick-n) beat-period) beat-window))

;; complement of beat?, written without `not` (not confirmed proven
;; vocabulary anywhere in this session's reference games) -- a boundary
;; tick where mod exactly equals beat-window is treated as on-beat by
;; beat? and this predicate both returning false there, a negligible
;; single-tick approximation, not a claim of exact logical negation.
(defn off-beat? []
  (< beat-window (mod (tick-n) beat-period)))

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; targets spawn on a ring, capped, tick-gated -- same shape as survivors' spawn.
(defsystem spawn [dt]
  (when (< (count-tagged "target") max-targets)
    (when (zero? (mod (tick-n) target-period))
      (let [r (rand-int 4)
            t (spawn-entity "target")]
        (cond
          (= r 0) (set-position! t (f32 400.0)  (f32 0.0)   (f32 0.0))
          (= r 1) (set-position! t (f32 -400.0) (f32 0.0)   (f32 0.0))
          (= r 2) (set-position! t (f32 0.0)    (f32 400.0) (f32 0.0))
          :else   (set-position! t (f32 0.0)   (f32 -400.0) (f32 0.0)))))))

;; hit: touching a target always succeeds -- an on-beat hit additionally
;; spawns a cosmetic beat-spark and extends the combo; an off-beat hit
;; resets it. Never required to clear, only rewarded with score.
;; (`do` is not confirmed proven vocabulary anywhere in this session's
;; reference games, so the on-beat branch is split across separate `when`
;; forms sharing the same beat? guard, instead of risking an unverified
;; special form.)
(defsystem hit [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [t (nearest-tagged "target" (get-x p) (get-y p) hit-range)]
        (when (not= t -1)
          (despawn-entity t)
          (when (beat?)
            (let [spark (spawn-entity "beat-spark")]
              (set-position! spark (get-x t) (get-y t) (f32 0.0))))
          (when (beat?)
            (set-atom! combo (+ (atom-val combo) 1)))
          (when (beat?)
            (when (< (atom-val best-combo) (atom-val combo))
              (set-atom! best-combo (atom-val combo))))
          (when (off-beat?)
            (set-atom! combo 0)))))))
