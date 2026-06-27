(ns kami.ipc
  "L1 — render-IR → KAMI IPC columnar buffer (zero-copy transport to the GPU).

  Reuses the engine's existing columnar format (`../kami-core/src/ipc.rs`:
  Column / Dtype / KamiFrame). Dtype already has Mat4 and Quat, so an instance
  `model` array is ONE Dtype/Mat4 column that DMAs straight into a wgpu instance
  buffer — 'all transitions zero-copy or single memcpy' per the format's contract
  (ARCHITECTURE.md §9).

  Buffer layout produced by `pack` (little-endian, matches the Rust reader):

    [KamiFrame header]                16 bytes  : magic 'KAMI' u32 | version u16 |
                                                  ncols u16 | frame_n u32 | pad u32
    [Column header] × ncols           16 bytes  : dtype u8 | stride u8 | pad u16 |
                                                  len u32 | offset u32 | pad u32
    [payload, 16-byte aligned] × ncols          : raw element bytes

  All offsets are relative to the start of the buffer and 16-byte aligned so a
  host can wrap each column as a GPU-aligned slice with no realignment copy.")

;; ---------------------------------------------------------------------------
;; Dtype table — mirrors kami-core/src/ipc.rs::Dtype
;; ---------------------------------------------------------------------------

(def dtype
  "KAMI Dtype tag → {:enum byte :elsize bytes}. Mat4 = 16×f32 = 64B;
  Quat = smallest-3 4×f16 = 8B."
  {:f32  {:enum 0 :elsize 4}
   :f16  {:enum 1 :elsize 2}
   :u32  {:enum 2 :elsize 4}
   :u16  {:enum 3 :elsize 2}
   :u8   {:enum 4 :elsize 1}
   :i16  {:enum 5 :elsize 2}
   :mat4 {:enum 6 :elsize 64}
   :quat {:enum 7 :elsize 8}})

(def ^:const magic
  "ASCII 'KAMI' as a little-endian u32 (K=0x4B A=0x41 M=0x4D I=0x49)."
  0x494D414B)

(def ^:const version 1)
(def ^:const header-bytes 16)
(def ^:const column-header-bytes 16)

(defn- align16 ^long [^long n]
  (bit-and (+ n 15) (bit-not 15)))

(defn byte-len
  "Payload bytes for a column of `n` items of `dt` with `stride` elements/item
  (NOT including the 16-byte column header). Use `align16` when laying out."
  [dt n stride]
  (let [{:keys [elsize]} (dtype dt)]
    (when-not elsize (throw (ex-info "unknown dtype" {:dtype dt})))
    (* (long elsize) (long n) (long stride))))

(defn column
  "Build one column descriptor.
  `data` is a seq/vector of raw element numbers already flattened
  (e.g. 320 mat4 → 320×16 = 5120 f32s). `stride` is elements-per-item."
  [dt stride data]
  (when-not (dtype dt) (throw (ex-info "unknown dtype" {:dtype dt})))
  (let [v (vec data)
        per (case dt :mat4 16 :quat 4 1)            ; sub-elements per item slot
        items (long (/ (count v) (* per stride)))]
    {:dtype dt :stride stride :len items :data v
     ;; element count actually written (flattened)
     :flat-count (count v)}))

;; ---------------------------------------------------------------------------
;; Byte writers (platform-neutral; produce a vector of unsigned bytes 0-255)
;; ---------------------------------------------------------------------------

(defn- u8s-of-u32 [^long x]
  [(bit-and x 0xff) (bit-and (bit-shift-right x 8) 0xff)
   (bit-and (bit-shift-right x 16) 0xff) (bit-and (bit-shift-right x 24) 0xff)])

(defn- u8s-of-u16 [^long x]
  [(bit-and x 0xff) (bit-and (bit-shift-right x 8) 0xff)])

(defn- f32-bits ^long [x]
  #?(:clj  (long (bit-and (Float/floatToRawIntBits (float x)) 0xffffffff))
     :cljs (let [b (js/ArrayBuffer. 4)]
             (aset (js/Float32Array. b) 0 x)
             (aget (js/Uint32Array. b) 0))))

