(ns skeleton-scene.vmd
  "MMD `.vmd` (Vocaloid Motion Data) motion import -> `skeleton`'s
  `animation-clip`.

  The binary counterpart of `skeleton-scene.clip/clip-from-edn`: load a
  MikuMikuDance bone animation and retarget it onto any skeleton via a
  caller-supplied bone map. MMD bone names are **Shift-JIS**;
  [[mmd-bone-to-humanoid]] maps the standard ones (センター/頭/左腕/…)
  to VRM humanoid names so one `.vmd` drives a VRM rig.

  Restored from the legacy kami-engine/kami-skeleton-scene Rust
  crate's `src/vmd.rs` (deleted in kotoba-lang/kami-engine PR #82
  'Remove Rust workspace from kami-engine') as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root). The original used
  `encoding_rs` for Shift-JIS decode; this port uses platform charset
  support (`java.lang.String`/`java.nio.charset` on Clojure,
  `TextDecoder` on ClojureScript — both natively support the
  `shift_jis` legacy encoding label).

  `.vmd` v2 layout: 30-byte signature + 20-byte model name, then a
  `u32` bone keyframe count, then that many **111-byte** records:
  `[15: name (SJIS)] [4: frame u32] [12: pos f32x3] [16: quat f32x4]
  [64: interp]`. Morph / camera / light keyframes after the bone block
  are ignored (motion only).

  A `bytes` argument throughout this namespace is a plain vector (or
  any indexed, `count`-able sequence) of unsigned byte values (0-255)
  — the CLJC-portable equivalent of Rust's `&[u8]`, working under both
  Clojure and ClojureScript with no platform-specific byte-array type
  in the public API.

  Zero-dep portable CLJC."
  (:require [skeleton :as skeleton]))

(defn- ->u32
  "Little-endian byte sequence `bs` -> unsigned integer."
  [bs]
  (reduce (fn [acc [i x]] (bit-or acc (bit-shift-left (long (bit-and 0xff x)) (* 8 i))))
          0 (map-indexed vector bs)))

(defn- ->f32
  "Little-endian 4-byte sequence `bs` -> IEEE-754 single-precision float
  (read as a double, matching Clojure's default numeric tower)."
  [bs]
  #?(:clj (double (Float/intBitsToFloat (unchecked-int (->u32 bs))))
     :cljs (let [buf (js/ArrayBuffer. 4) dv (js/DataView. buf)]
             (dotimes [i 4] (.setUint8 dv i (nth bs i)))
             (.getFloat32 dv 0 true))))

(defn- u32-le [b o] (when (<= (+ o 4) (count b)) (->u32 (subvec b o (+ o 4)))))
(defn- f32-le [b o] (when (<= (+ o 4) (count b)) (->f32 (subvec b o (+ o 4)))))

(defn- decode-sjis
  "Decode a null-padded Shift-JIS bone-name byte field."
  [bs]
  (let [end (or (first (keep-indexed (fn [i x] (when (zero? x) i)) bs)) (count bs))
        trimmed (vec (take end bs))]
    #?(:clj (String. (byte-array (map unchecked-byte trimmed)) "Shift_JIS")
       :cljs (.decode (js/TextDecoder. "shift_jis") (js/Uint8Array. (clj->js trimmed))))))

(defn mmd-bone-to-humanoid
  "Map a standard MMD bone name (Shift-JIS-decoded) to a VRM humanoid
  bone name. Covers the common locomotion/upper-body bones; unknown ->
  nil."
  [name]
  (case name
    ("センター" "center") "hips"
    "上半身" "spine"
    "上半身2" "chest"
    "首" "neck"
    "頭" "head"
    "左腕" "leftUpperArm"
    "右腕" "rightUpperArm"
    "左ひじ" "leftLowerArm"
    "右ひじ" "rightLowerArm"
    "左足" "leftUpperLeg"
    "右足" "rightUpperLeg"
    "左ひざ" "leftLowerLeg"
    "右ひざ" "rightLowerLeg"
    nil))

(defn vmd-to-clip
  "Parse a `.vmd` motion into an `animation-clip`. `fps` is the
  playback rate (MMD authors at 30); `bone-index` resolves a bone name
  -> skeleton index — it is tried on the raw name first, then on its
  [[mmd-bone-to-humanoid]] equivalent, so a caller can map either MMD
  names or VRM humanoid names. Returns nil if the data is too short or
  no track resolves."
  [bytes fps bone-index]
  (let [b (vec bytes)
        fps (if (> fps 0.0) fps 30.0)]
    (when-let [kf-count (u32-le b 50)]
      (loop [i 0 off 54 by-bone (sorted-map) max-frame 0]
        (if (or (= i kf-count) (> (+ off 111) (count b)))
          (when (seq by-bone)
            (let [tracks (mapv (fn [[bone-idx keys]]
                                  (skeleton/bone-track bone-idx (vec (sort-by :time keys)) :linear))
                                by-bone)]
              (skeleton/animation-clip
               {:name "vmd" :duration (/ max-frame fps) :tracks tracks :looping false})))
          (let [name (decode-sjis (subvec b off (+ off 15)))
                frame (u32-le b (+ off 15))
                pos [(f32-le b (+ off 19)) (f32-le b (+ off 23)) (f32-le b (+ off 27))]
                rot [(f32-le b (+ off 31)) (f32-le b (+ off 35)) (f32-le b (+ off 39)) (f32-le b (+ off 43))]
                idx (or (bone-index name) (some-> (mmd-bone-to-humanoid name) bone-index))]
            (if idx
              (recur (inc i) (+ off 111)
                     (update by-bone idx (fnil conj [])
                             (skeleton/keyframe {:time (/ frame fps) :position pos :rotation rot :scale nil}))
                     (max max-frame frame))
              (recur (inc i) (+ off 111) by-bone max-frame))))))))
