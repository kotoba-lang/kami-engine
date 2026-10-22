;; kami-genre-base-systems / platformer -- gameplay, in the kami-clj subset.
;;
;; NO jump/gravity primitive exists anywhere in the proven vocabulary or any
;; reference game -- an honest limitation, not an oversight (see
;; design/platformer.edn). This base is a sequential-waypoint traversal: a
;; fixed chain of "platform" entities the player must reach IN ORDER, with
;; the current target highlighted as "next" via a defatom sequence-index.
;; This is the closest honest analogue to jump-across-gaps progression
;; available without real physics/collision.

(def num-platforms 5)
(def reach-range    (f32 35.0))

(defatom seq-index 0)
;; maturity pass: a second entity type, a fixed "hazard" -- touching one
;; resets the sequence back to the start, giving the traversal real stakes
;; beyond pure progression.
(def hazard-range (f32 30.0))
(defatom falls 0)

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

;; fixed platform chain, laid out once at init -- positions are absolute
;; f32 constants (guest arithmetic is integer-only, matching every other
;; base system's convention).
(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 -800.0) (f32 0.0) (f32 0.0)))
  (let [a (spawn-entity "next")] (set-position! a (f32 -400.0) (f32 60.0)  (f32 0.0)))
  (let [b (spawn-entity "platform")] (set-position! b (f32 0.0)   (f32 -40.0) (f32 0.0)))
  (let [c (spawn-entity "platform")] (set-position! c (f32 400.0) (f32 90.0)  (f32 0.0)))
  (let [d (spawn-entity "platform")] (set-position! d (f32 800.0) (f32 0.0)  (f32 0.0)))
  (let [h (spawn-entity "hazard")] (set-position! h (f32 200.0) (f32 -20.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; reaching the current "next" platform advances the sequence and re-tags
;; the next "platform" in the chain as "next" -- position order matches
;; init's spawn order (-400 -> 0 -> 400 -> 800), tracked via seq-index.
(defsystem advance [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [hit (nearest-tagged "next" (get-x p) (get-y p) reach-range)]
        (when (not= hit -1)
          (despawn-entity hit)
          (set-atom! seq-index (+ (atom-val seq-index) 1))
          (let [remaining (nearest-tagged "platform" (f32 -1000.0) (f32 0.0) (f32 2000.0))]
            (when (not= remaining -1)
              (let [x (get-x remaining) y (get-y remaining)]
                (despawn-entity remaining)
                (let [n (spawn-entity "next")]
                  (set-position! n x y (f32 0.0)))))))))))

;; touching a hazard resets the run back to the first waypoint.
(defsystem hazard-check [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [h (nearest-tagged "hazard" (get-x p) (get-y p) hazard-range)]
        (when (not= h -1)
          (set-atom! falls (+ (atom-val falls) 1))
          (set-atom! seq-index 0)
          (set-position! p (f32 -800.0) (f32 0.0) (f32 0.0)))))))
