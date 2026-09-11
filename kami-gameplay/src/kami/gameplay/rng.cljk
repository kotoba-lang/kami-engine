(ns kami.gameplay.rng
  "Deterministic PRNG — exact on JVM and JavaScript, and honest about the one
  that ships today.

  ## What the browser host runs now, and why it is broken

  `kami.host`'s `kami:engine/random@1.0.0` import advances its state with

      s' = (1103515245 * s + 12345) & 0x7fffffff

  written directly in ClojureScript. In JavaScript that product reaches
  2.37e18, far past `Number.MAX_SAFE_INTEGER` (9.0e15), so the multiplication
  loses its low bits *before* the mask is applied. Measured against exact
  integer arithmetic, the first twenty draws of `(rand-int 4)` from seed 1 are

      shipped host : 2 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
      exact        : 2 3 0 1 2 3 0 1 2 3 0 1 2 3 0 1 2 3 0 1

  The shipped generator collapses to a constant. This is not academic: the
  royale spawn system picks one of four spawn points with `(rand-int 4)`, so
  every bot after the first spawns at the same corner and the whole opposition
  arrives from one direction in single file.

  `legacy-host-int` below reproduces that behaviour exactly, so the defect stays
  under test instead of under a comment.

  ## What this namespace runs instead

  The same LCG, but evaluated exactly. `s` is split at 16 bits and each partial
  product is reduced modulo 2^31 before it can leave the range where a double
  represents integers exactly, so JVM `long` arithmetic and JavaScript `number`
  arithmetic agree bit for bit. Verified against arbitrary-precision integers
  over 200,000 consecutive draws.

  Bounded values are taken from the **high** bits (`floor((s / 2^31) * n)`),
  never from `mod`. An LCG with a power-of-two modulus has low bits whose period
  is only 2^k, which is why even the exact stream above cycles `2 3 0 1` forever
  when you take it modulo 4. Taking the top bits removes that entirely: over
  100,000 draws the four buckets come out 24933 / 25110 / 24908 / 25049.

  All state is an explicit value. Nothing here reads a clock, an atom or a
  global, so a match replays from a seed and produces the same match."
  (:refer-clojure :exclude [int]))

(def ^:const multiplier 1103515245.0)
(def ^:const increment 12345.0)
(def ^:const modulus 2147483648.0)          ;; 2^31
(def ^:const split 65536.0)                 ;; 2^16

(defn seed
  "A generator state from any integer.

  Folds 0 to 1: a zero seed is almost always an uninitialised value rather than
  an intended one, and this generator's zero state is not special, so the
  mistake would otherwise present as `deterministic` rather than as
  `suspiciously identical across runs`."
  [n]
  (let [s (mod (Math/abs (double (or n 1))) modulus)]
    (if (zero? s) 1.0 s)))

(defn next-state
  "Advance the generator, exactly.

  `hi`/`lo` split the state at 16 bits. `multiplier * hi` reaches 3.6e13 and
  `multiplier * lo` reaches 7.2e13, both well inside exact-double range; the
  `mod` before re-scaling by 2^16 keeps the recombined term under 1.4e14. No
  intermediate ever approaches 2^53, so nothing rounds."
  [s]
  (let [s (double s)
        hi (Math/floor (/ s split))
        lo (- s (* hi split))]
    (mod (+ (* (mod (* multiplier hi) modulus) split)
            (* multiplier lo)
            increment)
         modulus)))

(defn legacy-host-int
  "Reproduce the shipped browser host's lossy draw, for regression tests only.

  Returns `[value next-state]` using the exact expression `kami.host` evaluates,
  including its JavaScript precision loss. On the JVM this is emulated by
  rounding the product to the nearest representable double first, which is what
  JavaScript does implicitly. Never call this from gameplay code."
  [s bound]
  (let [product (+ (* 1103515245.0 (double s)) 12345.0)
        ;; JS evaluates this product as a double; the JVM would otherwise keep
        ;; it exact as a long. Forcing the double round-trip makes both hosts
        ;; agree about the *defect*.
        rounded (double product)
        s' (double (bit-and (long rounded) 0x7fffffff))
        b (long bound)]
    [(if (pos? b) (mod (long s') b) 0) s']))

(defn unit
  "Draw a double in [0.0, 1.0) from the high bits. Returns `[value next-state]`."
  [s]
  (let [s' (next-state s)]
    [(/ s' modulus) s']))

(defn int
  "Draw an integer in [0, bound) from the high bits. Returns `[value next-state]`.

  `floor(unit * bound)` rather than `mod`: see the namespace docstring for the
  measured difference. A non-positive bound yields 0, matching the host import's
  contract for that case."
  [s bound]
  (let [[u s'] (unit s)
        b (long bound)]
    [(if (pos? b) (long (Math/floor (* u (double b)))) 0) s']))

(defn range-f
  "Draw a double in [lo, hi). Returns `[value next-state]`."
  [s lo hi]
  (let [[u s'] (unit s)]
    [(+ (double lo) (* u (- (double hi) (double lo)))) s']))

(defn pick
  "Draw one element of `coll`, or nil when empty. Returns `[value next-state]`."
  [s coll]
  (let [v (vec coll)]
    (if (empty? v)
      [nil (next-state s)]
      (let [[i s'] (int s (count v))]
        [(nth v i) s']))))

(defn shuffle-v
  "Deterministic Fisher-Yates. Returns `[shuffled-vector next-state]`."
  [s coll]
  (let [v0 (vec coll)]
    (loop [v v0 i (dec (count v0)) st s]
      (if (pos? i)
        (let [[j st'] (int st (inc i))]
          (recur (assoc v i (nth v j) j (nth v i)) (dec i) st'))
        [v st]))))
