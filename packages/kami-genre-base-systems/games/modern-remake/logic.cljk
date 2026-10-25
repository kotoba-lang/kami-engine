;; kami-genre-base-systems / modern-remake -- gameplay, in the kami-clj subset.
;;
;; A tuning-PRESET demo, not a new mechanic (see design/modern-remake.edn):
;; the exact same survivors-shaped loop as single-player/logic.clj, with
;; "modernization" adjustments applied to the tuning constants -- wider
;; contact tolerance and a much larger weapon-range (auto-aim-style),
;; matching how a modern remake typically loosens tolerances rather than
;; changing the underlying loop. Every construct here is identical to
;; single-player/logic.clj's proven set; only the numeric tuning differs.

;; maturity pass: a third modernization axis -- a gentler spawn-period
;; (30 vs single-player's 20) alongside the two range adjustments, matching
;; how modern re-releases commonly also ease base difficulty pacing, not
;; just hit-tolerance.
(def max-alive     150)
(def spawn-period  30)
(def fire-period   16)
(def enemy-speed   (f32 95.0))
;; modernized: 2x single-player's 260.0 weapon-range (auto-aim feel)
(def weapon-range  (f32 520.0))
;; modernized: 2.5x single-player's 20.0 contact-range (forgiving hitbox)
(def contact-range (f32 50.0))
(def spawn-radius  (f32 520.0))

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

(defsystem spawn [dt]
  (when (< (count-tagged "enemy") max-alive)
    (when (zero? (mod (tick-n) spawn-period))
      (let [r (rand-int 4)
            e (spawn-entity "enemy")]
        (cond
          (= r 0) (set-position! e spawn-radius (f32 0.0)    (f32 0.0))
          (= r 1) (set-position! e (f32 -520.0) (f32 0.0)    (f32 0.0))
          (= r 2) (set-position! e (f32 0.0)    spawn-radius (f32 0.0))
          :else   (set-position! e (f32 0.0)    (f32 -520.0) (f32 0.0)))))))

(defsystem ai [dt]
  (let [p (player)]
    (when (not= p -1)
      (doseq-entities [e "enemy"]
        (move-toward! e p enemy-speed)))))

(defsystem weapon [dt]
  (when (zero? (mod (tick-n) fire-period))
    (let [p (player)]
      (when (not= p -1)
        (let [hit (nearest-tagged "enemy" (get-x p) (get-y p) weapon-range)]
          (when (not= hit -1)
            (despawn-entity hit)))))))

(defsystem contact [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [touch (nearest-tagged "enemy" (get-x p) (get-y p) contact-range)]
        (when (not= touch -1)
          (despawn-entity touch))))))
