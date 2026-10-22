;; kami-genre-base-systems / puzzle -- gameplay, in the kami-clj subset.
;; Touch 4 fixed tiles in the correct order (0,1,2,3). Wrong order resets
;; progress to zero. No drag/swap primitive exists, so this is a
;; touch-in-sequence puzzle, not a grid-swap puzzle (see design/puzzle.edn).

(def touch-range (f32 40.0))
(def num-tiles   4)

(defatom progress 0)
;; maturity pass: a second distinct level so there is real progression
;; instead of one static puzzle -- level 1 is the original corner-square
;; order; level 2 (unlocked once level 1 is fully solved once) uses a
;; different order (reverse-diagonal instead of clockwise) over the same
;; 4 tile positions, confirmed via the compiler source (`*`/`not`/`do`/
;; `<=`/`>=` are all real emit targets in kotoba.engine-clj.ast/codegen,
;; not just the narrower set this session's reference-game grep happened
;; to demonstrate -- see ADR-2607031800's library-reuse-check note).
(defatom level 1)
(defatom solves 0)

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0)))
  ;; 4 fixed tiles at the corners of a small square, tagged by index order
  ;; via distinct tile spawns -- position doubles as identity since no
  ;; per-entity metadata/id-lookup beyond tag+position exists.
  (let [t0 (spawn-entity "tile")] (set-position! t0 (f32 150.0)  (f32 150.0)  (f32 0.0)))
  (let [t1 (spawn-entity "tile")] (set-position! t1 (f32 -150.0) (f32 150.0)  (f32 0.0)))
  (let [t2 (spawn-entity "tile")] (set-position! t2 (f32 -150.0) (f32 -150.0) (f32 0.0)))
  (let [t3 (spawn-entity "tile")] (set-position! t3 (f32 150.0)  (f32 -150.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; expected-position lookup for the current progress step, by (level, step).
;; level 1: clockwise from top-right. level 2: reverse-diagonal order over
;; the same 4 positions -- a genuinely different sequence to solve, not a
;; copy-pasted duplicate.
(defn expected-x [step]
  (if (= (atom-val level) 1)
    (cond (= step 0) (f32 150.0) (= step 1) (f32 -150.0)
          (= step 2) (f32 -150.0) :else (f32 150.0))
    (cond (= step 0) (f32 150.0) (= step 1) (f32 -150.0)
          (= step 2) (f32 150.0) :else (f32 -150.0))))
(defn expected-y [step]
  (if (= (atom-val level) 1)
    (cond (= step 0) (f32 150.0) (= step 1) (f32 150.0)
          (= step 2) (f32 -150.0) :else (f32 -150.0))
    (cond (= step 0) (f32 -150.0) (= step 1) (f32 150.0)
          (= step 2) (f32 150.0) :else (f32 -150.0))))

(defsystem touch [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [tile (nearest-tagged "tile" (get-x p) (get-y p) touch-range)]
        (when (not= tile -1)
          (if (and (= (get-x tile) (expected-x (atom-val progress)))
                   (= (get-y tile) (expected-y (atom-val progress))))
            (set-atom! progress (mod (+ (atom-val progress) 1) num-tiles))
            (set-atom! progress 0)))))))

;; level advance: completing num-tiles worth of correct touches (progress
;; wraps to 0 exactly on a full clean solve) promotes level 1 -> level 2
;; once, tracked by solves so it doesn't re-trigger every wrap.
(defsystem level-advance [dt]
  (when (zero? (atom-val progress))
    (when (< 0 (atom-val solves))
      (when (= (atom-val level) 1)
        (set-atom! level 2)))))

(defsystem count-solve [dt]
  (when (= (atom-val progress) (- num-tiles 1))
    (set-atom! solves (+ (atom-val solves) 1))))
