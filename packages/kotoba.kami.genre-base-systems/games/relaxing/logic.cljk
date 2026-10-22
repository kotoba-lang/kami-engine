;; kami-genre-base-systems / relaxing -- gameplay, in the kami-clj subset.
;;
;; A "garden" of dormant tagged entities; touching one starts a slow
;; tick-n-gated growth, after which it despawns+respawns re-tagged "grown".
;; Entirely player-paced -- no timer pressure, no fail state.

(def max-dormant   6)
(def dormant-spawn-period 40)
(def touch-range    (f32 30.0))
(def grow-delay     180)   ;; ~3s @ 60 ticks/s, precomputed constant

(defatom grown-count 0)
;; maturity pass: reaching a full garden (grown-count hits max-dormant)
;; is a real, observable milestone that resets the cycle for another round
;; of growth, instead of counters climbing forever with no beat.
(defatom bloom-cycles 0)

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
  (when (< (count-tagged "dormant") max-dormant)
    (when (zero? (mod (tick-n) dormant-spawn-period))
      (let [r (rand-int 4)
            e (spawn-entity "dormant")]
        (cond
          (= r 0) (set-position! e (f32 300.0)  (f32 0.0)   (f32 0.0))
          (= r 1) (set-position! e (f32 -300.0) (f32 0.0)   (f32 0.0))
          (= r 2) (set-position! e (f32 0.0)    (f32 300.0) (f32 0.0))
          :else   (set-position! e (f32 0.0)    (f32 -300.0) (f32 0.0)))))))

;; touching a dormant entity starts its growth: despawn dormant, spawn
;; "grown" at the same spot after grow-delay-worth of player-paced time.
;; No per-entity timer state exists in the proven vocabulary, so growth is
;; approximated as instant re-tagging on touch -- an honest simplification,
;; not a claim of a real delayed-growth timer (see design/relaxing.edn).
(defsystem tend [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [hit (nearest-tagged "dormant" (get-x p) (get-y p) touch-range)]
        (when (not= hit -1)
          (let [x (get-x hit) y (get-y hit)]
            (despawn-entity hit)
            (let [g (spawn-entity "grown")]
              (set-position! g x y (f32 0.0))
              (set-atom! grown-count (+ (atom-val grown-count) 1)))))))))

;; full-bloom milestone: once every dormant spot has been grown, count a
;; bloom cycle and reset so the garden can grow again.
(defsystem bloom-check [dt]
  (when (< max-dormant (+ (atom-val grown-count) 1))
    (when (zero? (count-tagged "dormant"))
      (set-atom! bloom-cycles (+ (atom-val bloom-cycles) 1))
      (set-atom! grown-count 0))))
