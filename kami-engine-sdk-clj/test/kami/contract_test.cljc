(ns kami.contract-test
  "GPU-free, Datomic-free contract tests for the pure core: scene → ECS →
  render-IR → KAMI columnar packing, plus WGSL emission and matrix math.
  These pin the clj ↔ Rust contracts (ARCHITECTURE.md §7/§9) so they can be
  validated long before a GPU is wired up."
  (:require [clojure.test :refer [deftest testing is]]
            [kami.scene  :as scene]
            [kami.ecs    :as ecs]
            [kami.render :as render]
            [kami.ipc    :as ipc]
            [kami.wgsl   :as wgsl]
            [kami.math   :as m]))

;; --- fixtures ---------------------------------------------------------------

(def cam-eid #uuid "00000000-0000-0000-0000-0000000000ca")
(def tree-a  #uuid "00000000-0000-0000-0000-00000000000a")
(def tree-b  #uuid "00000000-0000-0000-0000-00000000000b")

(def assets
  [{:asset/id "mesh/conifer" :asset/kind :mesh     :asset/uri "b2://m/conifer"}
   {:asset/id "mat/bark"     :asset/kind :material :asset/uri "b2://m/bark"}])

(def entities
  [{:kami/eid cam-eid :kami/name "cam" :camera/active? true :camera/fov 60.0
    :camera/near 0.1 :camera/far 100.0 :transform/translation [0.0 0.0 5.0]}
   {:kami/eid tree-a :kami/name "tree-a" :transform/translation [-2.0 0.0 0.0]
    :mesh/asset [:asset/id "mesh/conifer"] :material/asset [:asset/id "mat/bark"]}
   {:kami/eid tree-b :kami/name "tree-b" :transform/translation [2.0 0.0 0.0]
    :mesh/asset [:asset/id "mesh/conifer"] :material/asset [:asset/id "mat/bark"]}])

(def snap (scene/build-snapshot entities assets {:t 1 :scene "test" :env {}}))

;; --- scene ------------------------------------------------------------------

(deftest scene-add-entity
  (testing "unknown attrs rejected"
    (is (thrown? #?(:clj Exception :cljs js/Error)
                 (scene/add-entity {:bogus/attr 1}))))
  (testing "auto eid"
    (is (uuid? (-> (scene/add-entity {:kami/name "x"}) first :kami/eid)))))

(deftest scene-valid
  (testing "well-formed snapshot validates"
    (is (true? (scene/valid? snap))))
  (testing "dangling asset ref caught"
    (is (thrown? #?(:clj Exception :cljs js/Error)
                 (scene/valid?
                  (scene/build-snapshot
                   [{:kami/eid tree-a :mesh/asset [:asset/id "missing"]}] [] {})))))
  (testing "two active cameras caught"
    (is (thrown? #?(:clj Exception :cljs js/Error)
                 (scene/valid?
                  (scene/build-snapshot
                   [{:kami/eid cam-eid :camera/active? true}
                    {:kami/eid tree-a :camera/active? true}] [] {})))))
  (testing "parent cycle caught"
    (is (thrown? #?(:clj Exception :cljs js/Error)
                 (scene/valid?
                  (scene/build-snapshot
                   [{:kami/eid tree-a :transform/parent {:kami/eid tree-b}}
                    {:kami/eid tree-b :transform/parent {:kami/eid tree-a}}] [] {}))))))

(deftest scene-tree
  (let [t (scene/tree [{:kami/eid tree-a}
                       {:kami/eid tree-b :transform/parent {:kami/eid tree-a}}]
                      tree-a)]
    (is (= tree-b (-> t (get tree-a) :children first :entity :kami/eid)))))

;; --- ecs --------------------------------------------------------------------

(deftest ecs-load+query
  (let [w (ecs/load-snapshot snap)]
    (is (= 1 (:basis-t w)))
    (is (= 2 (count (ecs/query w #{:mesh/asset}))))
    (is (= 1 (count (ecs/query w #{:camera/active?}))))
    (is (= 0 (count (ecs/query w #{:mesh/asset :camera/active?}))))))

(deftest ecs-dirty+tx
  (let [w0 (ecs/load-snapshot snap)
        w1 (ecs/set-component w0 tree-a :transform/translation [9.0 0.0 0.0])
        tx (ecs/->tx w1)]
    (testing "only the changed entity is in the tx"
      (is (= 1 (count tx)))
      (is (= tree-a (:kami/eid (first tx))))
      (is (= [9.0 0.0 0.0] (:transform/translation (first tx)))))
    (testing "no-op change yields empty tx"
      (is (empty? (ecs/->tx w0))))
    (testing "removal emits retractEntity"
      (let [tx2 (ecs/->tx (ecs/remove-entity w0 tree-b))]
        (is (= [[:db/retractEntity [:kami/eid tree-b]]] tx2))))
    (testing "mark-saved clears dirty and re-anchors t"
      (is (empty? (ecs/->tx (ecs/mark-saved w1 2)))))))

;; --- render-IR --------------------------------------------------------------

(deftest render-frame
  (let [w  (ecs/load-snapshot snap)
        fr (render/frame w {:n 7 :aspect 1.0})]
    (testing "frame shape"
      (is (= 7 (:frame/n fr)))
      (is (= render/nintendo-cream (:frame/clear fr)))
      (is (= 16 (count (-> fr :frame/camera :view))))
      (is (= 16 (count (-> fr :frame/camera :proj)))))
    (testing "two trees with the same (pipeline,mesh,material) merge into one instanced draw"
      (let [draws (-> fr :frame/passes first :pass/draws)]
        (is (= 1 (count draws)))
        (is (= :pbr (:draw/pipeline (first draws))))
        (is (= 2 (-> draws first :draw/instances :count)))
        (is (= 32 (-> draws first :draw/instances :model count))))) ; 2 × mat4(16)
    (testing "frame is serializable plain data (record/replay surface)"
      (is (= fr (read-string (pr-str fr)))))))

(deftest render-camera-translation
  (let [w (ecs/load-snapshot snap)
        view (-> (render/camera-ir w 1.0) :view)]
    ;; camera at +5 z → view translation column is -5 z
    (is (= -5.0 (nth view 14)))))

;; --- KAMI columnar packing --------------------------------------------------

(deftest ipc-pack
  (let [w  (ecs/load-snapshot snap)
        fr (render/frame w {:n 3 :aspect 1.0})
        {:keys [buffer len ncols layout]} (ipc/pack fr)]
    (testing "header magic 'KAMI' little-endian"
      (is (= [0x4B 0x41 0x4D 0x49] (subvec buffer 0 4))))
    (testing "buffer length is 16-byte aligned and matches :len"
      (is (= len (count buffer)))
      (is (zero? (mod len 16))))
    (testing "column count = camera + 1 instanced draw"
      (is (= 2 ncols))
      (is (= 2 (count layout))))
    (testing "every column payload offset is 16-byte aligned"
      (is (every? #(zero? (mod (:offset %) 16)) layout)))
    (testing "camera column is 2 mat4 items (view+proj), draw column is 2 instances"
      (is (= [2 2] (mapv :len layout)))
      (is (every? #(= :mat4 (:dtype %)) layout)))))

(deftest ipc-byte-len
  (is (= 64  (ipc/byte-len :mat4 1 1)))   ; one mat4 = 64B
  (is (= 128 (ipc/byte-len :mat4 2 1)))
  (is (= 16  (ipc/byte-len :f32 4 1)))
  (testing "half-precision dtypes"
    (is (= 2  (ipc/byte-len :f16 1 1)))   ; one half = 2B
    (is (= 8  (ipc/byte-len :f16 4 1)))
    (is (= 8  (ipc/byte-len :quat 1 1)))  ; one quat = 4×f16 = 8B
    (is (= 16 (ipc/byte-len :quat 2 1)))))

(deftest ipc-f16-encoding
  (let [f16 @#'ipc/f16-bits
        u8  @#'ipc/u8s-of-element]
    (testing "exact-representable f32 → known binary16 bit patterns"
      (is (= 0x0000 (f16 0.0)))
      (is (= 0x3C00 (f16 1.0)))
      (is (= 0x4000 (f16 2.0)))
      (is (= 0x3800 (f16 0.5)))
      (is (= 0xBC00 (f16 -1.0)))
      (is (= 0x7BFF (f16 65504.0))))            ; largest finite half
    (testing "exponent overflow saturates to ±Inf (not NaN)"
      (is (= 0x7C00 (f16 1.0e5)))
      (is (= 0xFC00 (f16 -1.0e5))))
    (testing "round-to-nearest-even at the half-way point (both directions)"
      (is (= 0x3C00 (f16 (+ 1.0 (/ 1.0 2048.0)))))   ; tie → down to even mantissa 0
      (is (= 0x3C02 (f16 (+ 1.0 (/ 3.0 2048.0))))))   ; tie → up   to even mantissa 2
    (testing "byte emit is little-endian 2 bytes, shared by :f16 and :quat"
      (is (= [0x00 0x3C] (u8 :f16  1.0)))
      (is (= [0x00 0x3C] (u8 :quat 1.0)))           ; quat comps emit one half each
      (is (= [0xFF 0x7B] (u8 :f16  65504.0))))))

(deftest ipc-unpack-roundtrip
  (let [w      (ecs/load-snapshot snap)
        fr     (render/frame w {:n 7 :aspect 1.0})
        packed (ipc/pack fr)
        back   (ipc/unpack (:buffer packed))]
    (testing "header: magic verified, version + frame-n + ncols recovered"
      (is (= 1 (:version back)))
      (is (= 7 (:n back)))
      (is (= (:ncols packed) (:ncols back))))
    (testing "column descriptors (dtype/stride/len/offset) survive the round trip"
      (is (= (mapv #(select-keys % [:dtype :len :stride :offset]) (:layout packed))
             (mapv #(select-keys % [:dtype :len :stride :offset]) (:columns back)))))
    (testing "mat4/f32 payloads decode bit-exactly back to the source numbers"
      (is (= (mapv #(mapv float (:data %)) (:columns packed))
             (mapv :data (:columns back)))))
    (testing "rejects a buffer whose magic isn't 'KAMI'"
      (is (thrown? #?(:clj Exception :cljs js/Error)
                   (ipc/unpack (assoc (:buffer packed) 0 0)))))))

(deftest ipc-unpack-rejects-malformed
  ;; typed errors mirror the Rust decoder's DecodeError (no raw index-out-of-range).
  ;; frame has 2 columns → header region = 16 + 2×16 = 48 bytes.
  (let [w   (ecs/load-snapshot snap)
        buf (:buffer (ipc/pack (render/frame w {:n 1 :aspect 1.0})))
        err (fn [b] (try (ipc/unpack b) :no-throw
                         (catch #?(:clj clojure.lang.ExceptionInfo :cljs ExceptionInfo) e
                           (:kami.ipc/error (ex-data e)))))]
    (is (= 2 (:ncols (ipc/unpack buf))))                         ; sanity: valid buffer decodes
    (testing "every malformation yields a typed kami.ipc error"
      (is (= :bad-magic            (err (assoc buf 0 0))))       ; clobbered magic
      (is (= :too-short            (err (subvec buf 0 8))))      ; below the 16-byte header
      (is (= :too-short            (err (subvec buf 0 24))))     ; truncated mid column header
      (is (= :column-out-of-bounds (err (subvec buf 0 48))))     ; headers ok, payload gone
      (is (= :unknown-dtype        (err (assoc buf 16 9)))))))   ; col0 dtype byte → unknown tag

(deftest ipc-f16-roundtrip
  (let [f16  @#'ipc/f16-bits
        f16' @#'ipc/f16->f32
        abs* (fn [v] (if (neg? v) (- v) v))]
    (testing "encode→decode stays within half-precision relative error"
      (doseq [x [0.0 1.0 -1.0 0.5 -2.5 100.0 -0.001 65504.0 0.333]]
        (let [y   (f16' (f16 x))
              tol (* 1.0e-3 (+ 1.0 (abs* (double x))))]
          (is (< (abs* (- (double x) (double y))) tol) (str x " → " y)))))
    (testing "±Inf survive the half round trip"
      (is (= #?(:clj Double/POSITIVE_INFINITY :cljs js/Infinity) (f16' (f16 1.0e9))))
      (is (= #?(:clj Double/NEGATIVE_INFINITY :cljs (- js/Infinity)) (f16' (f16 -1.0e9)))))))

(deftest ipc-pack-deterministic
  ;; merge-instances sorts renderables by eid so pack output is byte-reproducible
  ;; (the record/replay + cross-language golden surface, ARCHITECTURE.md §7/§9).
  ;; This pins that invariant, which the Rust decoder fixture (gen_fixture) relies on.
  (let [cam* (nth entities 0)
        ta   (nth entities 1)                       ; tree-a @ x=-2
        tb   (nth entities 2)                       ; tree-b @ x=+2
        buf  (fn [ents]
               (-> (scene/build-snapshot ents assets {:t 1 :scene "det" :env {}})
                   ecs/load-snapshot
                   (render/frame {:n 5 :aspect 1.0})
                   ipc/pack
                   :buffer))]
    (testing "pack is byte-reproducible across repeated calls"
      (is (= (buf [cam* ta tb]) (buf [cam* ta tb]))))
    (testing "instance ordering is independent of ECS insertion order (sort-by eid)"
      (is (= (buf [cam* ta tb])
             (buf [cam* tb ta])
             (buf [tb cam* ta]))))
    (testing "non-vacuous: instance transforms do reach the bytes (so order matters)"
      (is (not= (buf [cam* ta tb])
                (buf [cam* (assoc ta :transform/translation [-99.0 0.0 0.0]) tb]))))))

(deftest ipc-pack-tint-v2
  (let [w  (ecs/load-snapshot snap)
        fr (render/frame w {:n 3 :aspect 1.0})
        v1 (ipc/pack fr)
        v2 (ipc/pack fr {:tint? true})]
    (testing "default pack is unchanged: version 1, camera + 1 model column"
      (is (= 1 (:version v1)))
      (is (= 2 (:ncols v1)))
      (is (every? #(= :mat4 (:dtype %)) (:columns v1))))
    (testing "v2 adds an f16 RGBA tint column after each draw's model column"
      (is (= 2 (:version v2)))
      (is (= 3 (:ncols v2)))                              ; camera + (model + tint)
      (is (= [:mat4 :mat4 :f16] (mapv :dtype (:columns v2)))))
    (testing "v2 buffer round-trips through unpack (version, alignment, tint values)"
      (let [back (ipc/unpack (:buffer v2))
            tint (last (:columns back))]
        (is (= 2 (:version back)))
        (is (every? #(zero? (mod (:offset %) 16)) (:columns back)))
        (is (= :f16 (:dtype tint)))
        (is (= 2 (:len tint)))                             ; 2 instances
        (is (= 8 (count (:data tint))))                    ; 2 × RGBA
        (is (every? #(< (abs (- 1.0 (double %))) 1.0e-3) (:data tint))))))) ; default white

(deftest ipc-pack-tint-values
  ;; per-entity :material/tint flows through render → v2 tint column (not just white).
  (let [red   [1.0 0.0 0.0 1.0]
        green [0.0 1.0 0.0 1.0]
        ents  [(nth entities 0)                                ; camera
               (assoc (nth entities 1) :material/tint red)     ; tree-a (eid …000a)
               (assoc (nth entities 2) :material/tint green)]  ; tree-b (eid …000b)
        snap* (scene/build-snapshot ents assets {:t 1 :scene "tintvals" :env {}})
        w     (ecs/load-snapshot snap*)
        v2    (ipc/pack (render/frame w {:n 1 :aspect 1.0}) {:tint? true})
        back  (ipc/unpack (:buffer v2))
        tint  (:data (last (:columns back)))]
    (testing "per-instance tint reaches the v2 column in eid order (a=red, b=green)"
      (is (= 8 (count tint)))
      (is (every? true? (map #(< (abs (- (double %1) (double %2))) 1.0e-3)
                             tint (concat red green)))))))

;; --- WGSL emission ----------------------------------------------------------

(def ripple-shader
  {:wgsl/name "ripple"
   :wgsl/bindings [{:group 0 :binding 0 :var :uniform :name "u" :type :Globals}]
   :wgsl/structs  {:Globals [[:mvp :mat4x4<f32>]]}
   :wgsl/vertex   {:in  [[:pos :vec3<f32> {:location 0}]]
                   :out [[:clip :vec4<f32> :builtin/position]]
                   :body '[(set! out.clip (* u.mvp (vec4 in.pos 1.0)))]}
   :wgsl/fragment {:out [[:color :vec4<f32> {:location 0}]]
                   :body '[(set! out.color (vec4 0.3 0.6 1.0 1.0))]}})

(deftest wgsl-emit
  (let [src (wgsl/emit ripple-shader)]
    (is (re-find #"@group\(0\) @binding\(0\) var<uniform> u: Globals;" src))
    (is (re-find #"struct Globals" src))
    (is (re-find #"@vertex" src))
    (is (re-find #"@fragment" src))
    (is (re-find #"@builtin\(position\)" src))
    (is (re-find #"out.clip = \(u.mvp \* vec4<f32>\(in.pos, 1.0\)\);" src))
    (is (re-find #"out.color = vec4<f32>\(0.3, 0.6, 1.0, 1.0\);" src)))
  (testing "built-in pipelines need no WGSL"
    (is (wgsl/builtin? :pbr))
    (is (not (wgsl/builtin? "custom/ripple")))))

;; --- math -------------------------------------------------------------------

(deftest math-identity-mul
  (is (= m/identity4 (m/mul m/identity4 m/identity4))))

(deftest math-trs-translation
  (let [mm (m/from-trs [1.0 2.0 3.0] [0.0 0.0 0.0 1.0] [1.0 1.0 1.0])]
    (is (= [1.0 2.0 3.0] [(nth mm 12) (nth mm 13) (nth mm 14)]))))
