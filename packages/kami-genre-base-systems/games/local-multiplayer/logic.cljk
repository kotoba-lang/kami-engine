;; kami-genre-base-systems / local-multiplayer -- gameplay, in the kami-clj subset.
;;
;; HARD CONSTRAINT (confirmed this session, isekai-network concept-3): only
;; ONE local input stream exists. True simultaneous local multiplayer input
;; is not buildable today. This base is the honest "1 human + 1 AI" fallback:
;; p1 is player-controlled, p2-ai chases the ball autonomously. Whichever
;; side is nearest the ball when it "scores" (reaches a fixed goal point)
;; gets the point; see design/local-multiplayer.edn.

(def goal-range   (f32 25.0))
(def ai-speed     (f32 180.0))
;; 360 ticks (precomputed 6s @ 60 ticks/s -- `*` is not confirmed proven
;; vocabulary anywhere in this session's reference games, so this constant
;; is written pre-multiplied rather than risk an unverified construct).
(def score-reset-period 360)

(defatom score-p1 0)
(defatom score-p2 0)
;; maturity pass: a real match-point win condition -- first to
;; match-target points wins the match, then both scores reset for a new
;; match instead of counting forever.
(def match-target 5)

(defn p1 [] (nearest-tagged "p1" (f32 0.0) (f32 0.0) (f32 1000000.0)))
(defn p2 [] (nearest-tagged "p2-ai" (f32 0.0) (f32 0.0) (f32 1000000.0)))
(defn ball [] (nearest-tagged "ball" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [a (spawn-entity "p1")
        b (spawn-entity "p2-ai")
        g (spawn-entity "ball")]
    (set-position! a (f32 -200.0) (f32 0.0) (f32 0.0))
    (set-position! b (f32 200.0) (f32 0.0) (f32 0.0))
    (set-position! g (f32 0.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (p1)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; p2-ai always chases the ball -- the honest single-input-stream AI ally.
(defsystem ai [dt]
  (let [p (p2)
        b (ball)]
    (when (and (not= p -1) (not= b -1))
      (move-toward! p b ai-speed))))

;; whichever side's entity is nearest the ball at the scoring tick wins the
;; point, then the ball resets to centre.
(defsystem score [dt]
  (when (zero? (mod (tick-n) score-reset-period))
    (let [b (ball)]
      (when (not= b -1)
        (let [near (nearest-tagged "p1" (get-x b) (get-y b) goal-range)]
          (if (not= near -1)
            (set-atom! score-p1 (+ (atom-val score-p1) 1))
            (let [near2 (nearest-tagged "p2-ai" (get-x b) (get-y b) goal-range)]
              (when (not= near2 -1)
                (set-atom! score-p2 (+ (atom-val score-p2) 1))))))
        (set-position! b (f32 0.0) (f32 0.0) (f32 0.0))))))

;; match-point: first to match-target resets both scores for a new match.
(defsystem match-check [dt]
  (when (< match-target (+ (atom-val score-p1) 1))
    (set-atom! score-p1 0)
    (set-atom! score-p2 0))
  (when (< match-target (+ (atom-val score-p2) 1))
    (set-atom! score-p1 0)
    (set-atom! score-p2 0)))
