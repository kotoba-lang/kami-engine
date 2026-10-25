(ns kami.creative-studio.ui
  (:require [kotoba.lang.text :as str]
            [cljs.pprint]
            [reagent.core :as r]
            [reagent.dom :as rdom]
            [shadow.css :refer [css]]
            [kami.creative-studio.core :as core]
            [kami.creative-studio.media :as media]))

(def $body (css {:margin "0" :background "#080b12" :color "#f0f3f8" :overflow "hidden" :color-scheme "dark"
                 :font-family "Inter,ui-sans-serif,system-ui,-apple-system,Hiragino Sans,sans-serif"}))
(def $top (css {:height "54px" :display "grid" :grid-template-columns "1fr auto 1fr" :align-items "center"
                :padding "env(safe-area-inset-top,0px) 14px 0 14px"
                :border-bottom "1px solid #253044" :background "#111722e8" :backdrop-filter "blur(20px)" :z-index "5"}))
(def $brand (css {:font-weight "900" :letter-spacing ".08em" :color "#f0f3f8" :text-decoration "none"}))
(def $actions (css {:display "flex" :gap "8px" :align-items "center" :justify-content "flex-end"}))
(def $badge (css {:font-size "10px" :font-weight "800" :letter-spacing ".12em" :border "1px solid #33415a" :border-radius "20px" :padding "6px 9px"}))
(def $workspace (css {:display "grid" :grid-template-columns "240px minmax(420px,1fr) 330px" :grid-template-rows "minmax(0,1fr) 112px"
                      :height "calc(100vh - 54px)" :min-width "960px"}))
(def $aside (css {:padding "18px 14px" :border-right "1px solid #253044" :background "#0d131e" :overflow-y "auto"}))
(def $eyebrow (css {:font-family "ui-monospace,monospace" :font-size "10px" :font-weight "800" :letter-spacing ".18em" :color "#80ead0"}))
(def $title (css {:font-size "20px" :line-height "1.2" :margin "7px 0 8px"}))
(def $muted (css {:color "#8c98aa" :line-height "1.6"}))
(def $form (css {:display "grid" :gap "10px" :margin-top "16px"}))
(def $label (css {:display "grid" :gap "6px" :font-size "12px" :font-weight "700" :color "#b9c1ce"}))
(def $input (css {:width "100%" :box-sizing "border-box" :border "1px solid #253044" :background "#0b1019" :color "#f0f3f8"
                  :border-radius "8px" :padding "11px 12px" :font "inherit"}
                 ["&:focus" {:outline "1px solid #80ead0" :border-color "#80ead0"}]))
(def $button (css {:border "1px solid #33415a" :background "#131b28" :color "#f0f3f8" :border-radius "10px" :padding "8px 11px" :min-height "44px"
                   :font-weight "800" :cursor "pointer"}
                  ["&:hover" {:border-color "#80ead0"}] ["&:disabled" {:opacity "0.35" :cursor "not-allowed"}]
                  ["&:focus-visible" {:outline "2px solid #80ead0" :outline-offset "2px"}]))
(def $primary (css {:background "#80ead0" :border-color "#80ead0" :color "#07110e"}))
(def $main (css {:min-width "0" :padding "14px" :background "#080d15" :overflow "hidden"}))
(def $head (css {:display "flex" :justify-content "space-between" :align-items "end" :gap "12px" :margin-bottom "24px"}))
(def $preview (css {:height "100%" :display "grid" :grid-template-rows "52px minmax(0,1fr) auto auto" :border "1px solid #253044" :border-radius "16px" :overflow "hidden" :background "#0d141f"}))
(def $preview-head (css {:display "flex" :justify-content "space-between" :align-items "center" :padding "14px 18px" :border-bottom "1px solid #253044"}))
(def $viewer (css {:width "100%" :height "100%" :min-height "300px" :background "radial-gradient(circle at 50% 42%,#263247,#101621 50%,#090d14)"}))
(def $empty (css {:height "100%" :display "grid" :place-content "center" :text-align "center" :gap "6px" :color "#8c98aa"}))
(def $preview-actions (css {:display "flex" :gap "7px" :align-items "center" :padding "9px 12px" :overflow-x "auto"}
                           ["& label" {:min-width "210px"}]))
