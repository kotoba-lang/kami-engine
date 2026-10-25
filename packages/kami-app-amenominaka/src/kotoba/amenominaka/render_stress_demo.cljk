(ns kotoba.amenominaka.render-stress-demo
  "M4 investigation (ADR-2607100100): NOT part of the library surface —
  a real-browser stress-test entry point used to check whether M2's
  CPU-authored instancing actually hits a performance wall before
  deciding whether M4 (GPU-side `wgsl` `@compute` instancing) is
  warranted (ADR-2607100100 D6-b's own condition: only pursue M4 if a
  real wall is found, not speculatively). Instance count comes from the
  `?n=` URL query param (default 1000) so one compiled bundle covers
  many scales without a shadow-cljs build per scale. Draws `frames`
  (`?frames=`, default 60) real frames back-to-back and reports average
  frame time / fps to `#out` — the actual GPU-rasterization + per-frame
  executor cost, not just the CPU-side `scene->render-ir` bridge time
  (separately benchmarked JVM-side and found trivial even at 20k
  instances — this demo is what actually found the real wall, in
  `kotoba-lang/webgpu`'s `draw!`, not in this repo's own code — see
  ADR-2607100100's M4 addendum for the full investigation and fix)."
  (:require [bim]
            [kotoba.amenominaka.scene :as amenominaka-scene]
            [kotoba.amenominaka.render-ir :as render-ir]
            [kami.webgpu :as webgpu]))

(defn- query-param [name default]
  (let [params (js/URLSearchParams. (.-search js/location))
        v (.get params name)]
    (if v (js/parseInt v 10) default)))

(defn- grid-building [n]
  (let [side (long (js/Math.ceil (js/Math.sqrt n)))
        spacing 6.0
        elements (vec (for [i (range n)]
                        (let [row (quot i side) col (mod i side)
                              x0 (* col spacing) z0 (* row spacing)]
                          (bim/element
                           {:id (inc i) :kind :wall :name (str "Wall " i) :global-id (str "GUID-" i)
                            :placement :identity
                            :geometry (bim/axis-sweep-geometry [[x0 0.0 z0] [(+ x0 4.0) 0.0 z0]]
                                                                (bim/rectangle-profile 0.2 3.0))}))))]
    (-> (bim/project (str "Stress " n))
        (update :sites conj
                (bim/site {:id 1 :name "Site 1" :geo nil :placement :identity
                           :buildings
                           [(bim/building {:id 2 :name "B" :placement :identity :reference-elevation 0.0
                                           :storeys
                                           [(bim/storey {:id 3 :name "L1" :elevation 0.0 :height 3.0 :placement :identity
                                                         :spaces [] :elements elements})]})]})))))

(defn- set-out! [text]
  (when-let [el (.getElementById js/document "out")]
    (set! (.-textContent el) text)))

(defn- draw-n-frames! [ctx render-ir-data n done!]
  (let [times (atom [])]
    (letfn [(step [i]
              (if (>= i n)
                (done! @times)
                (let [t0 (js/performance.now)]
                  (webgpu/draw! ctx render-ir-data)
                  (let [t1 (js/performance.now)]
                    (swap! times conj (- t1 t0))
                    (js/requestAnimationFrame (fn [_] (step (inc i))))))))]
      (step 0))))

(defn init! []
  (let [n (query-param "n" 1000)
        frames (query-param "frames" 60)
        canvas (.getElementById js/document "canvas")
        t-compose0 (js/performance.now)
        scene (amenominaka-scene/compose {:building (grid-building n) :weather "overcast" :terrain "plains"})
        render-ir-data (render-ir/scene->render-ir scene)
        t-compose1 (js/performance.now)
        instance-count (count (:instances render-ir-data))]
    (-> (webgpu/init! canvas)
        (.then (fn [ctx]
                 (draw-n-frames!
                  ctx render-ir-data frames
                  (fn [times]
                    (let [avg (/ (reduce + times) (count times))
                          sorted (sort times)
                          p95 (nth sorted (min (dec (count sorted)) (js/Math.floor (* 0.95 (count sorted)))))
                          max-t (last sorted)]
                      (set-out!
                       (js/JSON.stringify
                        (clj->js {:n n :instanceCount instance-count :framesDrawn (count times)
                                  :composeMs (- t-compose1 t-compose0)
                                  :avgFrameMs avg :p95FrameMs p95 :maxFrameMs max-t
                                  :avgFps (/ 1000.0 avg)
                                  ;; ADR-2607100100 M6: instanceCount is the scene's INPUT count
                                  ;; (never truncated, always == n) — instBufferCapacity is what
                                  ;; the GPU instance buffer actually grew to. Before M6 this repo's
                                  ;; own instanceCount metric couldn't reveal the MAX-INST=16384
                                  ;; silent-truncation bug at all (draw!'s internal `take` was
                                  ;; invisible from here); this field makes that assertable now.
                                  :instBufferCapacity (webgpu/inst-buffer-capacity ctx)}))))))))
        (.catch (fn [err] (set-out! (str "error: " err)))))))
