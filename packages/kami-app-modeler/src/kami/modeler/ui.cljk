(ns kami.modeler.ui
  "Kami Modeler UI, authored as portable EDN and rendered only through the
  kotoba-lang HTML/CSS substrates. Browser behavior is layered on this shell."
  (:require [html.core :as html]
            [css.core :as css]))

(def sheet {:rules
            {"*" {:box-sizing :border-box}
             "body" {:margin 0 :height "100vh" :display :grid :grid-template-rows "54px 1fr" :font-family "Inter,ui-sans-serif,system-ui" :background "#0b1020" :color "#e8ecf8"}
             "header" {:display :flex :align-items :center :gap 20 :padding "0 18px" :background "#111936" :border-bottom "1px solid #293456"}
             "header strong" {:color "#9eb8ff" :letter-spacing ".04em"}
             "header span,.hint" {:color "#9ca9c9" :font-size 12}
             "header button" {:margin-left :auto}
             "main" {:display :grid :grid-template-columns "230px 1fr 260px" :min-height 0}
             "aside" {:padding 18 :background "#101733" :display :flex :flex-direction :column :gap 9 :border-right "1px solid #293456"}
             "aside:last-child" {:border-left "1px solid #293456" :border-right 0}
             "h2" {:margin "8px 0 3px" :font-size 11 :text-transform :uppercase :letter-spacing ".12em" :color "#91a1ca"}
             "button" {:border "1px solid #35446e" :border-radius 7 :background "#171f3b" :color "#e8ecf8" :padding "9px 10px" :text-align :left :cursor :pointer}
             "button:hover,.selected" {:border-color "#7d9cff" :background "#293a70"}
             "button:focus-visible,input:focus-visible,select:focus-visible,a:focus-visible" {:outline "3px solid #ffd166" :outline-offset 2}
             ".primary" {:background "#6f8fff" :color "#071127" :border 0 :font-weight 700}
             ".viewport" {:position :relative :overflow :hidden :background "radial-gradient(circle at 50% 42%,#1e315a,#0a0f1e 62%)"}
             "#gpu-canvas" {:width "100%" :height "100%" :display :block}
             ".gpu-fallback" {:position :absolute :inset "45% auto auto 50%" :transform "translate(-50%,-50%)" :color "#ffb9b9"}
             ".axis,.status" {:position :absolute :font-family :ui-monospace :font-size 12 :color "#b8caff"}
             ".axis" {:bottom 24 :left 26 :color "#ffadb7"}
             ".status" {:right 18 :bottom 16 :display :flex :gap 14 :background "#0b1020bb" :padding 8 :border-radius 6}
             "label" {:display :grid :gap 8 :color "#bfcbe8"}
             "input,select" {:accent-color "#8aa8ff" :background "#171f3b" :color "#e8ecf8" :border "1px solid #35446e" :padding 8}
             ".modifier-row" {:display :grid :grid-template-columns "1fr 30px 30px 30px" :gap 4 :align-items :center :font-size 11}
             ".outliner-row" {:display :grid :grid-template-columns "1fr 38px 38px" :gap 4}
             ".outliner-row button" {:padding 6}
             ".modifier-row button" {:padding 5 :text-align :center}}
            :media {"(max-width: 900px)" {"body" {:height :auto :min-height "100vh"}
                                           "header" {:flex-wrap :wrap :height :auto :padding "10px 12px" :gap 8}
                                           "header button" {:margin-left 0}
                                           "main" {:grid-template-columns "1fr" :grid-template-rows "minmax(55vh,1fr) auto auto"}
                                           ".viewport" {:grid-row 1 :min-height "55vh"}
                                           "aside" {:border "1px solid #293456"}}
                    "(prefers-reduced-motion: reduce)" {"*" {:scroll-behavior :auto :transition :none :animation :none}}}})