(def $file (css {:border "1px solid #33415a" :border-radius "8px" :padding "11px 14px" :cursor "pointer" :position "relative" :overflow "hidden"}
                ["&:focus-within" {:outline "2px solid #80ead0" :outline-offset "2px"}]))
(def $hidden-file (css {:position "absolute" :opacity "0" :width "1px" :height "1px"}))
(def $progress (css {:height "26px" :background "#090d15" :position "relative" :border-top "1px solid #253044"}))
(def $progress-bar (css {:height "100%" :background "linear-gradient(90deg,#375f78,#80ead0)" :transition "width .25s"}))
(def $progress-label (css {:position "absolute" :inset "0" :display "grid" :place-content "center" :font "800 10px ui-monospace"}))
(def $tabs (css {:display "flex" :gap "6px" :padding "10px 12px" :border-bottom "1px solid #253044"}))
(def $panel (css {:height "100%" :box-sizing "border-box" :padding "16px" :background "#0d141f" :overflow-y "auto"}))
(def $grid (css {:display "grid" :grid-template-columns "repeat(2,minmax(0,1fr))" :gap "12px"}
                 ["@media(max-width:650px)" {:grid-template-columns "1fr"}]))
(def $ops (css {:color "#8c98aa" :font "11px/1.7 ui-monospace" :padding-left "22px"}))
(def $catalog-head (css {:display "flex" :align-items "center" :justify-content "space-between" :gap "10px" :margin "20px 0 10px"}))
(def $trait-row (css {:display "grid" :grid-template-columns "62px 1fr" :gap "8px" :align-items "start" :margin-bottom "8px"}
                     ["@media(max-width:650px)" {:grid-template-columns "1fr"}]))
(def $trait-label (css {:padding-top "10px" :font "800 10px ui-monospace" :letter-spacing ".12em" :color "#80ead0"}))
(def $trait-options (css {:display "grid" :grid-template-columns "repeat(2,minmax(0,1fr))" :gap "6px"}
                         ["@media(max-width:650px)" {:grid-template-columns "1fr"}]))
(def $trait-card (css {:border "1px solid #253044" :background "#0a1019" :color "#b9c1ce" :border-radius "9px"
                       :padding "10px" :cursor "pointer" :text-align "left" :font "inherit"}
                      ["&:hover" {:border-color "#80ead0"}]
                      ["&:focus-visible" {:outline "2px solid #80ead0" :outline-offset "2px"}]))
(def $trait-selected (css {:border-color "#80ead0" :background "#142a2a" :color "#f0f3f8"}))
(def $pipeline (css {:display "grid" :grid-template-columns "repeat(4,1fr)" :gap "8px"}
                     ["@media(max-width:700px)" {:grid-template-columns "1fr 1fr"}]))
(def $stage (css {:border "1px solid #253044" :border-radius "10px" :padding "9px" :background "#111824"}))
(def $stage-name (css {:font-size "10px" :color "#80ead0" :font-weight "800" :letter-spacing ".12em"}))
(def $runbar (css {:position "absolute" :left "0" :right "0" :bottom "0" :min-height "72px" :border-top "1px solid #253044" :background "#0b1019"
                  :display "flex" :align-items "center" :justify-content "space-between" :gap "12px"
                  :padding "12px clamp(18px,4vw,52px) max(12px,env(safe-area-inset-bottom)) clamp(18px,4vw,52px)"}))
