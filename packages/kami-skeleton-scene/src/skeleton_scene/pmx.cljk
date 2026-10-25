(ns skeleton-scene.pmx
  "MMD `.pmx` (Polygon Model eXtended) model import — **mesh v1** plus
  a best-effort v2 rig/morph/material read.

  Parses the header + vertex + face sections into a renderable
  `pmx-model` (positions / normals / UVs + per-vertex bone indices &
  weights, ready for GPU skinning the same way a VRM mesh is).
  Textures / materials / bones / morphs live after the face block and
  are read best-effort (a truncated / mesh-only PMX still returns the
  mesh with an empty rig) — the geometry is the MMD counterpart of the
  VRM mesh load.

  Layout (PMX 2.0/2.1): `\"PMX \" + f32 version + globals[u8 count +
  bytes]`, then 4 text fields (model name x2, comment x2), then `i32
  vertex count` + vertices, then `i32 face count` + faces (3 indices
  each). Globals carry the text encoding (0 = UTF-16LE, 1 = UTF-8) and
  the per-section index byte sizes.

  Restored from the legacy kami-engine/kami-skeleton-scene Rust
  crate's `src/pmx.rs` (deleted in kotoba-lang/kami-engine PR #82
  'Remove Rust workspace from kami-engine') as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root). The original
  mutated a byte cursor (`struct Cur`) and an `&mut PmxModel` in
  place; this port uses an atom-backed cursor (mirroring the Rust
  cursor's incremental position commits 1:1 — a field only advances
  once its bytes are fully read) and an atom-backed accumulator for
  the v2 sections so a truncated section still returns whatever
  sections parsed before it, exactly like the original's `&mut model`.

  A `bytes` argument is a plain vector (or any indexed, `count`-able
  sequence) of unsigned byte values (0-255) — the CLJC-portable
  equivalent of Rust's `&[u8]`.

  Zero-dep portable CLJC."
  (:require [skeleton :as skeleton]
            [skeleton.math :as m]))

;; ── low-level byte cursor ────────────────────────────────────────

(defn- ->u32 [bs]
  (reduce (fn [acc [i x]] (bit-or acc (bit-shift-left (long (bit-and 0xff x)) (* 8 i))))
          0 (map-indexed vector bs)))

(defn- ->i32 [bs] (let [u (->u32 bs)] (if (>= u 0x80000000) (- u 0x100000000) u)))
(defn- ->i16 [bs] (let [u (->u32 bs)] (if (>= u 0x8000) (- u 0x10000) u)))

(defn- ->f32 [bs]
  #?(:clj (double (Float/intBitsToFloat (unchecked-int (->u32 bs))))
     :cljs (let [buf (js/ArrayBuffer. 4) dv (js/DataView. buf)]
             (dotimes [i 4] (.setUint8 dv i (nth bs i)))
             (.getFloat32 dv 0 true))))

(defn- ->cur [bytes opts]
  (merge {:b (vec bytes) :p (atom 0)} opts))

(defn- take!
  "Read `n` bytes at the cursor and advance; nil (cursor untouched) if
  out of bounds."
  [cur n]
  (let [b (:b cur) p @(:p cur)]
    (when (<= (+ p n) (count b))
      (reset! (:p cur) (+ p n))
      (subvec b p (+ p n)))))

(defn- u8! [cur] (some-> (take! cur 1) first))
(defn- u16! [cur] (some->> (take! cur 2) ->u32))
(defn- i32! [cur] (some->> (take! cur 4) ->i32))
(defn- f32! [cur] (some->> (take! cur 4) ->f32))

(defn- chain!
  "Run 0-arg thunks `ts` in order (mirrors Rust's `?` chaining across
  several cursor reads). Stops at the first nil result (returning nil
  overall); otherwise returns a vector of every result, in order."
  [& ts]
  (loop [ts ts acc []]
    (if (empty? ts)
      acc
      (let [r ((first ts))]
        (when (some? r) (recur (rest ts) (conj acc r)))))))

(defn- read-floats! [cur n]
  (loop [i 0 acc []]
    (if (= i n)
      acc
      (when-let [f (f32! cur)] (recur (inc i) (conj acc f))))))

(defn- vec3f! [cur] (read-floats! cur 3))

;; A signed N-byte index (bone refs; -1 = none).
(defn- sidx! [cur n]
  (case (int n)
    1 (when-let [b (u8! cur)] (if (>= b 128) (- b 256) b))
    2 (some->> (take! cur 2) ->i16)
    (i32! cur)))

;; An unsigned N-byte index (face vertex refs).
(defn- uidx! [cur n]
  (case (int n)
    1 (u8! cur)
    2 (u16! cur)
    (some->> (take! cur 4) ->u32)))

(defn- decode-text [bs enc-utf16?]
  (if enc-utf16?
    #?(:clj (String. (byte-array (map unchecked-byte bs)) "UTF-16LE")
       :cljs (.decode (js/TextDecoder. "utf-16le") (js/Uint8Array. (clj->js (vec bs)))))
    #?(:clj (String. (byte-array (map unchecked-byte bs)) "UTF-8")
       :cljs (.decode (js/TextDecoder. "utf-8") (js/Uint8Array. (clj->js (vec bs)))))))

