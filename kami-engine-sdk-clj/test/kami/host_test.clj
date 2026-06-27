(ns kami.host-test
  "JVM headless-backend tests: kami.backend.host implements IGpuBackend by decoding
  each submitted frame with kami.ipc/unpack and recording verification stats — no
  GPU. Exercises the asset-registration, submit/verify, resize, and strict paths."
  (:require [clojure.test :refer [deftest testing is]]
            [kami.scene  :as scene]
            [kami.ecs    :as ecs]
            [kami.gpu    :as gpu]
            [kami.ipc    :as ipc]
            [kami.render :as render]
            [kami.backend.host :as host]))

(def cam  #uuid "00000000-0000-0000-0000-0000000000c1")
(def tree #uuid "00000000-0000-0000-0000-000000000a01")

(def snap
  (scene/build-snapshot
   [{:kami/eid cam :camera/active? true :camera/fov 60.0 :camera/near 0.1
     :camera/far 100.0 :transform/translation [0.0 0.0 5.0]}
    {:kami/eid tree :transform/translation [0.0 0.0 0.0]
     :transform/rotation [0.0 0.0 0.0 1.0]
     :mesh/asset {:asset/id "mesh/conifer"} :material/asset {:asset/id "mat/bark"}}]
   [{:asset/id "mesh/conifer" :asset/kind :mesh
     :asset/data {:vertices [0.0 1.0 2.0] :indices [0 1 2]}}
    {:asset/id "mat/bark" :asset/kind :material :asset/data {:params [1.0 1.0 1.0 1.0]}}]
   {:t 1 :scene "host" :env {}}))

(deftest host-backend-registers-and-verifies
  (let [b (host/make {})
        w (ecs/load-snapshot snap)]
    (gpu/ensure-assets! b snap)
    (gpu/submit! b (render/frame w {:n 1 :aspect 1.0}))
    (gpu/submit! b (render/frame w {:n 2 :aspect 1.0}))
    (gpu/resize! b 800 600)
    (let [s (host/state b)]
      (testing "assets registered once, keyed by id"
        (is (contains? (:meshes s) "mesh/conifer"))
        (is (= {:vertices 3 :indices 3} (get-in s [:meshes "mesh/conifer"])))
        (is (contains? (:materials s) "mat/bark")))
      (testing "every submitted frame decoded + verified ok, in order"
        (is (= 2 (count (:frames s))))
        (is (every? :ok (:frames s)))
        (is (= [1 2] (mapv :n (:frames s)))))
      (testing "verified ncols agrees with what pack produced"
        (let [packed (ipc/pack (render/frame w {:n 1 :aspect 1.0}))]
          (is (= (:ncols packed) (:ncols (first (:frames s)))))))
      (testing "resize recorded"
        (is (= [800 600] (:size s)))))))

(deftest host-backend-submit-forwards-pack-opts
  ;; gpu/submit! forwards pack-opts to ipc/pack, so {:tint? true} reaches the
  ;; backend as a verified v2 frame (the browser path's tint enable switch).
  (let [w (ecs/load-snapshot snap)
        b (host/make {})]
    (gpu/submit! b (render/frame w {:n 1 :aspect 1.0}))               ; default → v1
    (gpu/submit! b (render/frame w {:n 2 :aspect 1.0}) {:tint? true}) ; opts → v2
    (let [[f1 f2] (:frames (host/state b))]
      (is (every? :ok [f1 f2]))
      (is (= 1 (:version f1)))
      (is (= 2 (:version f2)))
      (is (< (:ncols f1) (:ncols f2))))))                             ; v2 adds tint columns

(deftest host-backend-flags-corrupt-frame
  (let [w       (ecs/load-snapshot snap)
        packed  (ipc/pack (render/frame w {:n 9 :aspect 1.0}))
        corrupt (assoc packed :buffer (assoc (:buffer packed) 0 0))] ; clobber 'KAMI' magic
    (testing "non-strict backend records the frame as not-ok with an error"
      (let [b (host/make {})]
        (gpu/submit-frame! b corrupt)
        (let [f (last (:frames (host/state b)))]
          (is (false? (:ok f)))
          (is (seq (:errors f))))))
    (testing "strict backend throws on a corrupt buffer"
      (let [b (host/make {:strict? true})]
        (is (thrown? Exception (gpu/submit-frame! b corrupt)))))))