(defn- f16-bits
  "f32 number → IEEE-754 binary16 as a 16-bit value (0-0xffff), round-to-nearest-even.
  Port of the `half` crate's `f32_to_f16` fallback. The KAMI host transports these
  raw (no Rust-side decode — wgpu reads them as `Float16`/`Float16x4`), so the only
  contract is 'valid little-endian binary16'. Drives both :f16 and :quat columns."
  ^long [x]
  (let [b         (f32-bits x)                                  ; unsigned 32-bit float bits
        half-sign (bit-and (unsigned-bit-shift-right b 16) 0x8000)
        exp       (bit-and b 0x7F800000)
        man       (bit-and b 0x007FFFFF)]
    (if (= exp 0x7F800000)
      ;; Inf (man=0) / NaN (man≠0, force a quiet-NaN bit so it survives the truncation)
      (bit-or half-sign 0x7C00
              (if (zero? man) 0 0x0200)
              (unsigned-bit-shift-right man 13))
      (let [half-exp (+ (- (unsigned-bit-shift-right exp 23) 127) 15)] ; unbias f32, rebias f16
        (cond
          ;; exponent overflow → ±Inf
          (>= half-exp 0x1F)
          (bit-or half-sign 0x7C00)

          ;; subnormal / underflow (half-exp <= 0)
          (<= half-exp 0)
          (if (> (- 14 half-exp) 24)
            half-sign                                            ; full underflow → signed zero
            (let [man*  (bit-or man 0x00800000)                 ; restore hidden leading 1
                  hm    (unsigned-bit-shift-right man* (- 14 half-exp))
                  round (bit-shift-left 1 (- 13 half-exp))]
              (if (and (not (zero? (bit-and man* round)))
                       (not (zero? (bit-and man* (dec (* 3 round))))))
                (bit-or half-sign (inc hm))
                (bit-or half-sign hm))))

          ;; normal
          :else
          (let [he    (bit-shift-left half-exp 10)
                hm    (unsigned-bit-shift-right man 13)
                round 0x1000]
            (if (and (not (zero? (bit-and man round)))
                     (not (zero? (bit-and man (dec (* 3 round))))))
              (inc (bit-or half-sign he hm))                    ; round up may carry into exp
              (bit-or half-sign he hm))))))))

(defn- u8s-of-element [dt x]
  (case dt
    (:f32 :mat4) (u8s-of-u32 (f32-bits x))   ; mat4 payload is a stream of f32
    :u32         (u8s-of-u32 (long x))
    (:u16 :i16)  (u8s-of-u16 (long x))
    :u8          [(bit-and (long x) 0xff)]
    ;; f16 column = one half per element; quat = 4×f16 (already flattened to 4 comps
    ;; per item by `column`), so per-element emit is identical — both → 1 half / 2 LE bytes.
    (:f16 :quat) (u8s-of-u16 (f16-bits x))))

(defn- pad-to [bytes ^long target]
  (into bytes (repeat (- target (count bytes)) 0)))

;; ---------------------------------------------------------------------------
;; pack — render-IR frame → KamiFrame columnar byte vector
;; ---------------------------------------------------------------------------

(defn frame->columns
  "Flatten a render-IR frame (`kami.render/frame`) into ordered columns:
  one Mat4 column for the camera (view++proj packed as 2 mat4), then per draw a
  Mat4 instance-model column.

  When `tint?` is true (the version-2 layout), each draw additionally gets an f16
  RGBA tint column right after its model column, so the per-draw column block is
  `[model-mat4, tint-f16×4]`. Default (`tint?` false) is the version-1 layout —
  one model column per draw — byte-identical to what the Rust decoder fixture
  (`kami-clj-host`) currently pins."
  ([frame] (frame->columns frame false))
  ([frame tint?]
   (let [{:keys [view proj]} (:frame/camera frame)
         cam-col (column :mat4 1 (into (vec view) proj)) ; stride-1, len 2 (view, proj)
         draw-cols
         (for [pass (:frame/passes frame)
               draw (:pass/draws pass)
               :let [inst (:draw/instances draw)]
               :when inst
               col  (if tint?
                      [(column :mat4 1 (:model inst))
                       (column :f16  4 (:tint inst))]   ; RGBA half, stride-4
                      [(column :mat4 1 (:model inst))])]
           col)]
     (into [cam-col] draw-cols))))

(defn frame->meta
  "The small, JSON-able draw-table sidecar that travels alongside the columnar
  buffer (ARCHITECTURE.md §9). The heavy per-instance matrices stay in the
  zero-copy buffer; this carries only the retained-by-id references the host needs
  to resolve handles + pick a pipeline, in the SAME order as the draw columns
  (column 0 is always the camera; columns 1..n map to :draws 0..n-1)."
  [frame]
  {:n     (:frame/n frame 0)
   :clear (:frame/clear frame)
   :draws (vec (for [pass (:frame/passes frame)
                     draw (:pass/draws pass)
                     :when (:draw/instances draw)]
                 {:pipeline (:draw/pipeline draw)
                  :mesh     (:draw/mesh draw)
                  :material (:draw/material draw)
                  :count    (:count (:draw/instances draw))}))})