(defn- text! [cur]
  (when-let [len (i32! cur)]
    (when (>= len 0)
      (when-let [bs (take! cur len)]
        (decode-text bs (:enc-utf16 cur))))))

;; ── mesh (v1) ────────────────────────────────────────────────────

(defn- read-bone-weights! [cur wtype]
  (case (int wtype)
    0 (when-let [b0 (sidx! cur (:bidx cur))]
        [[b0 -1 -1 -1] [1.0 0.0 0.0 0.0]])
    1 (when-let [b0 (sidx! cur (:bidx cur))]
        (when-let [b1 (sidx! cur (:bidx cur))]
          (when-let [w0 (f32! cur)]
            [[b0 b1 -1 -1] [w0 (- 1.0 w0) 0.0 0.0]])))
    (2 4) (when-let [bones (chain! #(sidx! cur (:bidx cur)) #(sidx! cur (:bidx cur))
                                    #(sidx! cur (:bidx cur)) #(sidx! cur (:bidx cur)))]
            (when-let [weights (chain! #(f32! cur) #(f32! cur) #(f32! cur) #(f32! cur))]
              [bones weights]))
    3 (when-let [b0 (sidx! cur (:bidx cur))]
        (when-let [b1 (sidx! cur (:bidx cur))]
          (when-let [w0 (f32! cur)]
            (when (take! cur 36)
              [[b0 b1 -1 -1] [w0 (- 1.0 w0) 0.0 0.0]]))))
    nil))

(defn- read-vertex! [cur]
  (when-let [pos (vec3f! cur)]
    (when-let [normal (vec3f! cur)]
      (when-let [uv (chain! #(f32! cur) #(f32! cur))]
        (when (loop [i 0]
                (if (= i (:add-uv cur)) true (when (take! cur 16) (recur (inc i)))))
          (when-let [wtype (u8! cur)]
            (when-let [[bones weights] (read-bone-weights! cur wtype)]
              (when (f32! cur) ; edge scale, discarded
                {:pos pos :normal normal :uv uv :bones bones :weights weights}))))))))

;; ── v2 sections (best-effort): textures -> materials -> bones -> morphs

(defn- read-material! [cur]
  (when-let [name (text! cur)]
    (when (text! cur) ; name universal
      (when-let [diffuse (chain! #(f32! cur) #(f32! cur) #(f32! cur) #(f32! cur))]
        (when (take! cur (+ 12 4 12)) ; specular(3) + spec-strength + ambient(3)
          (when (u8! cur) ; draw flags
            (when (take! cur (+ 16 4)) ; edge colour(4) + edge size
              (when-let [texture (sidx! cur (:tidx cur))]
                (when (sidx! cur (:tidx cur)) ; sphere texture index
                  (when (u8! cur) ; sphere mode
                    (when-let [toon-flag (u8! cur)]
                      (when (if (zero? toon-flag) (sidx! cur (:tidx cur)) (u8! cur))
                        (when (text! cur) ; memo
                          (when-let [surface-count (i32! cur)]
                            {:name name :diffuse diffuse :texture texture :surface-count surface-count}))))))))))))))

(defn- read-bone! [cur]
  (when-let [name (text! cur)]
    (when (text! cur) ; universal
      (when-let [pos (vec3f! cur)]
        (when-let [parent (sidx! cur (:bidx cur))]
          (when (i32! cur) ; transform layer
            (when-let [flags (u16! cur)]
              (when (and
                     (if (zero? (bit-and flags 0x0001)) (take! cur 12) (sidx! cur (:bidx cur)))
                     (if (zero? (bit-and flags 0x0300)) true (and (sidx! cur (:bidx cur)) (f32! cur)))
                     (if (zero? (bit-and flags 0x0400)) true (take! cur 12))
                     (if (zero? (bit-and flags 0x0800)) true (take! cur 24))
                     (if (zero? (bit-and flags 0x2000)) true (i32! cur))
                     (if (zero? (bit-and flags 0x0020))
                       true
                       (and (sidx! cur (:bidx cur))
                            (i32! cur)
                            (f32! cur)
                            (when-let [links (i32! cur)]
                              (loop [i 0]
                                (if (= i links)
                                  true
                                  (when (sidx! cur (:bidx cur))
                                    (when-let [ty (u8! cur)]
                                      (when (if (= ty 1) (take! cur 24) true)
                                        (recur (inc i)))))))))))
                {:name name :pos pos :parent parent}))))))))

(defn- read-morph!
  "Read one morph record. Returns `{:morph m}` where `m` is a
  `pmx-morph` for a vertex morph (type 1) or nil for any other morph
  type (bytes still consumed, matching the original's skip-and-keep-
  going v2 behaviour); nil (not a map) on a truncated read."
  [cur]
  (when-let [name (text! cur)]
    (when (text! cur) ; universal
      (when (u8! cur) ; panel
        (when-let [ty (u8! cur)]
          (when-let [n (i32! cur)]
            (if (= ty 1)
              (let [offsets (loop [i 0 acc []]
                              (if (= i n)
                                acc
                                (when-let [vi (uidx! cur (:vidx cur))]
                                  (when-let [off (vec3f! cur)]
                                    (recur (inc i) (conj acc [vi off]))))))]
                (when offsets
                  {:morph {:name name :offsets offsets}}))
              (let [each (case (int ty)
                           (0 9) (+ (:moidx cur) 4)
                           2 (+ (:bidx cur) 28)
                           (3 4 5 6 7) (+ (:vidx cur) 16)
                           8 (+ (:midx cur) 1 (* 28 4))
                           10 (+ (:rbidx cur) 1 24)
                           0)]
                (when (take! cur (* each n))
                  {:morph nil})))))))))

(defn- parse-rest!
  "Parse the post-face sections (textures / materials / bones /
  morphs) into `model`, best-effort — returns `model` with as many
  sections filled in as parsed successfully before any truncation."
  [cur model]
  (let [result (atom model)]
    (when-let [tex-n (i32! cur)]
      (let [textures (loop [i 0 acc []]
                        (if (= i tex-n) acc (when-let [t (text! cur)] (recur (inc i) (conj acc t)))))]
        (when textures
          (swap! result assoc :textures textures)
          (when-let [mat-n (i32! cur)]
            (let [materials (loop [i 0 acc []]
                               (if (= i mat-n) acc
                                   (when-let [mt (read-material! cur)] (recur (inc i) (conj acc mt)))))]
              (when materials
                (swap! result assoc :materials materials)
                (when-let [bone-n (i32! cur)]
                  (let [bones (loop [i 0 acc []]
                                (if (= i bone-n) acc
                                    (when-let [bn (read-bone! cur)] (recur (inc i) (conj acc bn)))))]
                    (when bones
                      (swap! result assoc :bones bones)
                      (when-let [morph-n (i32! cur)]
                        (let [morphs (loop [i 0 acc []]
                                       (if (= i morph-n) acc
                                           (when-let [r (read-morph! cur)]
                                             (recur (inc i) (if (:morph r) (conj acc (:morph r)) acc)))))]
                          (when morphs
                            (swap! result assoc :morphs morphs)))))))))))))
    @result))

(defn pmx-to-model
  "Parse the PMX header + vertex + face sections (plus best-effort v2
  sections) into a `pmx-model`:
  `{:name :vertices :indices :bones :morphs :materials :textures}`."
  [bytes]
  (let [b (vec bytes)]
    (when (and (>= (count b) 9) (= (subvec b 0 4) [0x50 0x4D 0x58 0x20]))
      (let [gcount (nth b 8)]
        (when (<= (+ 9 gcount) (count b))
          (let [globals (subvec b 9 (+ 9 gcount))
                enc-utf16 (zero? (nth globals 0))
                add-uv (nth globals 1 0)
                g (fn [i] (nth globals i 4))
                cur (->cur b {:enc-utf16 enc-utf16 :vidx (g 2) :tidx (g 3) :midx (g 5)
                              :bidx (g 4) :moidx (g 6) :rbidx (g 7) :add-uv add-uv})]
            (reset! (:p cur) (+ 9 gcount))
            (when-let [name (text! cur)]
              (when (chain! #(text! cur) #(text! cur) #(text! cur))
                (when-let [vcount (i32! cur)]
                  (let [vertices (loop [i 0 acc []]
                                   (if (= i vcount) acc
                                       (when-let [v (read-vertex! cur)] (recur (inc i) (conj acc v)))))]
                    (when vertices
                      (when-let [icount (i32! cur)]
                        (let [indices (loop [i 0 acc []]
                                        (if (= i icount) acc
                                            (when-let [ix (uidx! cur (:vidx cur))] (recur (inc i) (conj acc ix)))))]
                          (when indices
                            (parse-rest! cur {:name name :vertices vertices :indices indices
                                               :bones [] :morphs [] :materials [] :textures []})))))))))))))))

;; ── PMX -> skeleton ──────────────────────────────────────────────

(defn- mat4-from-translation [[x y z]]
  [1.0 0.0 0.0 0.0
   0.0 1.0 0.0 0.0
   0.0 0.0 1.0 0.0
   x   y   z   1.0])

(defn pmx-to-skeleton
  "Realise a PMX model's bones into a `skeleton/skeleton` — the rig a
  `.vmd` motion (via `skeleton-scene.vmd/vmd-to-clip`) plays on. PMX
  bone positions are world-space at rest (identity rotation); local
  position = world - parent world, and the inverse-bind is the
  inverse of the rest world translation."
  [model]
  (let [bones-vec (:bones model)]
    (skeleton/skeleton
     (mapv (fn [b]
             (let [parent (when (>= (:parent b) 0) (:parent b))
                   p-bone (when parent (get bones-vec parent))
                   local (if p-bone (m/vec3- (:pos b) (:pos p-bone)) (:pos b))]
               (skeleton/bone
                {:name (:name b)
                 :parent parent
                 :local-position local
                 :local-rotation m/quat-identity
                 :local-scale m/vec3-one
                 :inverse-bind (mat4-from-translation (m/vec3-scale (:pos b) -1.0))})))
           bones-vec))))
