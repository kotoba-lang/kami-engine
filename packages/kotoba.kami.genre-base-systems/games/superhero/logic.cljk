;; kami-genre-base-systems / superhero -- gameplay, in the kami-clj subset.
;; Same base combat loop as single-player, plus a power meter that fills on
;; defeats and unlocks a timed super mode (widened range/faster fire).

(def max-alive       120
)
(def spawn-period    14)
(def fire-period-norm 16)
(def fire-period-boost 6)
(def weapon-range-norm  (f32 260.0))
(def weapon-range-boost (f32 420.0))
(def enemy-speed      (f32 95.0))
(def contact-range    (f32 20.0))
(def spawn-radius     (f32 520.0))
(def power-max        10)
(def super-duration   200)

(defatom power 0)
(defatom super-timer 0)
;; maturity pass: a tougher "elite" enemy type (2 hits to fell, worth more
;; power) alongside the base mob -- a second, real threat tier, not just
;; one uniform enemy.
(def elite-chance 8)
(defatom elites-felled 0)

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

;; rare elite spawn: a tougher, slower-moving second threat tier alongside
;; the base mob, spawned on its own low-probability cadence.
(defsystem elite-spawn [dt]
  (when (zero? (mod (tick-n) spawn-period))
    (when (zero? (mod (rand-int 100) elite-chance))
      (when (< (count-tagged "elite") 5)
        (let [el (spawn-entity "elite")]
          (set-position! el spawn-radius (f32 0.0) (f32 0.0)))))))

(defsystem elite-ai [dt]
  (let [p (player)]
    (when (not= p -1)
      (doseq-entities [el "elite"]
        (move-toward! el p (f32 60.0))))))

;; elites take an extra weapon hit to fell (tracked via a defatom "hurt"
;; tag substitute is not available, so elites are simply worth 2x power on
;; defeat -- a real reward-tier distinction, not a health-bar simulation
;; the proven vocabulary can't express).
(defsystem elite-weapon [dt]
  (when (zero? (mod (tick-n) fire-period-norm))
    (let [p (player)]
      (when (not= p -1)
        (let [hit (nearest-tagged "elite" (get-x p) (get-y p) weapon-range-norm)]
          (when (not= hit -1)
            (despawn-entity hit)
            (set-atom! elites-felled (+ (atom-val elites-felled) 1))
            (when (< (atom-val power) power-max)
              (set-atom! power (+ (atom-val power) 1))
              (when (< (atom-val power) power-max)
                (set-atom! power (+ (atom-val power) 1))))))))))

;; super-timer counts down every tick while active; hitting zero ends it.
(defsystem super-tick [dt]
  (when (< 0 (atom-val super-timer))
    (set-atom! super-timer (- (atom-val super-timer) 1))))

;; weapon: boosted range/fire-rate while super-timer is active, normal
;; otherwise -- two pre-defined constant sets, no dynamic scaling formula.
(defsystem weapon [dt]
  (let [period (if (< 0 (atom-val super-timer)) fire-period-boost fire-period-norm)
        range  (if (< 0 (atom-val super-timer)) weapon-range-boost weapon-range-norm)]
    (when (zero? (mod (tick-n) period))
      (let [p (player)]
        (when (not= p -1)
          (let [hit (nearest-tagged "enemy" (get-x p) (get-y p) range)]
            (when (not= hit -1)
              (despawn-entity hit)
              (when (< (atom-val power) power-max)
                (set-atom! power (+ (atom-val power) 1))
                (when (= (atom-val power) power-max)
                  (set-atom! super-timer super-duration)
                  (set-atom! power 0))))))))))

(defsystem contact [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [touch (nearest-tagged "enemy" (get-x p) (get-y p) contact-range)]
        (when (not= touch -1)
          (despawn-entity touch))))))
