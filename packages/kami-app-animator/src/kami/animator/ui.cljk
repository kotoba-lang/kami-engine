(ns kami.animator.ui (:require [html.core :as html] [css.core :as css]))

(def sheet {:rules
 {"*" {:box-sizing :border-box} "body" {:margin 0 :height "100vh" :display :grid :grid-template-rows "52px 1fr 340px" :font-family "Inter,system-ui" :background "#090d18" :color "#e8ecf8"}
  "header" {:display :flex :align-items :center :gap 18 :padding "0 16px" :background "#111936" :border-bottom "1px solid #293456"}
  "header .spacer" {:flex 1} "button,select,input" {:background "#18213c" :color "#eef2ff" :border "1px solid #40517d" :border-radius 5 :padding "7px 10px"}
  "button" {:cursor :pointer} ".primary" {:background "#6f8fff" :color "#071127" :font-weight 700}
  ".workspace" {:display :grid :grid-template-columns "220px 1fr 250px" :min-height 0}
  "aside" {:padding 14 :background "#10172b" :border-right "1px solid #293456" :overflow-y :auto} "aside:last-child" {:border-left "1px solid #293456" :border-right 0}
  "h2" {:font-size 11 :text-transform :uppercase :letter-spacing ".12em" :color "#91a1ca"}
  ".viewport" {:position :relative} "#gpu-canvas" {:width "100%" :height "100%" :display :block}
  "#gpu-status" {:position :absolute :left "50%" :top "45%" :transform "translate(-50%,-50%)" :color "#ffb5b5"}
  ".timeline" {:display :grid :grid-template-columns "220px 1fr 250px" :background "#0e1427" :border-top "1px solid #334267"}
  ".tracks" {:padding 14 :border-right "1px solid #334267"} ".lane" {:position :relative :margin "42px 18px" :height 50 :background "repeating-linear-gradient(90deg,#2b385b 0 1px,transparent 1px 10%)"}
  ".key" {:position :absolute :top 17 :width 14 :height 14 :border 0 :padding 0 :background "#87a7ff" :transform "translateX(-50%) rotate(45deg)"}
  ".key.selected" {:background "#ffd18f"} ".playhead" {:position :absolute :top 0 :bottom 0 :width 2 :background "#ff6f87" :pointer-events :none}
  "#scrub" {:width "calc(100% - 36px)"} ".inspector" {:padding 14 :border-left "1px solid #334267"} "label" {:display :grid :gap 5 :margin-bottom 10}
  "#curve-graph" {:width "calc(100% - 36px)" :height 150 :margin "0 18px" :background "#09101e"}
  ".graph-grid" {:stroke "#263554" :stroke-width 1} ".curve-path" {:fill :none :stroke "#82a7ff" :stroke-width 3}
  ".graph-key" {:fill "#ffd18f" :stroke "#09101e" :stroke-width 2 :cursor :pointer} ".graph-key.selected" {:fill "#ff6f87"}}})

