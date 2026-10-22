;; kami-genre-base-systems / open-world -- gameplay, in the kami-clj subset.
;;
;; "Open world" here = a large flat arena with scattered static points of
;; interest, NOT streaming terrain/biomes -- that fidelity lives only in
;; Rust host code (kami-terrain/kami-vegetation/kami-atmosphere), unreachable
;; from kami-clj logic (see design/open-world.edn's known-constraints).

(def max-poi       40)
(def spawn-period  30)
(def discover-range (f32 30.0))
(def spawn-half     (f32 900.0))

(defatom discovered 0)
;; maturity pass: a rarer "landmark" POI type worth more, on a slower
;; cadence -- a real discovery-tier distinction beyond one uniform POI.
(def landmark-period 150)
(defatom landmarks-found 0)

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; scatter points of interest across the whole arena (not a ring, unlike
;; the combat-focused bases -- open-world is about wide free roaming).
(defsystem spawn [dt]
  (when (< (count-tagged "poi") max-poi)
    (when (zero? (mod (tick-n) spawn-period))
      (let [e (spawn-entity "poi")
            rx (- (rand-int 2) 1)
            ry (- (rand-int 2) 1)]
        (cond
          (and (zero? rx) (zero? ry)) (set-position! e spawn-half spawn-half (f32 0.0))
          (zero? rx)                  (set-position! e spawn-half (f32 0.0) (f32 0.0))
          (zero? ry)                  (set-position! e (f32 0.0) spawn-half (f32 0.0))
          :else                       (set-position! e spawn-half spawn-half (f32 0.0)))))))

;; discovery: reaching a point of interest collects it, no combat, no fail.
(defsystem discover [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [hit (nearest-tagged "poi" (get-x p) (get-y p) discover-range)]
        (when (not= hit -1)
          (despawn-entity hit)
          (set-atom! discovered (+ (atom-val discovered) 1)))))))

;; rarer landmark POI, on its own slower cadence, worth counting separately.
(defsystem landmark-spawn [dt]
  (when (zero? (mod (tick-n) landmark-period))
    (let [l (spawn-entity "landmark")]
      (set-position! l (f32 -700.0) (f32 700.0) (f32 0.0)))))

(defsystem landmark-discover [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [hit (nearest-tagged "landmark" (get-x p) (get-y p) discover-range)]
        (when (not= hit -1)
          (despawn-entity hit)
          (set-atom! landmarks-found (+ (atom-val landmarks-found) 1)))))))
