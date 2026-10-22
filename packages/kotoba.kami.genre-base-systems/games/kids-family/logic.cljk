;; kami-genre-base-systems / kids-family -- gameplay, in the kami-clj subset.
;;
;; Deliberately the simplest base in this repo: zero-fail collection loop.
;; No ai-tick chase system at all -- stationary "treat" entities, touching
;; one collects it, a new one spawns elsewhere. No timers, no loss condition.

(def max-treats     8)
(def spawn-period    24)
(def collect-range   (f32 30.0))
(def spawn-half      (f32 380.0))

(defatom collected 0)

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0)))
  ;; maturity pass: a gentle companion that follows the player and helps
  ;; collect nearby treats -- a real second interaction beyond solo
  ;; collection, still zero-fail, still no chase-AI-toward-the-player-
  ;; threat shape.
  (let [c (spawn-entity "companion")]
    (set-position! c (f32 40.0) (f32 40.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

(defsystem spawn [dt]
  (when (< (count-tagged "treat") max-treats)
    (when (zero? (mod (tick-n) spawn-period))
      (let [r (rand-int 4)
            e (spawn-entity "treat")]
        (cond
          (= r 0) (set-position! e spawn-half (f32 0.0)     (f32 0.0))
          (= r 1) (set-position! e (f32 -0.0) spawn-half     (f32 0.0))
          (= r 2) (set-position! e (f32 0.0)  (f32 -0.0)     (f32 0.0))
          :else   (set-position! e spawn-half spawn-half     (f32 0.0)))))))

;; collection: no fail state, no timer -- purely player-paced.
(defsystem collect [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [hit (nearest-tagged "treat" (get-x p) (get-y p) collect-range)]
        (when (not= hit -1)
          (despawn-entity hit)
          (set-atom! collected (+ (atom-val collected) 1)))))))

;; companion follows the player at a gentle distance and can also collect
;; nearby treats itself.
(defsystem companion-follow [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [c (nearest-tagged "companion" (f32 0.0) (f32 0.0) (f32 1000000.0))]
        (when (not= c -1)
          (move-toward! c p (f32 150.0)))))))

(defsystem companion-collect [dt]
  (let [c (nearest-tagged "companion" (f32 0.0) (f32 0.0) (f32 1000000.0))]
    (when (not= c -1)
      (let [hit (nearest-tagged "treat" (get-x c) (get-y c) collect-range)]
        (when (not= hit -1)
          (despawn-entity hit)
          (set-atom! collected (+ (atom-val collected) 1)))))))
