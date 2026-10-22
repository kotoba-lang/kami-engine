;; kami-genre-base-systems / sports -- gameplay, in the kami-clj subset.
;; Abstract ball-and-goals: proximity pushes the ball toward the far goal;
;; crossing a fixed x-threshold scores and respawns the ball at center.

(def push-range   (f32 40.0))
(def push-speed   (f32 220.0))
(def goal-x       (f32 480.0))
(def opp-goal-x   (f32 -480.0))
(def opp-speed    (f32 170.0))

;; maturity pass: score-left was declared but dead (nothing ever pushed the
;; ball toward the -x goal, so it could never increment). Fixed by giving
;; the sports base a real opponent -- same honest "1 human + 1 AI" shape
;; local-multiplayer/fighting already established for this engine's single-
;; input-stream constraint -- so both goals are genuinely reachable.
(defatom score-right 0)
(defatom score-left  0)

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))
(defn opponent []
  (nearest-tagged "opponent" (f32 0.0) (f32 0.0) (f32 1000000.0)))
(defn ball []
  (nearest-tagged "ball" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 -200.0) (f32 0.0) (f32 0.0)))
  (let [o (spawn-entity "opponent")]
    (set-position! o (f32 200.0) (f32 0.0) (f32 0.0)))
  (let [b (spawn-entity "ball")]
    (set-position! b (f32 0.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; push: player proximity to the ball sends it toward the +x goal.
(defsystem push [dt]
  (let [p (player)
        b (ball)]
    (when (and (not= p -1) (not= b -1))
      (let [near (nearest-tagged "ball" (get-x p) (get-y p) push-range)]
        (when (not= near -1)
          (set-velocity! b push-speed (f32 0.0) (f32 0.0)))))))

;; the AI opponent chases the ball and pushes it toward the -x goal --
;; the honest single-input-stream fallback, same as local-multiplayer/
;; fighting.
(defsystem opponent-ai [dt]
  (let [o (opponent)
        b (ball)]
    (when (and (not= o -1) (not= b -1))
      (move-toward! o b opp-speed)
      (let [near (nearest-tagged "ball" (get-x o) (get-y o) push-range)]
        (when (not= near -1)
          (set-velocity! b (-f (f32 0.0) opp-speed) (f32 0.0) (f32 0.0)))))))

;; score: ball crossing either goal threshold scores for that side and
;; respawns the ball at center.
(defsystem score [dt]
  (let [b (ball)]
    (when (not= b -1)
      (when (<f goal-x (get-x b))
        (set-atom! score-right (+ (atom-val score-right) 1))
        (set-position! b (f32 0.0) (f32 0.0) (f32 0.0))
        (set-velocity! b (f32 0.0) (f32 0.0) (f32 0.0)))
      (when (<f (get-x b) opp-goal-x)
        (set-atom! score-left (+ (atom-val score-left) 1))
        (set-position! b (f32 0.0) (f32 0.0) (f32 0.0))
        (set-velocity! b (f32 0.0) (f32 0.0) (f32 0.0))))))
