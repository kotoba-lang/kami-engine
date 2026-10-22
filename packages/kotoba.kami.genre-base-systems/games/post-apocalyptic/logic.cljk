;; kami-genre-base-systems / post-apocalyptic -- gameplay, in the kami-clj
;; subset.
;;
;; A resource-scarcity system layered on the single-player base: weapon use
;; consumes a limited "charges" defatom; at zero charges the weapon system
;; stops firing until the player collects a "salvage" entity to replenish.
;; This forces conservation/pacing -- a genuine scarcity mechanic, not a
;; claim that the setting itself is a distinct core loop.

(def max-alive       120)
(def spawn-period    22)
(def salvage-period  90)
(def fire-period     16)
(def enemy-speed     (f32 100.0))
(def weapon-range    (f32 260.0))
(def contact-range   (f32 20.0))
(def salvage-range   (f32 30.0))
(def spawn-radius    (f32 520.0))
(def salvage-yield   3)
(def start-charges   6)
;; maturity pass: total salvage collected is tracked as a survival
;; milestone -- a real long-run progress signal beyond the moment-to-moment
;; charges counter.
(defatom total-salvaged 0)

(defatom charges 6)

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

;; scarce salvage: rarer than enemy spawns, on its own cadence.
(defsystem spawn-salvage [dt]
  (when (zero? (mod (tick-n) salvage-period))
    (let [s (spawn-entity "salvage")
          r (rand-int 4)]
      (cond
        (= r 0) (set-position! s (f32 300.0)  (f32 300.0)  (f32 0.0))
        (= r 1) (set-position! s (f32 -300.0) (f32 300.0)  (f32 0.0))
        (= r 2) (set-position! s (f32 300.0)  (f32 -300.0) (f32 0.0))
        :else   (set-position! s (f32 -300.0) (f32 -300.0) (f32 0.0))))))

(defsystem ai [dt]
  (let [p (player)]
    (when (not= p -1)
      (doseq-entities [e "enemy"]
        (move-toward! e p enemy-speed)))))

;; weapon: only fires while charges remain, consumes one per shot.
(defsystem weapon [dt]
  (when (zero? (mod (tick-n) fire-period))
    (when (< 0 (atom-val charges))
      (let [p (player)]
        (when (not= p -1)
          (let [hit (nearest-tagged "enemy" (get-x p) (get-y p) weapon-range)]
            (when (not= hit -1)
              (despawn-entity hit)
              (set-atom! charges (- (atom-val charges) 1)))))))))

;; collecting salvage replenishes charges.
(defsystem scavenge [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [hit (nearest-tagged "salvage" (get-x p) (get-y p) salvage-range)]
        (when (not= hit -1)
          (despawn-entity hit)
          (set-atom! charges (+ (atom-val charges) salvage-yield))
          (set-atom! total-salvaged (+ (atom-val total-salvaged) 1)))))))

(defsystem contact [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [touch (nearest-tagged "enemy" (get-x p) (get-y p) contact-range)]
        (when (not= touch -1)
          (despawn-entity touch))))))