(defn page []
  (html/html5 [:html {:lang "en"}
   [:head [:meta {:charset "utf-8"}] [:meta {:name "viewport" :content "width=device-width,initial-scale=1"}] [:link {:rel "icon" :href "data:,"}] [:title "Kami Animator"] [:style (css/css sheet)]]
   [:body
    [:header [:strong "KAMI ANIMATOR"] [:a {:href "https://kotoba-lang.github.io/kami-studio/"} "Studio"] [:span "WebGPU animation workspace"] [:span.spacer] [:button {:id "undo"} "Undo"] [:button {:id "redo"} "Redo"] [:button {:id "save-project"} "Save"] [:button {:id "load-project"} "Load"] [:button {:id "import"} "Import"] [:input {:id "import-file" :type "file" :accept ".edn,.kami-animator.edn" :style {:display "none"}}] [:button {:id "export"} "Export Project"]]
    [:section.workspace [:aside [:h2 "Outliner"] [:button "◆ Cube"] [:h2 "Skeleton"] [:button.primary {:id "bone-root"} "◇ Root"] [:button {:id "bone-spine"} "  ◇ Spine"] [:button {:id "bone-head"} "    ◇ Head"] [:h2 "Channels"]
                                [:button.primary {:id "channel-x"} "Location X"]
                                [:button {:id "channel-y"} "Location Y"] [:button {:id "channel-z"} "Location Z"]
                                [:button {:id "channel-rotation-x"} "Rotation X"] [:button {:id "channel-rotation-y"} "Rotation Y"] [:button {:id "channel-rotation-z"} "Rotation Z"]
                                [:button {:id "channel-scale-x"} "Scale X"] [:button {:id "channel-scale-y"} "Scale Y"] [:button {:id "channel-scale-z"} "Scale Z"]]
     [:main.viewport [:canvas {:id "gpu-canvas" :aria-label "Animated WebGPU viewport"}] [:div {:id "gpu-status"} "Initializing WebGPU…"]]
     [:aside [:h2 "Pose Mode"] [:strong {:id "pose-bone-name"} "Root"]
      [:label "Translate X" [:input {:id "pose-tx" :type "number" :step 0.1 :value 0}]] [:label "Translate Y" [:input {:id "pose-ty" :type "number" :step 0.1 :value 0}]] [:label "Translate Z" [:input {:id "pose-tz" :type "number" :step 0.1 :value 0}]]
      [:label "Rotate X" [:input {:id "pose-rx" :type "number" :step 0.05 :value 0}]] [:label "Rotate Y" [:input {:id "pose-ry" :type "number" :step 0.05 :value 0}]] [:label "Rotate Z" [:input {:id "pose-rz" :type "number" :step 0.05 :value 0}]] [:button.primary {:id "key-bone-pose"} "Key Bone Pose"]
      [:h2 "Two-Bone IK"] [:label "Target X" [:input {:id "ik-target-x" :type "number" :step 0.1 :value 1}]] [:label "Target Y" [:input {:id "ik-target-y" :type "number" :step 0.1 :value 1}]]
      [:label "Elbow / Pole" [:select {:id "ik-elbow"} [:option {:value "positive"} "Positive"] [:option {:value "negative"} "Negative"]]] [:button.primary {:id "solve-ik"} "Solve Root → Spine"] [:p {:id "ik-status" :role "status"} "Target reached"]
      [:h2 "Playback"] [:button.primary {:id "play"} "▶ Play"] [:button {:id "add-key"} "＋ Key at playhead"] [:button {:id "delete-key"} "Delete selected"]
      [:label [:input {:id "loop" :type "checkbox"}] "Loop range"]
      [:label "Loop start" [:input {:id "loop-start" :type "number" :min 0 :max 2 :step 0.1 :value 0}]]
      [:label "Loop end" [:input {:id "loop-end" :type "number" :min 0 :max 2 :step 0.1 :value 2}]]
      [:label "Playback rate" [:input {:id "playback-rate" :type "number" :min 0.1 :max 4 :step 0.1 :value 1}]]
      [:label "Frame rate" [:select {:id "fps"} [:option {:value 24} "24 fps"] [:option {:value 25} "25 fps"] [:option {:value 30} "30 fps"] [:option {:value 60} "60 fps"]]]
      [:label "Duration (s)" [:input {:id "duration" :type "number" :min 0.1 :step 0.1 :value 2}]]
      [:label [:input {:id "frame-snap" :type "checkbox" :checked true}] "Snap keys to frames"]
      [:h2 "Profile"] [:select {:id "profile"} [:option {:value "maya"} "Maya"] [:option {:value "blender"} "Blender"] [:option {:value "max"} "3ds Max"] [:option {:value "c4d"} "Cinema 4D"]] [:p {:id "profile-hint"} "S Set Key · Alt+V Play · Delete Key"]]]
    [:section.timeline [:div.tracks [:strong "Cube"] [:p {:id "active-channel"} "Location X"]]
     [:div [:div.lane {:id "lane"} [:div.playhead {:id "playhead"}]] [:svg {:id "curve-graph" :viewBox "0 0 800 160" :role "img" :aria-label "Active channel animation curve"}] [:input {:id "scrub" :type "range" :min 0 :max 2 :step 0.01 :value 0}] [:span {:id "time"} "0.00 / 2.00 s"]]
     [:div.inspector [:h2 "Keyframe"] [:div [:button {:id "copy-key"} "Copy"] [:button {:id "paste-key"} "Paste"] [:button {:id "duplicate-key"} "Duplicate +1 frame"]] [:label "Time" [:input {:id "key-time" :type "number" :step 0.1}]] [:label "Value" [:input {:id "key-value" :type "number" :step 0.1}]] [:label "Interpolation" [:select {:id "interpolation"} [:option {:value "linear"} "Linear"] [:option {:value "smooth"} "Smooth"] [:option {:value "hermite"} "Hermite"] [:option {:value "step"} "Step"]]] [:label "Tangent in" [:input {:id "tangent-in" :type "number" :step 0.1 :value 0}]] [:label "Tangent out" [:input {:id "tangent-out" :type "number" :step 0.1 :value 0}]] [:button {:id "auto-tangents"} "Auto tangents"]]]
    [:span {:id "debug-state" :style {:display "none"}}]
    [:script {:src "./js/app.js"}]]]))