(def $toast (css {:position "fixed" :right "20px" :bottom "92px" :background "#172131" :border "1px solid #33415a" :padding "11px 15px" :border-radius "8px" :z-index "10"}))
(def $runtime (css {:display "flex" :flex-wrap "wrap" :gap "7px" :padding "0 18px 14px" :color "#8c98aa" :font "10px ui-monospace"}))
(def $source-link (css {:color "#80ead0" :text-decoration "none"}))
(def $inspector (css {:grid-column "3" :grid-row "1" :border-left "1px solid #253044" :min-width "0" :overflow "hidden" :display "grid" :grid-template-rows "auto minmax(0,1fr)"}))
(def $timeline (css {:grid-column "1 / 4" :grid-row "2" :border-top "1px solid #253044" :background "#101722" :padding "10px 14px" :display "grid" :grid-template-columns "180px 1fr auto" :gap "12px" :align-items "center"}))
(def $lanes (css {:display "grid" :gap "5px"}))
(def $lane (css {:height "26px" :display "grid" :grid-template-columns "58px 1fr" :align-items "center" :gap "7px"}))
(def $lane-name (css {:font "800 9px ui-monospace" :color "#8c98aa"}))
(def $lane-rail (css {:height "20px" :position "relative" :border-radius "4px" :background "#080d15" :overflow "hidden"}))
(def $clip (css {:position "absolute" :inset "2px" :border-radius "3px" :background "linear-gradient(90deg,#2d6d68,#80ead0)" :color "#07110e" :padding "2px 7px" :font "800 9px ui-monospace" :white-space "nowrap" :overflow "hidden"}))
(def $audio-clip (css {:background "linear-gradient(90deg,#61458a,#b9a0ec)"}))
(def $toolbar-title (css {:font-size "13px" :font-weight "700" :text-align "center"}))

(def official-vrm-url
  "https://raw.githubusercontent.com/pixiv/three-vrm/release/packages/three-vrm/examples/models/VRM1_Constraint_Twist_Sample.vrm")
(def seed-vrm-url
  "https://raw.githubusercontent.com/vrm-c/vrm-specification/master/samples/Seed-san/vrm/Seed-san.vrm")

(defonce state
  (r/atom {:name "Untitled character" :brief "" :reference "" :endpoint "" :api-key "" :artifact-url ""
           :motion-preset :dance :duration 4 :edits [] :tab :editor :status :ready :progress 0
           :manifest nil :media-project nil :playhead-frame 0 :playing? false
           :toast nil :object-url nil :selection {} :random-seed 1
           :composition-state :idle :composition-error nil :composition-plan nil}))

(def trait-catalog
  {:body [{:id "seed-san" :label "Seed-san" :source seed-vrm-url}
          {:id "constraint" :label "Constraint" :source official-vrm-url}]
   :hair [{:id "seed-hair" :label "Seed Hair" :source seed-vrm-url}
          {:id "constraint-hair" :label "Brown Hair" :source official-vrm-url}]
   :face [{:id "seed-face" :label "Seed Face" :source seed-vrm-url}
          {:id "constraint-face" :label "Soft Face" :source official-vrm-url}]
   :outfit [{:id "seed-outfit" :label "Seed Suit" :source seed-vrm-url}
            {:id "constraint-outfit" :label "Casual" :source official-vrm-url}]
   :accessory [{:id "none" :label "None" :source nil}]})

