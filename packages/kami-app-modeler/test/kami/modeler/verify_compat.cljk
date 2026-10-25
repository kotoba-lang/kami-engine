(ns kami.modeler.verify-compat
  (:require ["playwright" :refer [chromium firefox webkit]]
            [kami.modeler.browser :as browser]
            [promesa.core :as p]))

(def engines {"chromium" chromium "firefox" firefox "webkit" webkit})

(p/let [{:keys [server url]} (browser/start-server!)
        name (or (.. js/process -env -BROWSER) "chromium")
        instance (.launch (get engines name) #js {:headless true})
        page (.newPage instance #js {:viewport #js {:width 1280 :height 800}})
        errors (browser/browser-errors! page)
        _ (.goto page (or (.. js/process -env -MODELER_URL) url) #js {:waitUntil "networkidle"})
        _ (.waitForFunction page "() => window.__kami_modeler_ready === true" nil #js {:timeout 60000})
        _ (.click page "#new-cube")
        _ (.click page "#save-project")
        state (.evaluate page "JSON.parse(document.querySelector('#debug-state').textContent)")]
  (browser/assert! (and (= 2 (.-objectCount state)) (= "saved" (.-saveStatus state)))
                   (str "Core workflow failed: " (js/JSON.stringify state)))
  (browser/assert! (empty? @errors) (.join (clj->js @errors) "\n"))
  (p/let [_ (.close instance)
          _ (browser/close-server! server)]
    (println (js/JSON.stringify #js {:browser name :coreWorkflow true}))))