(def ^:const version-tint
  "Layout version emitted when `pack` is asked for per-draw tint columns (v2).
  v1 (`version`) = camera + per-draw model only; v2 = camera + per-draw
  [model, tint-f16×4]. A decoder selects its draw-column stride from this byte."
  2)

(defn pack
  "Serialize a render-IR frame into a KamiFrame columnar buffer + draw-table meta.
  Returns {:buffer <vector of u8> :len n :ncols c :version v :columns [descriptor…]
           :layout [{:dtype .. :len .. :offset ..} …] :meta {…}}.
  Pure and platform-neutral; the browser backend memcpys :buffer into WASM memory
  and passes :meta (as JSON) to submit-frame.

  `opts` may set `:tint?` (default false): when true, emits the version-2 layout
  with a per-draw f16 RGBA tint column after each model column. The default is the
  version-1 layout — byte-identical to before — so existing decoders are unaffected
  until they opt into v2."
  ([frame] (pack frame nil))
  ([frame {:keys [tint?]}]
  (let [cols      (frame->columns frame (boolean tint?))
        ver       (if tint? version-tint version)
        ncols     (count cols)
        ;; header + column headers, then 16-aligned payloads
        hdr-end   (+ header-bytes (* ncols column-header-bytes))
        ;; compute payload offsets
        [layout payload-end]
        (reduce (fn [[acc off] c]
                  (let [pbytes (byte-len (:dtype c) (:len c) (:stride c))
                        start  (align16 off)]
                    [(conj acc {:dtype (:dtype c) :len (:len c)
                                :stride (:stride c) :offset start})
                     (+ start pbytes)]))
                [[] hdr-end]
                cols)
        total (align16 payload-end)
        ;; --- emit bytes ---
        frame-hdr (-> []
                      (into (u8s-of-u32 magic))
                      (into (u8s-of-u16 ver))
                      (into (u8s-of-u16 ncols))
                      (into (u8s-of-u32 (long (:frame/n frame 0))))
                      (into (u8s-of-u32 0)))            ; pad → 16 bytes
        col-hdrs  (reduce
                   (fn [acc {:keys [dtype len stride offset]}]
                     (-> acc
                         (conj (:enum (kami.ipc/dtype dtype)))
                         (conj (bit-and (long stride) 0xff))
                         (into (u8s-of-u16 0))           ; pad
                         (into (u8s-of-u32 (long len)))
                         (into (u8s-of-u32 (long offset)))
                         (into (u8s-of-u32 0))))         ; pad → 16 bytes
                   []
                   (map #(assoc %2 :dtype (:dtype %1)) cols layout))
        buf0 (pad-to (into frame-hdr col-hdrs) hdr-end)
        ;; write each column payload at its aligned offset
        buf  (reduce
              (fn [b [c {:keys [offset]}]]
                (let [b1   (pad-to b offset)
                      data (mapcat #(u8s-of-element (:dtype c) %) (:data c))]
                  (into b1 data)))
              buf0
              (map vector cols layout))]
    {:buffer (pad-to buf total) :len total :ncols ncols :version ver
     :columns cols :layout layout :meta (frame->meta frame)})))

;; ---------------------------------------------------------------------------
;; unpack — KamiFrame columnar byte vector → frame header + decoded columns
;; (clj mirror of kami-core/src/ipc.rs's KamiFrame reader; inverse of `pack`).
;; ---------------------------------------------------------------------------

(def ^:private enum->dtype
  "Inverse of the `dtype` table: Dtype enum byte → keyword tag."
  (into {} (map (fn [[k v]] [(:enum v) k])) dtype))

(defn- rd-u16 ^long [buf ^long off]
  (+ (long (nth buf off)) (* 256 (long (nth buf (inc off))))))

(defn- rd-u32 ^long [buf ^long off]
  (+ (long (nth buf off))
     (* 256       (long (nth buf (+ off 1))))
     (* 65536     (long (nth buf (+ off 2))))
     (* 16777216  (long (nth buf (+ off 3))))))

(defn- rd-f32 [buf ^long off]
  (let [bits (rd-u32 buf off)]
    #?(:clj  (Float/intBitsToFloat (unchecked-int bits))
       :cljs (let [b (js/ArrayBuffer. 4)]
               (aset (js/Uint32Array. b) 0 bits)
               (aget (js/Float32Array. b) 0)))))

(defn- pow2 [n] #?(:clj (Math/pow 2.0 n) :cljs (js/Math.pow 2.0 n)))

(defn- f16->f32
  "Decode IEEE-754 binary16 bits (0-0xffff) → a number. Inverse of `f16-bits`."
  [^long h]
  (let [sign (if (zero? (bit-and h 0x8000)) 1.0 -1.0)
        exp  (bit-and (unsigned-bit-shift-right h 10) 0x1F)
        man  (bit-and h 0x3FF)]
    (cond
      (= exp 0x1F) (if (zero? man)
                     (* sign #?(:clj Double/POSITIVE_INFINITY :cljs js/Infinity))
                     #?(:clj Double/NaN :cljs js/NaN))
      (zero? exp)  (* sign (pow2 -14) (/ man 1024.0))            ; subnormal (man=0 → ±0)
      :else        (* sign (pow2 (- exp 15)) (+ 1.0 (/ man 1024.0))))))

(defn- decode-element [dt buf ^long off]
  (case dt
    (:f32 :mat4) (rd-f32 buf off)
    :u32         (rd-u32 buf off)
    :u16         (rd-u16 buf off)
    :i16         (let [v (rd-u16 buf off)] (if (>= v 0x8000) (- v 0x10000) v))
    :u8          (long (nth buf off))
    (:f16 :quat) (f16->f32 (rd-u16 buf off))))

(defn unpack
  "Inverse of `pack`: parse a KAMI columnar buffer back into
  `{:n :version :ncols :columns [{:dtype :stride :len :offset :data} …]}`, where
  each `:data` is a flat vector of decoded numbers (f32/mat4 → f32; f16/quat → f32
  via `f16->f32`; integer dtypes → ints), in the same order `pack` wrote them.
  Verifies the 'KAMI' magic and reads the little-endian headers exactly as the
  Rust `KamiFrame` reader does. Bounds-checks every column header + payload against
  the buffer length and throws a typed `ex-info` (`:kami.ipc/error` ∈ #{:too-short
  :bad-magic :unknown-dtype :column-out-of-bounds}, mirroring the Rust decoder's
  `DecodeError`) rather than letting an out-of-range index escape. Pure; round-trips
  `pack`."
  [buffer]
  (let [buf (vec buffer)
        n   (count buf)]
    (when (< n header-bytes)
      (throw (ex-info "unpack: buffer shorter than 16-byte header"
                      {:kami.ipc/error :too-short :len n})))
    (let [magic* (rd-u32 buf 0)]
      (when-not (= magic* magic)
        (throw (ex-info "unpack: bad magic (not a KAMI frame)"
                        {:kami.ipc/error :bad-magic :got magic* :want magic}))))
    (let [version* (rd-u16 buf 4)
          ncols    (rd-u16 buf 6)
          frame-n  (rd-u32 buf 8)
          columns
          (mapv
           (fn [i]
             (let [base (+ header-bytes (* (long i) column-header-bytes))]
               (when (> (+ base column-header-bytes) n)
                 (throw (ex-info "unpack: truncated inside column headers"
                                 {:kami.ipc/error :too-short :column i
                                  :need (+ base column-header-bytes) :len n})))
               (let [enum   (long (nth buf base))
                     stride (long (nth buf (inc base)))
                     len    (rd-u32 buf (+ base 4))
                     off    (rd-u32 buf (+ base 8))
                     dt     (or (enum->dtype enum)
                                (throw (ex-info "unpack: unknown dtype enum"
                                                {:kami.ipc/error :unknown-dtype
                                                 :column i :enum enum})))
                     per    (case dt :mat4 16 :quat 4 1)
                     esz    (long (/ (long (:elsize (dtype dt))) per)) ; bytes per flat element
                     n-el   (* len per stride)
                     end    (+ off (* esz n-el))]
                 (when (> end n)
                   (throw (ex-info "unpack: column payload out of bounds"
                                   {:kami.ipc/error :column-out-of-bounds
                                    :column i :offset off :end end :len n})))
                 {:dtype dt :stride stride :len len :offset off
                  :data (mapv #(decode-element dt buf (+ off (* (long %) esz)))
                              (range n-el))})))
           (range ncols))]
      {:n frame-n :version version* :ncols ncols :columns columns})))
