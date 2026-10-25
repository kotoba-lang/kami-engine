;; kami-genre-base-systems / fighting -- gameplay, in the kami-clj subset.
;;
;; Same single-local-input-stream constraint as local-multiplayer.edn --
;; true 2-player local fighting is not currently buildable. This is the
;; honest 1-human + 1-AI fallback: p1 is player-controlled, p2-ai closes
;; distance autonomously; proximity-based exchanges on a tick-gated
;; interval decrement a defatom health counter per side.

(def exchange-period 20)
(def exchange-range  (f32 40.0))
(def ai-speed        (f32 150.0))
(def max-health      10)

(defatom health-p1 10)
(defatom health-p2 10)
;; maturity pass: best-of-N round wins tracked toward a match win, instead
;; of rounds resetting forever with no overall outcome.
(def rounds-to-win 2)
(defatom rounds-p1 0)
(defatom rounds-p2 0)

(defn p1 [] (nearest-tagged "p1" (f32 0.0) (f32 0.0) (f32 1000000.0)))
(defn p2 [] (nearest-tagged "p2-ai" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [a (spawn-entity "p1")
        b (spawn-entity "p2-ai")]
    (set-position! a (f32 -80.0) (f32 0.0) (f32 0.0))
    (set-position! b (f32 80.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (p1)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

(defsystem ai [dt]
  (let [a (p1)
        b (p2)]
    (when (and (not= a -1) (not= b -1))
      (move-toward! b a ai-speed))))

;; proximity exchange: if within range on the tick-gated interval, both
;; sides trade damage simultaneously (a simple, honest "exchange" model --
;; no attack-timing/blocking primitives exist in the proven vocabulary).
(defsystem exchange [dt]
  (when (zero? (mod (tick-n) exchange-period))
    (let [a (p1) b (p2)]
      (when (and (not= a -1) (not= b -1))
        (let [near (nearest-tagged "p2-ai" (get-x a) (get-y a) exchange-range)]
          (when (not= near -1)
            (set-atom! health-p1 (- (atom-val health-p1) 1))
            (set-atom! health-p2 (- (atom-val health-p2) 1))))))))

;; reset when either side is depleted -- health restores, and the
;; depleted side's opponent earns a round win toward the match target.
(defsystem round-reset [dt]
  (when (< (atom-val health-p1) 1)
    (set-atom! health-p1 max-health)
    (set-atom! health-p2 max-health)
    (set-atom! rounds-p2 (+ (atom-val rounds-p2) 1)))
  (when (< (atom-val health-p2) 1)
    (set-atom! health-p1 max-health)
    (set-atom! health-p2 max-health)
    (set-atom! rounds-p1 (+ (atom-val rounds-p1) 1))))

;; match win: first to rounds-to-win resets both round counters for a new
;; match.
(defsystem match-check [dt]
  (when (< rounds-to-win (+ (atom-val rounds-p1) 1))
    (set-atom! rounds-p1 0)
    (set-atom! rounds-p2 0))
  (when (< rounds-to-win (+ (atom-val rounds-p2) 1))
    (set-atom! rounds-p1 0)
    (set-atom! rounds-p2 0)))
