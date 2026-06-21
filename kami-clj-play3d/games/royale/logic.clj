;; KAMI Royale — gameplay in the kami-clj subset (3rd-person battle-royale demo).
;;
;; Ground is the (x, y) plane; the host renders it as world (x, 0, y) and owns the
;; camera, gravity, jump height, and 3D presentation. This file owns the *game*:
;; the player's ground velocity from input, and the AI bots that hunt the player.
;; Guest arithmetic is integer-only → positions are absolute f32 constants.

(def max-bots    24)
(def spawn-every 30)
(def bot-speed   (f32 60.0))
(def hunt-range  (f32 900.0))
(def ring        (f32 700.0))
(def fire-every   12)            ;; ticks between auto-fire shots
(def weapon-range (f32 340.0))   ;; auto-fire reach (ground units)

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 9000000.0)))

(defn init []
  (set-position! (spawn-entity "player") (f32 0.0) (f32 0.0) (f32 0.0)))

;; movement: host feeds camera-relative ground axes (already px/s) into velocity.
(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; drop bots on a ring around the origin until the lobby is full.
(defsystem spawn [dt]
  (when (< (count-tagged "bot") max-bots)
    (when (zero? (mod (tick-n) spawn-every))
      (let [r (rand-int 4)
            e (spawn-entity "bot")]
        (cond
          (= r 0) (set-position! e ring        (f32 0.0)   (f32 0.0))
          (= r 1) (set-position! e (f32 -700.0) (f32 0.0)   (f32 0.0))
          (= r 2) (set-position! e (f32 0.0)    ring        (f32 0.0))
          :else   (set-position! e (f32 0.0)    (f32 -700.0)(f32 0.0)))))))

;; every bot within range hunts the player across the ground plane.
(defsystem ai [dt]
  (let [p (player)]
    (when (not= p -1)
      (doseq-entities [e "bot"]
        (let [near (nearest-tagged "player" (get-x e) (get-y e) hunt-range)]
          (when (not= near -1)
            (move-toward! e near bot-speed)))))))

;; auto-fire: on the fire cadence, cull the nearest bot within weapon range.
;; The host renders the hit (particle burst) by watching for bots that vanish.
(defsystem weapon [dt]
  (let [p (player)]
    (when (not= p -1)
      (when (zero? (mod (tick-n) fire-every))
        (let [hit (nearest-tagged "bot" (get-x p) (get-y p) weapon-range)]
          (when (not= hit -1)
            (despawn-entity hit)))))))
