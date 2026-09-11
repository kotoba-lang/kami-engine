(ns kami.gameplay.vec3
  "Minimal 3-vector math on plain `[x y z]` vectors.

  Deliberately not a record or a typed buffer: these values cross the EDN
  boundary into scene data, test fixtures and match transcripts, and a plain
  vector reads and diffs there without a print-method."
  (:refer-clojure :exclude [min max]))

(def zero [0.0 0.0 0.0])
(def up [0.0 1.0 0.0])

(defn v3 [x y z] [(double x) (double y) (double z)])
(defn x [v] (double (nth v 0)))
(defn y [v] (double (nth v 1)))
(defn z [v] (double (nth v 2)))

(defn add [a b] [(+ (x a) (x b)) (+ (y a) (y b)) (+ (z a) (z b))])
(defn sub [a b] [(- (x a) (x b)) (- (y a) (y b)) (- (z a) (z b))])
(defn scale [a s] (let [s (double s)] [(* (x a) s) (* (y a) s) (* (z a) s)]))
(defn dot [a b] (+ (* (x a) (x b)) (* (y a) (y b)) (* (z a) (z b))))

(defn cross [a b]
  [(- (* (y a) (z b)) (* (z a) (y b)))
   (- (* (z a) (x b)) (* (x a) (z b)))
   (- (* (x a) (y b)) (* (y a) (x b)))])

(defn length [a] (Math/sqrt (dot a a)))
(defn dist [a b] (length (sub a b)))

(defn dist-xz
  "Ground-plane distance. The storm, most AI ranges and the minimap all care
  about how far apart two things are on the map, not how far apart they are
  after a jump."
  [a b]
  (let [dx (- (x a) (x b)) dz (- (z a) (z b))]
    (Math/sqrt (+ (* dx dx) (* dz dz)))))

(defn normalize
  "Unit vector, or `zero` for a degenerate input. Returning zero rather than
  NaN keeps one bad direction from poisoning every downstream dot product."
  [a]
  (let [l (length a)]
    (if (< l 1e-9) zero (scale a (/ 1.0 l)))))

(defn lerp [a b t]
  (let [t (double t)]
    (add (scale a (- 1.0 t)) (scale b t))))

(defn with-y [a ny] [(x a) (double ny) (z a)])

(defn clamp-length [a maxl]
  (let [l (length a)]
    (if (> l (double maxl)) (scale (normalize a) (double maxl)) a)))
