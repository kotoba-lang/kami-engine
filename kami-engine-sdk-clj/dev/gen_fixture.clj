(ns gen-fixture
  "Emit a deterministic KAMI columnar frame to a binary fixture so the Rust
  decoder (kami-clj-host) can prove it parses the exact bytes kami.ipc/pack emits.
  This is the cross-language contract anchor for the clj brain ↔ Rust GPU arm.

  Run: clojure -Sdeps '{:paths [\"src\" \"dev\"]}' -M -m gen-fixture <out-path>"
  (:require [kami.scene  :as scene]
            [kami.ecs    :as ecs]
            [kami.render :as render]
            [kami.ipc    :as ipc]
            [clojure.java.io :as io]
            [clojure.string :as str]))

(def cam  #uuid "00000000-0000-0000-0000-0000000000ca")
(def t1   #uuid "00000000-0000-0000-0000-00000000000a")
(def t2   #uuid "00000000-0000-0000-0000-00000000000b")

(def snap
  (scene/build-snapshot
   [{:kami/eid cam :camera/active? true :camera/fov 60.0 :camera/near 0.1
     :camera/far 100.0 :transform/translation [0.0 0.0 5.0]}
    {:kami/eid t1 :transform/translation [-2.0 0.0 0.0]
     :mesh/asset {:asset/id "mesh/conifer"} :material/asset {:asset/id "mat/bark"}}
    {:kami/eid t2 :transform/translation [2.0 0.0 0.0]
     :mesh/asset {:asset/id "mesh/conifer"} :material/asset {:asset/id "mat/bark"}}]
   [{:asset/id "mesh/conifer" :asset/kind :mesh}
    {:asset/id "mat/bark" :asset/kind :material}]
   {:t 0 :scene "fixture" :env {}}))

(defn- write-bin!
  "Write a packed buffer (vector of u8) to `out` as raw bytes; return byte count."
  [out buffer]
  (let [bytes (byte-array (map (fn [b] (unchecked-byte b)) buffer))]
    (io/make-parents out)
    (with-open [o (io/output-stream out)] (.write o bytes))
    (count bytes)))

(defn -main [& [out]]
  (let [out    (or out "../kami-clj-host/tests/fixtures/frame.bin")
        out2   (clojure.string/replace out #"\.bin$" "_v2.bin")
        world  (ecs/load-snapshot snap)
        frame  (render/frame world {:n 42 :aspect 1.0})
        v1     (ipc/pack frame)                ; v1 layout — the existing anchor
        v2     (ipc/pack frame {:tint? true})] ; v2 layout — adds per-draw f16 tint
    (println "wrote" (write-bin! out (:buffer v1)) "bytes →" out)
    (println "ncols:" (:ncols v1) "meta:" (pr-str (:meta v1)))
    (println "layout:" (pr-str (:layout v1)))
    (println "expected: magic=KAMI version=1 frame_n=42")
    (println "  col0 = camera: 2 mat4 (view, proj); view[14] = -5.0")
    (println "  col1 = instances: 2 mat4; x-translations (idx12) = #{-2.0 2.0}")
    (println)
    (println "wrote" (write-bin! out2 (:buffer v2)) "bytes →" out2)
    (println "v2 ncols:" (:ncols v2) "version:" (:version v2)
             "dtypes:" (pr-str (mapv :dtype (:columns v2))))
    (println "expected: version=2; per draw = [model mat4, tint f16×4]; tint default white")
    (System/exit 0)))