(defn page []
  (html/html5
   [:html {:lang "en"}
    [:head [:meta {:charset "utf-8"}] [:meta {:name "viewport" :content "width=device-width,initial-scale=1"}]
     [:link {:rel "icon" :href "data:,"}]
     [:title "Kami Modeler"] [:style (css/css sheet)]]
    [:body [:header {:role "banner"} [:strong "KAMI MODELER"] [:a {:href "https://kotoba-lang.github.io/kami-studio/"} "Studio"] [:span "WebGPU polygon workspace"] [:span {:id "project-status" :role "status" :aria-live "polite"} "clean · r0"] [:button {:id "new-cube" :type "button"} "New Cube"] [:button {:id "save-project" :type "button" :aria-keyshortcuts "Control+S Meta+S"} "Save"] [:button {:id "load-project" :type "button"} "Load"] [:button {:id "import" :type "button"} "Import Project"] [:input {:id "import-file" :type "file" :accept ".edn,.kami-modeler.edn" :aria-label "Import modeler project" :style {:display "none"}}] [:button {:id "export" :type "button"} "Export Project"]]
     [:main [:aside [:h2 "Scene"] [:button.primary {:id "mode-toggle"} "Edit Mode"] [:div {:id "scene-tree"}] [:button {:id "duplicate-object"} "Duplicate object"] [:button {:id "delete-object"} "Delete object"] [:h2 "Edit tools"] [:button.selected {:data-command "select"} "Select Face"] [:button {:id "inset"} "Inset"] [:button {:id "scale"} "Scale Face"] [:button {:id "move"} "Move Face"] [:button {:id "delete-face"} "Delete Face"] [:h2 "Interaction profile"] [:select {:id "profile"} [:option {:value "blender"} "Blender"] [:option {:value "maya"} "Maya"] [:option {:value "max"} "3ds Max"] [:option {:value "c4d"} "Cinema 4D"]] [:p.hint {:id "shortcutHint"} "Click Select · E Extrude · MMB Orbit · Ctrl+Z Undo"]]
      [:section.viewport {:aria-label "3D modeling viewport"} [:canvas {:id "gpu-canvas" :tabindex 0 :aria-label "Interactive 3D mesh viewport. Use pointer to select and drag to orbit."}] [:div.gpu-fallback {:id "gpu-status" :role "status" :aria-live "polite"} "Initializing WebGPU…"] [:div.axis {:aria-hidden "true"} "Y\n│\n└── X"] [:div.status {:role "status" :aria-live "polite"} [:span {:id "tool"} "Select Face"] [:span {:id "meshStats"} "8 vertices · 6 faces"]]]
      [:aside [:h2 "Object Inspector"] [:label "Name" [:input {:id "object-name"}]] [:label "Location X" [:input {:id "tx" :type "number" :step 0.1}]] [:label "Location Y" [:input {:id "ty" :type "number" :step 0.1}]] [:label "Location Z" [:input {:id "tz" :type "number" :step 0.1}]] [:button {:id "apply-transform"} "Apply object transform"]
       [:h2 "Material"] [:label "Base Color" [:input {:id "base-color" :type "color" :value "#597fff"}]] [:label "Metallic " [:output {:id "metallic-value"} "0.00"] [:input {:id "metallic" :type "range" :min 0 :max 1 :step 0.01 :value 0}]] [:label "Roughness " [:output {:id "roughness-value"} "0.50"] [:input {:id "roughness" :type "range" :min 0 :max 1 :step 0.01 :value 0.5}]]
       [:h2 "UV Mapping"] [:label "Projection axis" [:select {:id "unwrap-axis"} [:option {:value "z"} "Z · Front"] [:option {:value "y"} "Y · Top"] [:option {:value "x"} "X · Side"]]] [:button {:id "unwrap-uv"} "Planar Unwrap"]
       [:h2 "Modifiers"] [:button {:id "add-mirror"} "＋ Mirror X"] [:button {:id "add-subdivision"} "＋ Subdivision"] [:button {:id "add-array"} "＋ Array"]
       [:label "Catalog" [:select {:id "modifier-kind"}
                           (for [kind ["translate" "scale" "triangulate" "flip-normals" "weld" "solidify"
                                       "planar-unwrap" "rotate" "shear" "taper" "twist" "bend" "spherize"
                                       "decimate" "remove-degenerate" "orient-outward" "snap-grid"
                                       "axis-project" "center-origin" "clamp" "radial-wave" "deterministic-jitter"]]
                             [:option {:value kind} kind])]]
       [:button {:id "add-catalog-modifier"} "＋ Add Modifier"] [:div {:id "modifier-stack"}]
       [:h2 "Face Inspector"] [:p {:id "selection"} "Face 1 selected"] [:label "Operation amount " [:output {:id "distanceValue"} "0.50"] [:input {:id "distance" :type "range" :min "0.05" :max "2" :step "0.05" :value "0.5"}]] [:button.primary {:id "extrude"} "Extrude selected face"] [:button {:id "undo"} "Undo"] [:button {:id "redo"} "Redo"]]]
     [:span {:id "debug-state" :style {:display "none"}}]
     [:canvas {:id "large-scene-canvas" :aria-label "Large scene WebGPU verification canvas"
               :style {:position "fixed" :left "-10000px" :top 0 :width "512px" :height "512px"}}]
     [:script {:src "./js/app.js"}]]]))
