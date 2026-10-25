(ns kami.modeler.verify-webgpu
  (:require ["node:fs/promises" :as fs]
            ["playwright" :refer [chromium]]
            [kami.modeler.browser :as browser]
            [promesa.core :as p]))

(defn state! [page]
  (.evaluate page "JSON.parse(document.querySelector('#debug-state').textContent)"))

(defn topology! [page]
  (.evaluate page "({vertices: window.__kami_modeler_mesh.vertices?.length ?? window.__kami_modeler_mesh['mesh/vertices']?.length, faces: window.__kami_modeler_mesh.faces?.length ?? window.__kami_modeler_mesh['mesh/faces']?.length})"))

(defn topology-grew! [before after operation]
  (browser/assert! (and (> (.-vertices after) (.-vertices before))
                        (> (.-faces after) (.-faces before)))
                   (str operation " did not change topology: "
                        (js/JSON.stringify #js {:before before :after after}))))

(p/let [{:keys [server url]} (browser/start-server!)
        instance (.launch chromium #js {:headless true :executablePath (.executablePath chromium)})
        page (.newPage instance #js {:viewport #js {:width 1280 :height 800}})
        errors (browser/browser-errors! page)
        _ (.goto page (or (.. js/process -env -MODELER_URL) url) #js {:waitUntil "networkidle"})
        gpu? (.evaluate page "navigator.gpu && navigator.gpu.requestAdapter().then(Boolean)")
        _ (browser/assert! gpu? "A real WebGPU adapter is required for this verification")
        _ (.waitForFunction page "() => window.__kami_modeler_ready === true" nil #js {:timeout 60000})

        _ (.click page "#new-cube")
        _ (.selectOption page "#boolean-target" "1")
        _ (.click page "#boolean-union")
        boolean-state (state! page)
        _ (browser/assert! (and (= 1 (.-objectCount boolean-state))
                                (< (js/Math.abs (- (.-signedVolume boolean-state) 12)) 0.00001))
                           (str "BSP Boolean union failed: " (js/JSON.stringify boolean-state)))

        before (topology! page)
        _ (.click page "#extrude")
        after (topology! page)
        _ (topology-grew! before after "Extrude")
        _ (.click page "#inset")
        inset (topology! page)
        _ (topology-grew! after inset "Inset")
        _ (.click page "#bevel")
        bevel (topology! page)
        _ (topology-grew! inset bevel "Bevel")
        _ (.click page "#loop-cut")
        loop-cut (topology! page)
        _ (browser/assert! (and (= (+ 2 (.-vertices bevel)) (.-vertices loop-cut))
                                (= (inc (.-faces bevel)) (.-faces loop-cut)))
                           (str "Loop cut failed: " (js/JSON.stringify loop-cut)))
        _ (.click page "#knife")
        knife (topology! page)
        _ (browser/assert! (and (= (+ 2 (.-vertices loop-cut)) (.-vertices knife))
                                (= (inc (.-faces loop-cut)) (.-faces knife)))
                           (str "Knife failed: " (js/JSON.stringify knife)))

        _ (.click page "#unwrap-uv")
        data-uri (.evaluate page "(() => { const canvas = document.createElement('canvas'); canvas.width = 2; canvas.height = 2; const context = canvas.getContext('2d'); context.fillStyle = '#ff3366'; context.fillRect(0, 0, 1, 2); context.fillStyle = '#33ccff'; context.fillRect(1, 0, 1, 2); return canvas.toDataURL('image/png'); })()")
        png (js/Buffer.from (second (.split data-uri ",")) "base64")
        _ (.setInputFiles (.locator page "#texture-file")
                          #js {:name "pixel.png" :mimeType "image/png" :buffer png})
        _ (.waitForFunction page "() => JSON.parse(document.querySelector('#debug-state').textContent).textureLoaded === true" nil #js {:timeout 30000})
        texture-state (state! page)
        _ (browser/assert! (and (= 1 (.-textureCacheCount texture-state))
                                (.-textureDevice texture-state)
                                (= (.-uvCount texture-state) (.-vertices knife))
                                (.startsWith (aget (.-material texture-state) "base-color-texture")
                                             "data:image/png;base64,"))
                           (str "Texture upload failed: " (js/JSON.stringify texture-state)))

        export-topology (topology! page)
        export-state (state! page)
        _ (.click page "#save-project")
        download-result (.all js/Promise #js [(.waitForEvent page "download")
                                              (.click page "#export-gltf")])
        download (aget download-result 0)
        gltf-path (.path download)
        gltf (fs/readFile gltf-path)
        _ (.setInputFiles (.locator page "#import-gltf-file")
                          #js {:name "roundtrip.gltf" :mimeType "model/gltf+json" :buffer gltf})
        _ (.waitForFunction page
                            "() => (window.__kami_modeler_mesh.vertices?.length ?? window.__kami_modeler_mesh['mesh/vertices']?.length) > 0"
                            nil #js {:timeout 60000})
        roundtrip (topology! page)
        roundtrip-state (state! page)
        _ (browser/assert! (and (> (.-vertices roundtrip) 0) (> (.-faces roundtrip) 0)
                                (< (js/Math.abs (- (.-signedVolume export-state)
                                                   (.-signedVolume roundtrip-state))) 0.00001))
                           (str "glTF round trip changed topology: "
                                (js/JSON.stringify #js {:before export-topology :after roundtrip})))

        elapsed (.evaluate page "(() => { const start = performance.now(); for (let index = 0; index < 99; index += 1) document.querySelector('#new-cube').click(); return performance.now() - start; })()")
        _ (.waitForFunction page "() => JSON.parse(document.querySelector('#debug-state').textContent).objectCount === 100" nil #js {:timeout 60000})
        stress (state! page)
        _ (browser/assert! (and (= 100 (.-objectCount stress)) (< elapsed 10000))
                           (str "100-object WebGPU scene gate failed: " (js/JSON.stringify stress)))
        large (.evaluate page "window.__kami_large_scene_stress()")]
  (browser/assert!
   (and (= "webgpu" (.-backend large)) (= 20000 (.-instances large))
        (>= (.-capacity large) 20000) (= 1 (.-geometryKinds large))
        (= 1 (.-drawCalls large)) (>= (.-residentTriangles large) 10000000)
        (< (.-firstMs large) 3000) (= 120 (.-sampleFrames large))
        (< (.-p95SubmitMs large) 33.34) (< (.-maxSubmitMs large) 100)
        (< (.-instanceBufferBytes large) 10000000)
        (= "[1,10000,20000]" (js/JSON.stringify (.-pickingIds large)))
        (= "modeler-large-scene-v1" (.-provenanceRevision large)))
   (str "20,000-instance / 10M-triangle WebGPU gate failed: " (js/JSON.stringify large)))
  (browser/assert! (empty? @errors) (str "Browser errors: " (.join (clj->js @errors) "\n")))
  (p/let [_ (.screenshot page #js {:path "test/modeler-webgpu.png"})
          _ (.close instance)
          _ (browser/close-server! server)]
    (println (js/JSON.stringify #js {:webgpu true :largeScene large :roundtrip roundtrip}))))
