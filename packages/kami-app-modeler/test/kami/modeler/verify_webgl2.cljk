(ns kami.modeler.verify-webgl2
  (:require ["playwright" :refer [chromium]]
            [kami.modeler.browser :as browser]
            [promesa.core :as p]))

(p/let [{:keys [server url]} (browser/start-server!)
        instance (.launch chromium #js {:headless true :executablePath (.executablePath chromium)})
        page (.newPage instance #js {:viewport #js {:width 1280 :height 800}})
        _ (.addInitScript page "Object.defineProperty(navigator, 'gpu', {value: undefined, configurable: false})")
        errors (browser/browser-errors! page)
        _ (.goto page url #js {:waitUntil "networkidle"})
        _ (.waitForFunction page "() => window.__kami_modeler_ready === true" nil #js {:timeout 60000})
        result (.evaluate page "window.__kami_large_scene_stress()")]
  (browser/assert!
   (and (= "webgl2" (.-backend result)) (= 20000 (.-instances result))
        (>= (.-capacity result) 20000) (= 1 (.-geometryKinds result))
        (= 1 (.-drawCalls result)) (>= (.-residentTriangles result) 10000000)
        (< (.-firstMs result) 3000) (= 120 (.-sampleFrames result))
        (< (.-p95SubmitMs result) 33.34) (< (.-maxSubmitMs result) 100)
        (< (.-instanceBufferBytes result) 10000000)
        (= "[1,10000,20000]" (js/JSON.stringify (.-pickingIds result)))
        (= "modeler-large-scene-v1" (.-provenanceRevision result)))
   (str "20,000-instance / 10M-triangle WebGL2 fallback gate failed: " (js/JSON.stringify result)))
  (browser/assert! (empty? @errors) (str "Browser errors: " (.join (clj->js @errors) "\n")))
  (p/let [_ (.screenshot page #js {:path "test/modeler-webgl2.png"})
          _ (.close instance)
          _ (browser/close-server! server)]
    (println (js/JSON.stringify #js {:webgl2 true :largeScene result}))
    (.exit js/process 0)))
