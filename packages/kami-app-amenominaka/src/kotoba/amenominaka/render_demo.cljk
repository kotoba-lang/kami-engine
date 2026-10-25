(ns kotoba.amenominaka.render-demo
  "M2 (ADR-2607100100) real-browser smoke test entry point — not part of
  the library surface. Composes the same sample building used by the M0/
  M1/M2 test suites, bridges it through `kotoba.amenominaka.render-ir`,
  and draws one real frame via `kami.webgpu/init!`+`draw!`. Writes
  `ok`/`error: ...` to `#out` for `test/verify-m2-render.mjs`'s
  Playwright harness to read."
  (:require [bim]
            [kotoba.amenominaka.scene :as amenominaka-scene]
            [kotoba.amenominaka.render-ir :as render-ir]
            [kami.webgpu :as webgpu]))

(defn- sample-building []
  (-> (bim/project "Quarry Walk Lodge")
      (update :sites conj
              (bim/site
               {:id 1 :name "Site 1" :geo nil :placement :identity
                :buildings
                [(bim/building
                  {:id 2 :name "Lodge" :placement :identity :reference-elevation 0.0
                   :storeys
                   [(bim/storey
                     {:id 3 :name "L1" :elevation 0.0 :height 3.5 :placement :identity
                      :spaces []
                      :elements
                      [(bim/element
                        {:id 4 :kind :wall :name "Wall 1" :global-id "GUID-0004"
                         :placement :identity
                         :geometry (bim/axis-sweep-geometry
                                    [[0.0 0.0 0.0] [10.0 0.0 0.0]]
                                    (bim/rectangle-profile 0.2 3.5))})]})]})]}))))

(defn- sample-scene []
  (amenominaka-scene/compose {:building (sample-building) :weather "overcast" :terrain "plains"}))

(defn- set-out! [text]
  (when-let [el (.getElementById js/document "out")]
    (set! (.-textContent el) text)))

(defn init! []
  (let [canvas (.getElementById js/document "canvas")
        ir (render-ir/scene->render-ir (sample-scene))]
    (-> (webgpu/init! canvas)
        (.then (fn [ctx] (webgpu/draw! ctx ir) (set-out! "ok")))
        (.catch (fn [err] (set-out! (str "error: " err)))))))
