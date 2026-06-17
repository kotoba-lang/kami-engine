;; survivors-runtime authored in kami-clj — the gameka "DEADLINE" gameSpec
;; mechanics (survivors-zombie) expressed as a Clojure game script that the
;; kami-clj compiler lowers to WASM and kami-script-runtime drives over hecs.
;;
;; This is the interpret-at-runtime alternative to the JS-canvas P0 prototype:
;; ONE shared script; the gameSpec's numeric knobs (cap / fire rate / speeds /
;; ranges) fold in as defs. Demonstrates the full survivors core loop:
;;   control → wave spawn (rng-gated, capped) → enemy AI (chase all) →
;;   auto-fire (cull nearest in range) → contact (enemy reaching player).

;; --- tuning (from specs/survivors-zombie.gamespec.edn, integer/permille) ---
(def max-alive     200)      ;; :waves/max-alive
(def spawn-period  20)       ;; ticks between spawns
(def fire-period   30)       ;; ticks between auto-fire shots
(def enemy-speed   (f32 120.0))   ;; px/s (scaled up from spec for test motion)
(def weapon-range  (f32 220.0))   ;; px — auto-fire reach
(def contact-range (f32 18.0))    ;; px — enemy touches player
(def spawn-radius  (f32 300.0))   ;; px — enemies appear on this ring

;; The player is a singleton; resolve its id by nearest-tag from the origin.
(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0))))

;; twin-stick movement: feed analog axes into the player's velocity.
(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; wave spawning: honour the alive cap, gate on the tick clock, drop the new
;; enemy on one of four ring points chosen by the host PRNG.
(defsystem spawn [dt]
  (when (< (count-tagged "enemy") max-alive)
    (when (zero? (mod (tick-n) spawn-period))
      (let [r (rand-int 4)
            e (spawn-entity "enemy")]
        (cond
          (= r 0) (set-position! e spawn-radius (f32 0.0) (f32 0.0))
          (= r 1) (set-position! e (f32 -300.0) (f32 0.0) (f32 0.0))
          (= r 2) (set-position! e (f32 0.0) spawn-radius (f32 0.0))
          :else   (set-position! e (f32 0.0) (f32 -300.0) (f32 0.0)))))))

;; every enemy walks toward the player (host does the normalize×speed).
(defsystem ai [dt]
  (let [p (player)]
    (when (not= p -1)
      (doseq-entities [e "enemy"]
        (move-toward! e p enemy-speed)))))

;; auto-fire: each shot removes the nearest enemy within range (hitscan proxy).
(defsystem weapon [dt]
  (when (zero? (mod (tick-n) fire-period))
    (let [p (player)]
      (when (not= p -1)
        (let [hit (nearest-tagged "enemy" (get-x p) (get-y p) weapon-range)]
          (when (not= hit -1)
            (despawn-entity hit)
            (play-sound "shot")))))))

;; contact: an enemy that reaches the player is consumed (damage proxy).
(defsystem contact [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [touch (nearest-tagged "enemy" (get-x p) (get-y p) contact-range)]
        (when (not= touch -1)
          (despawn-entity touch))))))
