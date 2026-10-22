;; kami-genre-base-systems / horror -- gameplay, in the kami-clj subset.
;; Dread rises near a threat, decays otherwise. Rare random scares spawn
;; briefly near the player then despawn via a countdown.

(def detect-range (f32 140.0))
(def dread-max    100)
(def scare-chance 6)
(def scare-check-period 30)
(def scare-life   20)

(defatom dread 0)
(defatom scare-timer 0)
;; maturity pass: a fixed "safe zone" light source that decays dread much
;; faster when nearby -- a real tension-relief mechanic, not just a single
;; rising/falling meter.
(def safe-range   (f32 100.0))

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0)))
  (let [t (spawn-entity "threat")]
    (set-position! t (f32 300.0) (f32 0.0) (f32 0.0)))
  (let [s (spawn-entity "safe-zone")]
    (set-position! s (f32 -300.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; the threat drifts toward the player, same shape as ai-tick elsewhere.
(defsystem stalk [dt]
  (let [p (player)]
    (when (not= p -1)
      (doseq-entities [t "threat"]
        (move-toward! t p (f32 45.0))))))

(defsystem dread-tick [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [t (nearest-tagged "threat" (get-x p) (get-y p) detect-range)
            s (nearest-tagged "safe-zone" (get-x p) (get-y p) safe-range)]
        (if (not= t -1)
          (when (< (atom-val dread) dread-max)
            (set-atom! dread (+ (atom-val dread) 2)))
          (if (not= s -1)
            (when (< 0 (atom-val dread))
              (set-atom! dread (- (atom-val dread) 3)))
            (when (< 0 (atom-val dread))
              (set-atom! dread (- (atom-val dread) 1)))))))))

;; rare random scare: low-probability roll every scare-check-period ticks
;; spawns a short-lived scare entity near the player.
(defsystem scare-spawn [dt]
  (when (zero? (mod (tick-n) scare-check-period))
    (when (zero? (mod (rand-int 100) scare-chance))
      (let [p (player)]
        (when (not= p -1)
          (let [s (spawn-entity "scare")]
            (set-position! s (get-x p) (get-y p) (f32 0.0))
            (set-atom! scare-timer scare-life)))))))

(defsystem scare-countdown [dt]
  (when (< 0 (atom-val scare-timer))
    (set-atom! scare-timer (- (atom-val scare-timer) 1))
    (when (zero? (atom-val scare-timer))
      (let [s (nearest-tagged "scare" (f32 0.0) (f32 0.0) (f32 1000000.0))]
        (when (not= s -1)
          (despawn-entity s))))))