(defn setv! [k e] (swap! state assoc k (.. e -target -value)))
(defn notify! [s] (swap! state assoc :toast s) (js/setTimeout #(swap! state assoc :toast nil) 2200))
(defonce transport-timer (atom nil))

(defn stop-transport! []
  (when-let [timer @transport-timer] (js/clearInterval timer))
  (reset! transport-timer nil)
  (swap! state assoc :playing? false))

(defn toggle-transport! []
  (if (:playing? @state)
    (stop-transport!)
    (let [total (max 1 (* 30 (:duration @state)))]
      (swap! state assoc :playing? true)
      (reset! transport-timer
              (js/setInterval
               #(swap! state update :playhead-frame
                       (fn [frame]
                         (let [next-frame (inc (or frame 0))]
                           (if (< next-frame total) next-frame 0))))
               (/ 1000 30))))))

(defn manifest! []
  (let [s @state]
    (if (str/blank? (:brief s))
      (notify! "Creative briefを入力してください")
      (let [m (core/project-manifest
               {:id (str "project-" (.toString (js/Date.now) 36)) :name (:name s) :brief (:brief s)
                :reference (:reference s) :motion-preset (:motion-preset s) :duration (:duration s)
                :edits (:edits s) :created-at (.toISOString (js/Date.))})]
        (swap! state assoc :manifest m :status :planned)
        (notify! "制作planを更新しました") m))))

(defn media-project! []
  (let [{:keys [name duration artifact-url]} @state
        frames (* 30 duration)
        ticks (* 960 duration)
        picture-id "picture"
        music-id "music"
        p (media/project
           {:id (str "media-" (.toString (js/Date.now) 36)) :name name
            :video {:timeline/timebase {:num 30 :den 1 :drop-frame? false}
                    :timeline/tracks
                    [{:track/id :v1 :track/type :video :track/transitions [] :track/effect-stack []
                      :track/enabled? true :track/muted? false :track/locked? false
                      :track/clips [{:clip/id :picture :clip/source-id picture-id
                                     :clip/source-in 0 :clip/source-out frames
                                     :clip/timeline-start 0 :clip/duration frames :clip/effect-stack []}]}]
                    :timeline/markers []}
            :music {:project/ppq 480 :project/sample-rate 48000
                    :project/tempo-map [{:tempo-point/tick 0 :tempo-point/bpm 120
                                         :tempo-point/time-sig [4 4]}]
                    :project/tracks [{:track/id "music" :track/type :audio :track/name "Music"
                                      :track/mute? false :track/solo? false :track/armed? false
                                      :track/output-bus "master"}]
                    :project/buses [{:bus/id "master" :bus/name "Master"
                                     :bus/inputs #{"music"} :bus/plugin-chain []}]
                    :project/clips [{:clip/id "theme" :clip/track-id "music"
                                     :clip/start-tick 0 :clip/length-ticks ticks
                                     :clip/content {:audio/uri music-id}}]
                    :project/automation []}
            :assets [{:asset/id picture-id :asset/kind :video
                      :asset/path (if (str/blank? artifact-url) "cid:pending-picture" artifact-url)}
                     {:asset/id music-id :asset/kind :audio :asset/path "cid:pending-music"}]
            :output {:width 1920 :height 1080 :fps 30}})]
    (swap! state assoc :media-project p :playhead-frame 0 :tab :composer)
    (notify! (if (media/valid-project? p) "映像・音楽projectを同期しました" "project validationに失敗しました"))
    p))

(defn load-artifact! [url]
  (when (seq url)
    (swap! state assoc :artifact-url url :progress 100 :status :preview)
    (notify! "3D artifactをpreviewへ読み込みました")))

(defn parse-json [response]
  (-> (.text response)
      (.then (fn [text]
               (try (js->clj (js/JSON.parse text) :keywordize-keys true)
                    (catch :default _ {:message text}))))))

(defn request-headers []
  (cond-> {"content-type" "application/json"}
    (not (str/blank? (:api-key @state))) (assoc "x-api-key" (:api-key @state))))

(declare submit-compose!)
(defn compose-selection! [selection]
  (try
    (let [{:keys [plan]} (core/composition-context selection)
          artifact (core/preview-artifact selection {:constraint-url official-vrm-url
                                                      :seed-url seed-vrm-url})]
      (swap! state assoc :composition-plan plan :composition-error nil)
      (if artifact
        (do (swap! state assoc :composition-state :ready :status :loading :progress 90)
            (load-artifact! artifact)
            (notify! "build生成済み実VRM compositionを表示しました"))
        (do (swap! state assoc :composition-state :remote-required :status :planned)
            (if (str/blank? (:endpoint @state))
              (notify! "複数part compositionにはMurakumo endpointが必要です")
              (submit-compose! selection)))))
    (catch :default e
      (swap! state assoc :composition-state :failed :composition-error (.-message e))
      (notify! (str "capability plan失敗: " (.-message e))))))

(defn load-sample! []
  (-> (js/fetch "samples/kami-sample.project.json")
      (.then parse-json)
      (.then (fn [project]
               (let [selection {:body (second (:body trait-catalog))
                                :hair (second (:hair trait-catalog))
                                :face (second (:face trait-catalog))
                                :outfit (second (:outfit trait-catalog))
                                :accessory (first (:accessory trait-catalog))}
                     edits (vec (concat (get-in project [:character :operations])
                                        (core/selection-operations selection)))
                     project (assoc-in project [:character :operations] edits)]
                 (swap! state assoc
                        :name (:name project)
                        :brief (:brief project)
                        :manifest project
                        :selection selection
                        :edits edits
                        :status :sample))
               (load-artifact! (core/artifact-url project))
               (notify! "sample projectを生成物から読み込みました")))
      (.catch (fn [e]
                (swap! state assoc :status :failed)
                (notify! (str "sample読込失敗: " (.-message e)))))))

(defn load-official-vrm! []
  (swap! state assoc :name "VRM1 Constraint Character" :status :loading :progress 15)
  (load-artifact! official-vrm-url)
  (notify! "pixiv公式VRM sampleをsource URLから読み込みました"))

(declare poll-job!)
(defn handle-job-response! [body]
  (when-let [url (core/artifact-url body)] (load-artifact! url))
  (swap! state assoc :progress (core/progress body))
  (when-let [url (core/status-url body)] (poll-job! url)))

(defn poll-job! [url]
  (js/setTimeout
   #(-> (js/fetch url #js {:headers (clj->js (request-headers))})
        (.then parse-json)
        (.then (fn [body]
                 (handle-job-response! body)
                 (let [status (keyword (or (:status body) (get-in body [:job :status]) "running"))]
                   (swap! state assoc :status status)
                   (when-not (contains? #{:done :failed :cancelled} status) (poll-job! url)))))
        (.catch (fn [e] (swap! state assoc :status :failed) (notify! (str "進捗取得失敗: " (.-message e))))))
   1500))

(defn post-job! [payload success-message]
  (swap! state assoc :status :submitting :progress 1)
  (-> (js/fetch (:endpoint @state)
                #js {:method "POST" :headers (clj->js (request-headers))
                     :body (js/JSON.stringify (clj->js payload))})
      (.then (fn [response]
               (if (.-ok response) (parse-json response)
                   (throw (js/Error. (str "HTTP " (.-status response)))))))
      (.then (fn [body]
               (swap! state assoc :status :queued :composition-state :queued)
               (handle-job-response! body)
               (notify! success-message)))
      (.catch (fn [e]
                (swap! state assoc :status :failed :composition-state :failed
                       :composition-error (.-message e))
                (notify! (str "送信失敗: " (.-message e)))))))

(defn submit-compose! [selection]
  (post-job! (core/murakumo-compose-request selection)
             "VRM compositionをMurakumoへ送信しました"))

(defn submit! []
  (when-let [m (or (:manifest @state) (manifest!))]
    (if (str/blank? (:endpoint @state))
      (notify! "Murakumo endpointを入力してください")
      (if (= :remote-required (:composition-state @state))
        (submit-compose! (:selection @state))
        (post-job! m "Murakumoへ送信しました")))))

(defn file-change! [e]
  (when-let [file (aget (.. e -target -files) 0)]
    (when-let [old (:object-url @state)] (js/URL.revokeObjectURL old))
    (let [url (js/URL.createObjectURL file)]
      (swap! state assoc :object-url url) (load-artifact! url))))

(defn add-part! []
  (let [{:keys [part-kind part-source]} @state]
    (if (str/blank? (or part-source ""))
      (notify! "Donor URL / CIDを入力してください")
      (do (swap! state update :edits conj {:op/type "part/set" :part/kind (name (or part-kind :hair)) :part/source part-source})
          (manifest!) (notify! "kisekae part操作を追加しました")))))

(defn material! [e]
  (let [color (.. e -target -value)]
    (swap! state assoc :material-color color)
    (swap! state update :edits conj {:op/type "material/base-color" :value color})))

(defn apply-selection! [selection]
  (let [non-trait-ops (remove #(= "part/set" (:op/type %)) (:edits @state))]
    (swap! state assoc :selection selection
           :edits (vec (concat non-trait-ops (core/selection-operations selection))))
    (manifest!)
    (compose-selection! selection)))

(defn choose-trait! [slot asset]
  (apply-selection! (core/select-trait (:selection @state) (assoc asset :slot slot)))
  (notify! (str (:label asset) " を " (name slot) " に選択しました")))

(defn randomize! []
  (let [seed (inc (:random-seed @state))]
    (swap! state assoc :random-seed seed)
    (apply-selection! (core/seeded-selection trait-catalog seed))
    (notify! (str "seed " seed " でcharacterを構成しました"))))

(defn field [label child] [:label {:class $label} label child])
(defn button [label on-click & [primary? disabled?]]
  [:button {:class (str $button (when primary? (str " " $primary))) :on-click on-click :disabled disabled? :type "button"} label])

(defn preview []
  (let [{:keys [artifact-url progress status]} @state]
    [:section {:class $preview}
     [:div {:class $preview-head}
      [:div [:div {:class $eyebrow} "REALTIME PREVIEW"] [:h3 "VRM / GLB Viewer"]]
      [:span {:class $badge} (str/upper (name status))]]
     [:model-viewer {:id "modelViewer" :class $viewer :src (when (seq artifact-url) artifact-url)
                     :camera-controls true :autoplay true :shadow-intensity "1" :environment-image "neutral"
                     :on-load #(swap! state assoc :status :preview :progress 100)}
      [:div {:slot "poster" :class $empty} [:strong "3D artifactを読み込む"] [:span "生成完了後は自動表示されます"]]]
     [:div {:class $preview-actions}
      [:label {:class $file} "VRM / GLBを開く" [:input {:class $hidden-file :type "file" :accept ".vrm,.glb,.gltf" :on-change file-change!}]]
      (button "Sampleを読込" load-sample!)
      (button "実VRM Character" load-official-vrm!)
      (field "Artifact URL" [:input {:class $input :type "url" :value artifact-url :placeholder "https://…/character.vrm" :on-change #(setv! :artifact-url %)}])
      (button "表示" #(load-artifact! (:artifact-url @state)))]
     [:div {:class $runtime}
      [:span "VIEWER: model-viewer / glTF+VRM geometry"]
      [:span "·"]
      [:span (if (some? (.-gpu js/navigator)) "WEBGPU: AVAILABLE (direct CLJS executor opt-in)" "WEBGPU: UNAVAILABLE")]
      [:span "·"]
      [:span "WASM: kotoba guest logic only"]
      [:span "·"]
      [:a {:class $source-link :href "https://github.com/pixiv/three-vrm" :target "_blank" :rel "noreferrer"}
       "VRM sample © pixiv Inc. / redistribution allowed"]]
     (when (and (pos? progress) (< progress 100))
       [:div {:class $progress} [:i {:class $progress-bar :style {:width (str progress "%")}}] [:span {:class $progress-label} (str progress "%")]])]))

(defn editor []
  (let [{:keys [edits]} @state]
    [:section {:class $panel}
     [:div {:class $eyebrow} "VRM CHARACTER EDITOR · kotoba-lang/kisekae"]
     [:h3 "Character composition"]
     [:div {:class $catalog-head}
      [:div [:strong "Real VRM asset pack"]
       [:div {:class $muted} (case (:composition-state @state)
                               :loading "Fetching · parsing · skin rebinding…"
                               :ready "Composed artifact is visible"
                               :remote-required "Multiple parts · Murakumo build required"
                               :failed (str "Failed: " (:composition-error @state))
                               "Select a part to compose")]]
      (button (str "Randomize · " (:random-seed @state)) randomize!)]
     (for [slot core/trait-order]
       ^{:key slot}
       [:div {:class $trait-row}
        [:div {:class $trait-label} (str/upper (name slot))]
        [:div {:class $trait-options}
         (for [asset (get trait-catalog slot)]
           ^{:key (:id asset)}
           [:button {:type "button"
                     :class (str $trait-card (when (= (:id asset) (get-in @state [:selection slot :id]))
                                               (str " " $trait-selected)))
                     :on-click #(choose-trait! slot asset)}
            [:strong (:label asset)]
            [:div {:class $muted} (if (:source asset) "VRM 1.0 · upstream" "No mesh")]])]])
     [:div {:class $grid}
      (field "Part" [:select {:class $input :value (name (or (:part-kind @state) :hair)) :on-change #(swap! state assoc :part-kind (keyword (.. % -target -value)))}
                     (for [v ["hair" "face" "outfit" "accessory"]] ^{:key v} [:option {:value v} (str/capitalize v)])])
      (field "Donor URL / CID" [:input {:class $input :value (or (:part-source @state) "") :placeholder "bafy… / https://…" :on-change #(setv! :part-source %)}])
      (button "パーツ操作を追加" add-part!)
      (field "Material color" [:input {:class $input :type "color" :value (or (:material-color @state) "#ffffff") :on-change material!}])
      (field "Expression" [:select {:class $input :value (or (:expression @state) "happy") :on-change #(setv! :expression %)}
                           (for [v ["happy" "angry" "sad" "relaxed" "surprised"]] ^{:key v} [:option {:value v} (str/capitalize v)])])
      (field "Expression weight" [:input {:class $input :type "range" :min 0 :max 1 :step 0.01 :value (or (:expression-weight @state) 0)
                                         :on-change #(swap! state assoc :expression-weight (js/parseFloat (.. % -target -value)))}])
      (field "Motion" [:select {:class $input :value (name (:motion-preset @state)) :on-change #(swap! state assoc :motion-preset (keyword (.. % -target -value)))}
                       (for [v ["idle" "walk" "dance" "gesture"]] ^{:key v} [:option {:value v} (str/capitalize v)])])
      (field "Duration" [:input {:class $input :type "range" :min 1 :max 30 :value (:duration @state) :on-change #(swap! state assoc :duration (js/parseInt (.. % -target -value))) }])]
     [:ol {:class $ops}
      (if (seq edits)
        (map-indexed (fn [i op] ^{:key i} [:li (pr-str op)]) edits)
        [:li "編集操作はまだありません"])] ]))

(defn pipeline []
  [:div {:class $pipeline}
   (for [[id label detail] [[:model "MODEL" "TRELLIS"] [:rig "RIG" "UniRig"] [:motion "MOTION" "EDN retarget"] [:music "MUSIC" "ACE-Step"]]]
     ^{:key id} [:div {:class $stage} [:div {:class $stage-name} label] [:strong detail]])])

(defn composer-panel []
  (let [p (:media-project @state)
        problems (when p (media/validate-project p))]
    [:section {:class $panel}
     [:div {:class $eyebrow} "MEDIA PROJECT · VIDEO + MUSIC"]
     [:h3 "Unified composition"]
     [:p {:class $muted} "映像はframe、音楽はtickのnative単位を保ち、transportだけを同期します。"]
     (button "現在の素材から同期" media-project! true)
     (when p
       [:div
        [:p [:strong (if (empty? problems) "VALID" (str (count problems) " PROBLEMS"))]
         " · " (media/video-duration-seconds p) "s video · " (media/music-duration-seconds p) "s music"]
        [:div {:class $form}
         (field "Playhead"
                [:input {:class $input :type "range" :min 0
                         :max (max 0 (dec (* 30 (:duration @state))))
                         :value (:playhead-frame @state)
                         :on-change #(swap! state assoc :playhead-frame
                                            (js/parseInt (.. % -target -value)))}])
         [:div {:class $actions}
          (button "−1 frame" #(swap! state update :playhead-frame (fn [n] (max 0 (dec n)))))
          (button "Split V1" #(let [frame (:playhead-frame @state)]
                                 (swap! state update :media-project
                                        media/split-video-clip :picture frame
                                        (keyword (str "picture-" frame)))))
          (button "+1 frame" #(swap! state update :playhead-frame
                                     (fn [n] (min (dec (* 30 (:duration @state))) (inc n)))))] ]
        [:pre (with-out-str (cljs.pprint/pprint p))]])]))

(defn timeline-lanes []
  (let [p (:media-project @state)
        frame (:playhead-frame @state)
        transport (when p (media/playhead p frame))]
  [:div {:class $lanes}
   [:div {:class $lane} [:span {:class $lane-name} "V1 VIDEO"]
    [:div {:class $lane-rail}
     [:span {:class $clip} (or (:name @state) "Picture")]
     [:i {:style {:position "absolute" :top 0 :bottom 0
                  :left (str (* 100 (/ frame (max 1 (* 30 (:duration @state))))) "%")
                  :width "2px" :background "#fff" :z-index 2}}]]]
   [:div {:class $lane} [:span {:class $lane-name} "A1 MUSIC"]
    [:div {:class $lane-rail} [:span {:class (str $clip " " $audio-clip)}
                                     (if transport
                                       (str (:transport/timecode transport) " · tick " (:transport/tick transport))
                                       "Theme · 120 BPM")]]]]))

(defn app []
  (let [{:keys [name brief reference endpoint api-key tab status toast]} @state]
    [:div
     [:header {:class $top}
      [:a {:class $brand :href "#"} "神 KAMI"]
      [:div {:class $toolbar-title} (str name " · Creative Workspace")]
      [:div {:class $actions} [:span {:class $badge} (str/upper (clojure.core/name status))] (button "生成" submit! true (str/blank? endpoint))]]
     [:main {:class $workspace}
      [:aside {:class $aside} [:div {:class $eyebrow} "LIBRARY · PROJECT"] [:h1 {:class $title} "Character Project"]
       [:p {:class $muted} "Project、asset、生成sourceを一つのworkspaceで管理します。"]
       [:div {:class $form}
        (field "プロジェクト名" [:input {:class $input :value name :on-change #(setv! :name %)}])
        (field "Creative brief" [:textarea {:class $input :rows 3 :value brief :on-change #(setv! :brief %)}])
        (field "参照画像 / CID" [:input {:class $input :value reference :on-change #(setv! :reference %)}])
        (field "Murakumo endpoint" [:input {:class $input :type "url" :value endpoint :placeholder "https://…/v1/generation/vrm-compose" :on-change #(setv! :endpoint %)}])
        (field "Murakumo API key" [:input {:class $input :type "password" :value api-key
                                            :autocomplete "off" :placeholder "session only"
                                            :on-change #(setv! :api-key %)}])
        (button "Planを更新" manifest! true)
        (button "Video + Musicを同期" media-project!)
        (button "Sample Project" load-sample!)
        [:div {:class $eyebrow} "RUNTIME"]
        [:div {:class $muted} "CLJS host · WebGPU direct\nKotoba Wasm · guest logic"]]]
      [:section {:class $main}
       [preview]]
      [:aside {:class $inspector}
       [:div {:class $tabs}
        (button "Inspector" #(swap! state assoc :tab :editor) (= tab :editor))
        (button "Composer" #(swap! state assoc :tab :composer) (= tab :composer))
        (button "Manifest" #(swap! state assoc :tab :manifest) (= tab :manifest))]
       (case tab
         :editor [editor]
         :composer [composer-panel]
         [:section {:class $panel} [:div {:class $eyebrow} "PROJECT MANIFEST"] [:pre (with-out-str (cljs.pprint/pprint (:manifest @state)))]] )]
      [:footer {:class $timeline}
       [:div [:div {:class $eyebrow} "TIMELINE"] [:strong (str/capitalize (clojure.core/name (:motion-preset @state))) " · " (:duration @state) "s"]]
       [timeline-lanes]
       [:div {:class $actions}
        (button "◀" #(swap! state update :playhead-frame (fn [n] (max 0 (dec n)))))
        (button (if (:playing? @state) "❚❚" "▶") toggle-transport! true)
        (button "●" #(notify! "record armed"))]]]
     (when toast [:div {:class $toast} toast])]))

(defn ^:export init! []
  (set! (.-className js/document.body) $body)
  (rdom/render [app] (.getElementById js/document "app"))
  (load-sample!)
  (js/setTimeout load-official-vrm! 500))
