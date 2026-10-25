(ns kotoba.amenominaka.pages
  "Static Archviz presentation shell. The live WebGPU executor remains in
  `kotoba.amenominaka.ui`; this Pages entry is generated only by kotoba HTML/CSS."
  (:require [html.core :as html] [css.core :as css]))

(def styles {:rules
             {"body" {:margin 0 :min-height "100vh" :font-family "system-ui" :background "#08131a" :color "#f1f6f7"}
              "header" {:padding "18px 28px" :display :flex :justify-content :space-between :align-items :center :background "#0e222c" :border-bottom "1px solid #294650"}
              "main" {:display :grid :grid-template-columns "280px 1fr" :min-height "calc(100vh - 61px)"}
              "aside" {:padding 22 :background "#102b35" :border-right "1px solid #294650"}
              "button" {:display :block :width "100%" :margin "9px 0" :padding 10 :border "1px solid #46717e" :border-radius 7 :background "#173c48" :color "#effcff" :text-align :left}
              ".hero" {:display :grid :place-items :center :background "radial-gradient(circle at 50% 38%,#518a92,#16313b 48%,#08131a 90%)"}
              ".card" {:width "min(680px,80%)" :padding 28 :border "1px solid #82b5b8" :border-radius 12 :background "#0d2028cc" :box-shadow "0 20px 80px #0008"}
              ".metrics" {:display :grid :grid-template-columns "repeat(3,1fr)" :gap 10 :margin-top 22}
              ".metric" {:padding 12 :background "#17353e" :border-radius 7}
              "small" {:color "#add0d4"}}})

(defn page []
  (html/html5
   [:html [:head [:meta {:charset "utf-8"}] [:meta {:name "viewport" :content "width=device-width,initial-scale=1"}]
           [:title "Amenominaka · Archviz"] [:style (css/css styles)]]
    [:body [:header [:strong "天之御中 · AMENOMINAKA"] [:a {:href "https://kotoba-lang.github.io/kami-studio/"} "Studio"] [:small "Realtime architectural visualization"]]
     [:main [:aside [:h3 "Scene"] [:button "Quarry Walk Lodge"] [:button "Import BIM / USD"] [:h3 "Environment"] [:button "Overcast · Plains"] [:button "Grass · Soft light"] [:h3 "Output"] [:button "Capture image"] [:button "Export USD / glTF"]]
      [:section.hero [:article.card [:small "WALKTHROUGH READY"] [:h1 "Architecture, in real time."] [:p "Compose BIM geometry with atmosphere, terrain, vegetation and post-processing. The live application uses the existing WebGPU renderer; this page presents the same operational workflow."] [:div.metrics [:div.metric [:strong "USD"] [:br] [:small "scene export"]] [:div.metric [:strong "glTF"] [:br] [:small "interchange"]] [:div.metric [:strong "WebGPU"] [:br] [:small "realtime"]]]]]]]]))
