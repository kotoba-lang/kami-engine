(ns kami.bim-editor.app (:require [cljs.reader :as reader] [kotoba.lang.text :as string] [bim]
                                  [bim.mep :as mep]
                                  [bim.cloud :as cloud]
                                  [bim.editor :as editor]
                                  [bim.integration :as family]
                                  [bim.drawing :as drawing] [ifc.core :as ifc]
                                  [kotoba.issue.bcf :as bcf]
                                  [kami.bim-editor.integration :as integration]
                                  [kami.bim-editor.family-editor :as family-editor]
                                  [kami.bim-editor.interaction :as interaction]
                                  [kami.bim-editor.project :as project] [kami.webgpu.mesh :as gpu]))
(defn wall [id a b] (bim/wall {:id id :name (str "Wall " id) :start a :end b :thickness 0.25 :height 3.2 :material "Concrete"}))
(defn initial-project [] (let [st (bim/storey {:id 3 :name "Ground Floor" :elevation 0 :height 3.2 :placement :identity :spaces [] :elements []}) p (-> (bim/project "Lodge") (update :sites conj (bim/site {:id 1 :name "Site" :geo nil :placement :identity :buildings [(bim/building {:id 2 :name "Lodge" :placement :identity :reference-elevation 0 :storeys [st]})]})))] (reduce #(bim/add-element %1 3 %2) p [(wall 10 [0 0 0] [8 0 0]) (wall 11 [8 0 0] [8 6 0]) (wall 12 [8 6 0] [0 6 0]) (wall 13 [0 6 0] [0 0 0])])))
(defn initial-family-catalog []
  (family/family-catalog
   [(family/family-definition
     {:id "casework" :name "Casework" :category :furniture
      :parameters {:width {:type :length :scope :type :default 0.8 :min 0.3}
                   :depth {:type :length :scope :instance :default 0.6 :min 0.2}
                   :height {:type :length :scope :instance :default 0.9 :min 0.2 :max 3.0}
                   :finish {:type :material :scope :instance :default "Oak"}}
      :formulas {:volume [:* [:param :width] [:param :depth] [:param :height]]}
      :types {:standard {:id "casework-standard" :name "Standard" :parameters {:width 0.8}}
              :wide {:id "casework-wide" :name "Wide" :parameters {:width 1.2}}}
      :template {:kind :furniture :name "Casework" :global-id nil
                 :material [:material-param :finish]
                 :placement {:location [0 0 0]}
                 :geometry {:kind :extruded-area-solid
                            :profile {:kind :rectangle :x-dim [:param :width]
                                      :y-dim [:param :depth]}
                            :position {:location [0 0 0]} :direction [0 0 1]
                            :depth [:param :height]}
                 :quantities {:gross-volume-m3 [:param :volume]}
                 :psets {} :openings [] :connected-to []}})
    (family/annotation-family-definition
     {:id "wall-tag" :name "Wall Tag" :target-categories [:wall]
      :default-anchor :midpoint :label-bindings {:label [:name]}
      :parameters {:label {:type :text :scope :instance :default "Wall"}
                   :prefix {:type :text :scope :type :default "W"}}
      :types {:standard {:id "wall-tag-standard" :name "Standard"
                         :parameters {:prefix "W"}}}
      :template {:kind :tag :text [:param :label] :prefix [:param :prefix]}})]))
(defonce state (atom {:project (initial-project) :active-storey 3 :selected 10 :selection #{10}
                      :next-id 14 :next-storey-id 4
                      :selected-space nil :next-space-id 1000
                      :selected-clash nil :family-catalog (initial-family-catalog)
                      :structural-model nil :structural-overlay nil
                      :mep-systems [] :electrical-distribution nil
                      :review-topics [] :cloud-state nil
                      :cloud-workspace nil :cloud-checkpoint nil :cloud-sync-ack nil
                      :stream-settings {:cell-size 10.0 :batch-size 512
                                        :max-resident 10000 :max-loads 1000}
                      :stream-view nil
                      :history [] :future [] :azimuth 0.75 :elevation 0.5 :last-snap nil
                      :camera-target [4.0 1.5 3.0] :camera-distance 14.0
                      :profile :revit :shortcut-buffer "" :project-id "untitled-bim" :project-name "Untitled BIM"
                      :drawing-views {3 (family/drawing-view
                                         {:id "plan-3" :kind :floor-plan :name "Ground Floor"
                                          :view-range {:top 3.2 :cut-plane 1.2
                                                       :bottom 0 :view-depth -1}})}
                      :print-setting (family/print-setting
                                      {:id :default :name "Default" :paper-size :a1
                                       :orientation :landscape :scale 100})
                      :drawing-set {}
                      :selected-annotation nil :next-annotation-id 1
                      :revision 0 :save-status :clean}))
(defonce viewport (atom nil))
(declare refresh! refresh-drawing-annotations! workspace-status!)
(defn- storeys [] (:storeys (bim/find-building (:project @state) 2)))
(defn- elements [] (:elements (bim/find-storey (:project @state) (:active-storey @state))))
(defn- all-elements [] (mapcat :elements (storeys)))
(defn- spaces [] (:spaces (bim/find-storey (:project @state) (:active-storey @state))))
(defn- all-spaces [] (mapcat :spaces (storeys)))
(defn- selection-ids [] (set (:selection @state)))
(defn- selected-elements [] (filterv #(contains? (selection-ids) (:id %)) (all-elements)))
(defn- selected [] (first (filter #(= (:selected @state) (:id %)) (all-elements))))
(defn- select-only! [element-id]
  (swap! state assoc :selected element-id :selection (if element-id #{element-id} #{})))
(defn- apply-selection! [result]
  (swap! state assoc :selected (:primary result) :selection (:selection result)))
(defn- selection-mode [event]
  (cond (or (.-metaKey event) (.-ctrlKey event)) :toggle
        (.-shiftKey event) :add
        :else :replace))
(defn- selected-family []
  (get-in @state [:family-catalog :family-catalog/families
                  (.-value (.getElementById js/document "family-definition"))]))
(defn- refresh-family-definitions! []
  (let [select (.getElementById js/document "family-definition")
        selected (.-value select)
        families (->> (get-in @state [:family-catalog :family-catalog/families])
                      (remove (fn [[_ definition]] (= :annotation (:family/domain definition))))
                      (into {}))]
    (set! (.-innerHTML select) "")
    (doseq [[family-id definition] (sort-by key families)]
      (let [option (.createElement js/document "option")]
        (set! (.-value option) family-id)
        (set! (.-textContent option) (:family/name definition))
        (.appendChild select option)))
    (when (contains? families selected) (set! (.-value select) selected))))
(defn- refresh-annotation-families! []
  (let [select (.getElementById js/document "annotation-family")]
    (set! (.-innerHTML select) "")
    (doseq [[family-id definition]
            (sort-by key (filter (fn [[_ definition]]
                                   (= :annotation (:family/domain definition)))
                                 (get-in @state [:family-catalog :family-catalog/families])))]
      (let [option (.createElement js/document "option")]
        (set! (.-value option) family-id)
        (set! (.-textContent option) (:family/name definition))
        (.appendChild select option)))))
(defn- load-family-form! []
  (when-let [definition (selected-family)]
    (let [parameter #(get-in definition [:family/parameters % :default])]
      (set! (.-value (.getElementById js/document "family-id")) (:family/id definition))
      (set! (.-value (.getElementById js/document "family-name")) (:family/name definition))
      (set! (.-value (.getElementById js/document "family-category")) (name (:family/category definition)))
      (set! (.-value (.getElementById js/document "family-width")) (parameter :width))
      (set! (.-value (.getElementById js/document "family-depth")) (parameter :depth))
      (set! (.-value (.getElementById js/document "family-height")) (parameter :height))
      (set! (.-value (.getElementById js/document "family-material")) (parameter :finish))
      (set! (.-checked (.getElementById js/document "family-shared")) (:family/shared? definition))
      (set! (.-value (.getElementById js/document "family-parameters"))
            (pr-str (or (:family/parameters definition) {})))
      (set! (.-value (.getElementById js/document "family-lookup-tables"))
            (pr-str (or (:family/lookup-tables definition) {})))
      (set! (.-value (.getElementById js/document "family-formulas"))
            (pr-str (or (:family/formulas definition) {})))
      (set! (.-value (.getElementById js/document "family-reference-planes"))
            (pr-str (or (:family/reference-planes definition) {})))
      (set! (.-value (.getElementById js/document "family-constraints"))
            (pr-str (or (:family/constraints definition) [])))
      (set! (.-value (.getElementById js/document "family-sketches"))
            (pr-str (or (:family/sketches definition) {})))
      (set! (.-value (.getElementById js/document "family-template"))
            (pr-str (:family/template definition))))))
(defn- refresh-family-types! []
  (let [select (.getElementById js/document "family-type") family (selected-family)]
    (set! (.-innerHTML select) "")
    (doseq [[type-key type-spec] (:family/types family)]
      (let [option (.createElement js/document "option")]
        (set! (.-value option) (name type-key))
        (set! (.-textContent option) (or (:name type-spec) (name type-key)))
        (.appendChild select option)))
    (load-family-form!)))
(defn- mesh [] (bim/merge-meshes (keep bim/element-mesh (all-elements))))
(defn- element-rows []
  (mapcat (fn [storey]
            (concat
             (map (fn [element]
                    (let [q (:quantities element)]
                      {:id (:id element) :storey (:name storey) :kind (name (:kind element)) :name (:name element)
                       :classification (get-in element [:classification :code] "")
                       :length (:length-m q) :gross-area (:gross-area-m2 q) :net-area (:net-area-m2 q)
                       :gross-volume (:gross-volume-m3 q) :net-volume (:net-volume-m3 q)})) (:elements storey))
             (map (fn [space]
                    (let [q (:quantities space)]
                      {:id (:id space) :storey (:name storey) :kind "space" :name (:name space)
                       :classification (name (:category space)) :length nil :gross-area (:gross-area-m2 q)
                       :net-area (:net-area-m2 q) :gross-volume (:gross-volume-m3 q) :net-volume (:net-volume-m3 q)}))
                  (:spaces storey)))) (storeys)))
(defn- format-quantity [value] (if (number? value) (.toFixed value 3) "—"))
(defn- refresh-schedule! []
  (let [container (.getElementById js/document "quantity-schedule") rows (element-rows)]
    (set! (.-innerHTML container) "")
    (doseq [row rows]
      (let [line (.createElement js/document "button")]
        (set! (.-textContent line) (str (:storey row) " · " (:kind row) " · " (:name row)
                                            " · L " (format-quantity (:length row))
                                            " · A " (format-quantity (:net-area row))
                                            " · V " (format-quantity (:net-volume row))))
        (.addEventListener line "click" #(do (swap! state assoc :active-storey
                                                     (:id (first (filter (fn [s] (= (:name s) (:storey row))) (storeys))))
                                                :selected (:id row) :selection #{(:id row)}) (refresh!)))
        (.appendChild container line)))))
(defn- csv-cell [value] (str "\"" (.replaceAll (str (or value "")) "\"" "\"\"") "\""))
(defn- schedule-csv []
  (let [columns [[:id "ID"] [:storey "Storey"] [:kind "Kind"] [:name "Name"] [:classification "Classification"]
                 [:length "Length m"] [:gross-area "Gross area m2"] [:net-area "Net area m2"]
                 [:gross-volume "Gross volume m3"] [:net-volume "Net volume m3"]]]
    (str (string/join "," (map (comp csv-cell second) columns)) "\n"
         (string/join "\n" (map (fn [row] (string/join "," (map #(csv-cell (get row (first %))) columns))) (element-rows))))))
(defn- download-schedule! []
  (let [a (.createElement js/document "a") url (.createObjectURL js/URL (js/Blob. #js [(schedule-csv)] #js {:type "text/csv;charset=utf-8"}))]
    (set! (.-href a) url) (set! (.-download a) "bim-quantity-schedule.csv") (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- clashes [] (bim/detect-clashes (:project @state) {:tolerance 0.001}))
(defn- refresh-clashes! []
  (let [container (.getElementById js/document "clash-results") results (clashes)]
    (set! (.-innerHTML container) "")
    (if (empty? results)
      (set! (.-textContent container) "No clashes")
      (doseq [[index result] (map-indexed vector results)]
        (let [button (.createElement js/document "button")]
          (set! (.-textContent button) (str "⚠ " (:clash/a result) " × " (:clash/b result)
                                             " · " (.toFixed (:clash/volume result) 4) " m³"))
          (when (= index (:selected-clash @state)) (.add (.-classList button) "selected"))
          (.addEventListener button "click" #(do (swap! state assoc :selected-clash index :active-storey (:clash/storey result)
                                                        :selected (:clash/a result)
                                                        :selection #{(:clash/a result)}) (refresh!)))
          (.appendChild container button))))))
(defn- run-clash-check! []
  (let [existing (into {} (map (juxt :bcf.topic/title identity)) (:review-topics @state))
        created (.toISOString (js/Date.))
        topics (mapv (fn [clash]
                       (let [title (str "Clash " (:clash/a clash) " × " (:clash/b clash))]
                         (or (get existing title)
                             (bcf/topic
                              {:guid (str (random-uuid)) :type "Clash" :status "Open"
                               :title title
                               :description (str "Overlap " (:clash/volume clash)
                                                 " m³ on storey " (:clash/storey clash))
                               :creation-date created :creation-author "bim-editor"
                               :priority "Normal" :labels ["Clash"]}))))
                     (clashes))]
    (swap! state assoc :review-topics topics :save-status :dirty)
    (refresh!)
    (workspace-status! (str (count topics) " BCF clash topics ready"))))
(defn- download-clashes! []
  (let [rows (clashes) data (str "storey,element_a,element_b,kind_a,kind_b,overlap_x,overlap_y,overlap_z,volume_m3\n"
                                 (string/join "\n" (map (fn [c] (string/join "," (concat [(:clash/storey c) (:clash/a c) (:clash/b c)]
                                                                                         (map name (:clash/kinds c)) (:clash/overlap c) [(:clash/volume c)]))) rows)))
        a (.createElement js/document "a") url (.createObjectURL js/URL (js/Blob. #js [data] #js {:type "text/csv;charset=utf-8"}))]
    (set! (.-href a) url) (set! (.-download a) "bim-clashes.csv") (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- refresh! []
  (when-let [v @viewport]
    (let [m (mesh)
          model-items (interaction/element-render-items (all-elements) (selection-ids))
          overlay-items
          (keep (fn [member]
                  (when-let [mesh (bim/element-mesh
                                   {:kind :analysis-result
                                    :geometry {:kind :swept-disk-solid
                                               :directrix (:structural.overlay/deformed-axis member)
                                               :radius 0.035}})]
                    {:element/id [:analysis (:structural.overlay/member-id member)]
                     :mesh mesh :color (:structural.overlay/color member)}))
                (get-in @state [:structural-overlay :structural.overlay/members]))
          draws (mapv (fn [{:keys [element/id mesh color]}]
                        {:element/id id :buffers (gpu/upload-mesh! (:mesh-context v) mesh)
                         :color color})
                      (concat model-items overlay-items))]
      (swap! viewport assoc :draws draws)
      (set! (.-textContent (.getElementById js/document "stats")) (str (count (storeys)) " storeys · " (count (all-elements)) " elements · " (/ (count (:indices m)) 3) " triangles"))
      (set! (.-textContent (.getElementById js/document "debug-state"))
            (js/JSON.stringify (clj->js {:storeyCount (count (storeys)) :activeStorey (:active-storey @state)
                                         :elementCount (count (all-elements))
                                         :spaceCount (count (all-spaces))
                                         :wallCount (count (filter #(= :wall (:kind %)) (all-elements)))
                                         :slabCount (count (filter #(= :slab (:kind %)) (all-elements)))
                                         :openingCount (reduce + (map #(count (:openings %)) (filter #(= :wall (:kind %)) (all-elements))))
                                         :scheduleRows (count (element-rows))
                                         :grossVolume (reduce + 0 (keep #(get-in % [:quantities :gross-volume-m3]) (all-elements)))
                                         :selected (:selected @state) :selection (vec (selection-ids))
                                         :profile (name (:profile @state))
                                         :projectVersion project/current-version :revision (:revision @state) :saveStatus (name (:save-status @state))
                                         :clashCount (count (clashes)) :selectedClash (:selected-clash @state)
                                         :shortcutBuffer (:shortcut-buffer @state)})))))
  (let [levels (.getElementById js/document "levels")]
    (set! (.-innerHTML levels) "")
    (doseq [storey (storeys)]
      (let [b (.createElement js/document "button")]
        (set! (.-textContent b) (str (:name storey) " · " (.toFixed (:elevation storey) 2) " m"))
        (when (= (:id storey) (:active-storey @state)) (.add (.-classList b) "selected"))
        (.addEventListener b "click" #(do (swap! state assoc :active-storey (:id storey))
                                           (select-only! (some-> (first (:elements storey)) :id))
                                           (refresh!)))
        (.appendChild levels b))))
  (let [tree (.getElementById js/document "tree")]
    (set! (.-innerHTML tree) "")
    (doseq [e (elements)]
      (let [b (.createElement js/document "button") icon ({:wall "▱" :door "▯" :window "▦"} (:kind e) "◇")]
        (set! (.-textContent b) (str icon " " (:name e)))
        (when (contains? (selection-ids) (:id e)) (.add (.-classList b) "selected"))
        (.addEventListener b "click" (fn [event]
                                        (apply-selection!
                                         (interaction/selection-after-click
                                          (selection-ids) (:id e) (selection-mode event)))
                                        (refresh!)))
        (.appendChild tree b))))
  (let [rooms (.getElementById js/document "rooms")]
    (set! (.-innerHTML rooms) "")
    (doseq [space (spaces)]
      (let [b (.createElement js/document "button")]
        (set! (.-textContent b) (str "▣ " (:name space) " · " (format-quantity (get-in space [:quantities :net-area-m2])) " m²"))
        (when (= (:id space) (:selected-space @state)) (.add (.-classList b) "selected"))
        (.addEventListener b "click" #(do (swap! state assoc :selected-space (:id space))
                                           (set! (.-value (.getElementById js/document "room-name")) (:name space))
                                           (set! (.-value (.getElementById js/document "room-category")) (name (:category space)))
                                           (refresh!)))
        (.appendChild rooms b))))
  (refresh-schedule!)
  (refresh-clashes!)
  (refresh-drawing-annotations!)
  (when-let [e (selected)]
    (let [wall? (= :wall (:kind e))]
      (set! (.-textContent (.getElementById js/document "inspector-title")) (str "Selected " (name (:kind e))))
      (set! (.-value (.getElementById js/document "name")) (:name e))
      (set! (.-disabled (.getElementById js/document "apply")) (not wall?))
      (doseq [id ["length" "height" "thickness" "material"]] (set! (.-disabled (.getElementById js/document id)) (not wall?)))
      (when wall?
        (set! (.-value (.getElementById js/document "length")) (get-in e [:quantities :length-m]))
        (set! (.-value (.getElementById js/document "height")) (get-in e [:geometry :profile :height]))
        (set! (.-value (.getElementById js/document "thickness")) (get-in e [:geometry :profile :thickness]))))))
(defn- reassociate-drawing-views [views project]
  (into {} (map (fn [[storey-id view]]
                  [storey-id
                   (family/reassociate-view-annotations
                    view (:elements (bim/find-storey project storey-id)))])
                views)))
(defn- commit! [p]
  (swap! state
         (fn [s]
           (-> s (update :history conj (:project s))
               (assoc :project p :future [] :save-status :dirty
                      :drawing-views (reassociate-drawing-views (:drawing-views s) p)
                      :structural-model nil :structural-overlay nil
                      :electrical-distribution nil :stream-view nil)
               (update :revision inc))))
  (refresh!))
(defn- retain-mep-system! [system]
  (swap! state update :mep-systems
         (fn [systems]
           (conj (vec (remove #(= (:mep/id %) (:mep/id system)) systems)) system))))
(defn- authoring-commit! [label operation]
  (try
    (if-let [project (operation)]
      (do (commit! project)
          (set! (.-textContent (.getElementById js/document "authoring-status")) label))
      (set! (.-textContent (.getElementById js/document "authoring-status"))
            (str label " unavailable")))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "authoring-status"))
            (str "Error: " (.-message error))))))
(defn- camera-state []
  (let [{:keys [azimuth elevation camera-target camera-distance]} @state
        [tx ty tz] camera-target d camera-distance]
    {:eye [(+ tx (* d (js/Math.cos elevation) (js/Math.cos azimuth)))
           (+ ty (* d (js/Math.sin elevation)))
           (+ tz (* d (js/Math.cos elevation) (js/Math.sin azimuth)))]
     :target camera-target}))
(defn- draw! []
  (when-let [{:keys [draws] :as v} @viewport]
    (when draws
      (let [{:keys [eye target]} (camera-state)]
        (gpu/render-scene! v draws eye target))))
  (js/requestAnimationFrame draw!))
(defn- element-storey-id [element-id]
  (some (fn [storey]
          (when (some #(= element-id (:id %)) (:elements storey)) (:id storey)))
        (storeys)))
(defn- viewport-ndc-at [canvas client-x client-y]
  (let [rect (.getBoundingClientRect canvas)
        x (- (* 2.0 (/ (- client-x (.-left rect)) (.-width rect))) 1.0)
        y (- 1.0 (* 2.0 (/ (- client-y (.-top rect)) (.-height rect))))]
    [x y]))
(defn- viewport-ray-at [canvas client-x client-y]
  (let [[x y] (viewport-ndc-at canvas client-x client-y)
        rect (.getBoundingClientRect canvas)
        {:keys [eye target]} (camera-state)]
    (interaction/camera-ray {:eye eye :target target
                             :aspect (/ (.-width rect) (max 1.0 (.-height rect)))} x y)))
(defn- hit-at [canvas client-x client-y]
  (interaction/pick-element (all-elements) (viewport-ray-at canvas client-x client-y)))
(defn- pick-at! [canvas client-x client-y mode]
  (if-let [hit (hit-at canvas client-x client-y)]
      (let [element-id (:element/id hit)]
        (apply-selection! (interaction/selection-after-click (selection-ids) element-id mode))
        (swap! state assoc :active-storey (element-storey-id element-id))
        (refresh!))
      (do (when (= :replace mode) (select-only! nil)) (refresh!))))
(defn- frame-selected! []
  (when-let [bounds (some-> (selected) interaction/element-bounds)]
    (let [frame (interaction/frame-bounds bounds)]
      (swap! state assoc :camera-target (:camera/target frame)
             :camera-distance (:camera/distance frame))
      (refresh!))))
(defn- num [id] (js/parseFloat (.-value (.getElementById js/document id))))
(defn- edn-field [id fallback]
  (let [value (string/trim (.-value (.getElementById js/document id)))]
    (if (string/blank? value) fallback (reader/read-string value))))
(defn- snap-delta [elements delta]
  (let [elements (vec elements) selected-ids (set (map :id elements))
        enabled? (.-checked (.getElementById js/document "snap-enabled"))
        source (editor/model-snap-candidates elements)
        targets (editor/model-snap-candidates
                 (remove #(contains? selected-ids (:id %)) (all-elements)))
        evidence (when enabled?
                   (editor/snap-translation
                    source targets delta
                    {:grid (num "snap-grid") :tolerance (num "snap-tolerance")}))
        status (.getElementById js/document "snap-status")]
    (swap! state assoc :last-snap evidence)
    (set! (.-textContent status)
          (if evidence
            (str "Snapped " (name (:snap/kind evidence)) " · "
                 (.toFixed (:snap/distance evidence) 3) " m")
            (if enabled? "No snap in tolerance" "Snapping disabled")))
    (or (:snap/delta evidence) delta)))
(defn- transform-project [project elements transform & args]
  (reduce (fn [current element]
            (apply bim/update-element current (element-storey-id (:id element))
                   (:id element) transform args))
          project elements))
(defn- parse-point [id]
  (mapv #(js/parseFloat (.trim %))
        (string/split (.-value (.getElementById js/document id)) #",")))
(defn- parse-path [id]
  (mapv (fn [point]
          (mapv #(js/parseFloat (string/trim %)) (string/split point #",")))
        (remove string/blank?
                (map string/trim
                     (string/split (.-value (.getElementById js/document id)) #";")))))
(defn- active-drawing-view []
  (or (get-in @state [:drawing-views (:active-storey @state)])
      (family/drawing-view {:id (str "plan-" (:active-storey @state))
                            :kind :floor-plan :name "Floor Plan"})))
(defn- annotation-points-text [annotation]
  (let [points (case (:kind annotation)
                 :dimension [(:from annotation) (:to annotation)]
                 (:tag :text :level) [(:point annotation)]
                 (:points annotation))]
    (string/join ";" (map #(string/join "," %) points))))
(defn- load-drawing-annotation! [annotation]
  (swap! state assoc :selected-annotation (:annotation/id annotation))
  (set! (.-value (.getElementById js/document "drawing-annotation-kind"))
        (name (:kind annotation)))
  (set! (.-value (.getElementById js/document "drawing-annotation-points"))
        (annotation-points-text annotation))
  (set! (.-value (.getElementById js/document "drawing-annotation-text"))
        (or (:text annotation) (:label annotation) ""))
  (set! (.-value (.getElementById js/document "drawing-annotation-revision"))
        (or (:revision annotation) ""))
  (set! (.-textContent (.getElementById js/document "save-drawing-annotation"))
        "Update annotation")
  (refresh-drawing-annotations!))
(defn- refresh-drawing-annotations! []
  (when-let [container (.getElementById js/document "drawing-annotations")]
    (set! (.-innerHTML container) "")
    (doseq [annotation (:view/annotations (active-drawing-view))]
      (let [row (.createElement js/document "div")
            edit (.createElement js/document "button")
            delete (.createElement js/document "button")]
        (set! (.-textContent edit)
              (str (name (:kind annotation)) " · "
                   (or (:text annotation) (:label annotation)
                       (:revision annotation) (:annotation/id annotation))))
        (when (= (:annotation/id annotation) (:selected-annotation @state))
          (.add (.-classList edit) "selected"))
        (set! (.-textContent delete) "Delete")
        (.addEventListener edit "click" #(load-drawing-annotation! annotation))
        (.addEventListener delete "click"
                           #(do (swap! state update-in
                                      [:drawing-views (:active-storey @state)]
                                      family/remove-view-annotation (:annotation/id annotation))
                                (swap! state assoc :selected-annotation nil
                                       :save-status :dirty)
                                (swap! state update :revision inc)
                                (refresh!)))
        (.appendChild row edit) (.appendChild row delete) (.appendChild container row)))))
(defn- new-drawing-annotation! []
  (swap! state assoc :selected-annotation nil)
  (set! (.-textContent (.getElementById js/document "save-drawing-annotation"))
        "Add annotation")
  (set! (.-value (.getElementById js/document "drawing-annotation-points")) "0,0;6,0")
  (set! (.-value (.getElementById js/document "drawing-annotation-text")) "Note")
  (refresh-drawing-annotations!))
(defn- save-drawing-annotation! []
  (try
    (let [kind (keyword (.-value (.getElementById js/document "drawing-annotation-kind")))
          points (parse-path "drawing-annotation-points")
          points (mapv #(subvec % 0 (min 2 (count %))) points)
          text (.-value (.getElementById js/document "drawing-annotation-text"))
          revision (.-value (.getElementById js/document "drawing-annotation-revision"))
          existing-id (:selected-annotation @state)
          id (or existing-id (:next-annotation-id @state))
          spec (case kind
                 :dimension {:id id :kind kind :from (first points) :to (second points)
                             :label (when-not (string/blank? text) text)}
                 :tag {:id id :kind kind :point (first points) :text text}
                 :leader {:id id :kind kind :points points :text text}
                 :revision-cloud {:id id :kind kind :points points :revision revision})
          view (active-drawing-view)
          updated (if existing-id
                    (family/update-view-annotation view id spec)
                    (family/add-view-annotation view spec))]
      (swap! state assoc-in [:drawing-views (:active-storey @state)] updated)
      (swap! state (fn [state]
                     (cond-> (-> state (assoc :selected-annotation id :save-status :dirty)
                                 (update :revision inc))
                       (nil? existing-id) (update :next-annotation-id inc))))
      (set! (.-textContent (.getElementById js/document "drawing-annotation-status"))
            (str (if existing-id "Updated " "Added ") (name kind)))
      (set! (.-textContent (.getElementById js/document "save-drawing-annotation"))
            "Update annotation")
      (refresh-drawing-annotations!))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "drawing-annotation-status"))
            (str "Error: " (.-message error))))))
(defn- apply-view-range! []
  (try
    (let [range (family/view-range {:top (num "view-range-top")
                                    :cut-plane (num "view-range-cut")
                                    :bottom (num "view-range-bottom")
                                    :view-depth (num "view-range-depth")})
          storey-id (:active-storey @state)
          view (assoc (active-drawing-view) :view/view-range range)]
      (swap! state assoc-in [:drawing-views storey-id] view)
      (swap! state assoc :save-status :dirty)
      (swap! state update :revision inc)
      (set! (.-textContent (.getElementById js/document "drawing-annotation-status"))
            "View range applied")
      (refresh!))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "drawing-annotation-status"))
            (str "Error: " (.-message error))))))
(defn- auto-tag-elements! []
  (try
    (let [family-id (.-value (.getElementById js/document "annotation-family"))
          definition (get-in @state [:family-catalog :family-catalog/families family-id])
          type-key (first (sort (keys (:family/types definition))))
          tags (family/tag-elements-with-family
                (:family-catalog @state) family-id type-key
                (:elements (bim/find-storey (:project @state) (:active-storey @state)))
                {:offset [0 0.2]})
          view (active-drawing-view)
          retained (into [] (remove #(= family-id (:annotation/family-id %)))
                         (:view/annotations view))
          updated (reduce family/add-view-annotation
                          (assoc view :view/annotations retained) tags)]
      (swap! state assoc-in [:drawing-views (:active-storey @state)] updated)
      (swap! state assoc :selected-annotation nil :save-status :dirty)
      (swap! state update :revision inc)
      (set! (.-textContent (.getElementById js/document "drawing-annotation-status"))
            (str (count tags) " tags placed"))
      (refresh!))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "drawing-annotation-status"))
            (str "Error: " (.-message error))))))
(defn- apply-print-setting! []
  (try
    (let [setting (family/print-setting
                   {:id :project-print :name "Project print"
                    :paper-size (keyword (.-value (.getElementById js/document "print-paper")))
                    :orientation (keyword (.-value (.getElementById js/document "print-orientation")))
                    :scale (num "print-scale")
                    :color-mode (keyword (.-value (.getElementById js/document "print-color")))
                    :margins-mm [5 5 5 5]})]
      (swap! state assoc :print-setting setting :save-status :dirty)
      (swap! state update :revision inc)
      (set! (.-textContent (.getElementById js/document "print-status"))
            (str (name (:print-setting/paper-size setting)) " · 1:"
                 (:print-setting/scale setting))));
    (catch :default error
      (set! (.-textContent (.getElementById js/document "print-status"))
            (str "Error: " (.-message error))))))
(defn- generate-drawing-set! []
  (try
    (let [generated (family/generate-drawing-set (:project @state))
          setting (:print-setting @state)
          paper-size (:print-setting/paper-size setting)
          [raw-width raw-height] (get drawing/sheet-sizes-mm paper-size [841 594])
          portrait? (= :portrait (:print-setting/orientation setting))
          [paper-width paper-height] (if portrait?
                                      [(min raw-width raw-height) (max raw-width raw-height)]
                                      [(max raw-width raw-height) (min raw-width raw-height)])
          view-ids (get-in generated [:drawing/sheets 0 :sheet/views])
          title-block (family/title-block
                       {:id :project-title-block :name "Project Title Block"
                        :width 250 :height 55
                        :organization (.-value (.getElementById js/document
                                                                "title-block-organization"))
                        :project (:project-name @state)
                        :client (.-value (.getElementById js/document "title-block-client"))
                        :drawn-by (.-value (.getElementById js/document
                                                            "title-block-drawn-by"))
                        :checked-by (.-value (.getElementById js/document
                                                              "title-block-checked-by"))
                        :status :preliminary})
          viewports (family/layout-sheet-viewports
                     view-ids {:paper-width paper-width :paper-height paper-height
                               :columns 2 :title-block-height 55
                               :scale (:print-setting/scale setting)})
          sheet (family/drawing-sheet
                 {:id "A-001" :number "A-001" :name "General Arrangements"
                  :size paper-size :viewports viewports :title-block title-block
                  :revisions [{:revision "P01" :status :preliminary}]})
          drawing-set (assoc generated :drawing/sheets [sheet])]
      (swap! state assoc :drawing-set drawing-set :save-status :dirty)
      (set! (.-textContent (.getElementById js/document "drawing-set-status"))
            (str (count (:drawing/views drawing-set)) " views · "
                 (count (:drawing/schedules drawing-set)) " schedules · A-001 ready")))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "drawing-set-status"))
            (str "Drawing set error: " (.-message error))))))
(defn- parse-material-layers [id]
  (mapv (fn [spec]
          (let [parts (mapv string/trim (string/split spec #":"))
                [material thickness category] parts]
            (when (or (not= 3 (count parts)) (string/blank? material) (string/blank? thickness)
                      (string/blank? category))
              (throw (js/Error. (str "Invalid wall layer: " spec))))
            (bim/material-layer material (js/parseFloat thickness) false (keyword category))))
        (remove string/blank?
                (map string/trim
                     (string/split (.-value (.getElementById js/document id)) #",")))))
(defn- parse-wall-layers [] (parse-material-layers "wall-layers"))
(defn- element-obstacle [element]
  (when-let [mesh (bim/element-mesh element)]
    (let [positions (:positions mesh)]
      {:min (apply mapv min positions) :max (apply mapv max positions)})))
(defn- analyze-structure! []
  (let [span (num "structural-span") load-n (* 1000.0 (num "structural-load"))
        model (family/structural-model
               {:nodes [(family/structural-node {:id :fixed :point [0.0 0.0]
                                                 :restraints [true true]})
                        (family/structural-node {:id :loaded :point [span 0.0]
                                                 :restraints [false true]})]
                :members [(family/structural-analysis-member
                           {:id :member :start-node :fixed :end-node :loaded
                            :area-m2 0.01 :elastic-modulus-pa 2.0e11 :material "S355"})]
                :load-cases [(family/structural-load-case
                              {:id :design :name "Design" :nodal-loads
                               [{:node :loaded :fx load-n :fy 0.0}]})]})
        result (family/analyze-2d-truss model :design)
        displacement (get-in result [:structural.analysis/displacements :loaded 0])
        force (get-in result [:structural.analysis/member-axial-forces :member])]
    (set! (.-textContent (.getElementById js/document "engineering-status"))
          (str "Axial " (.toFixed (/ force 1000.0) 2) " kN · Δ "
               (.toFixed (* displacement 1000.0) 3) " mm"))))
(defn- assign-structural-role! []
  (try
    (let [element (selected)
          role (keyword (.-value (.getElementById js/document "structural-role")))
          valid? (case (:kind element)
                   :wall (= :shear-wall role)
                   :slab (= :diaphragm role)
                   (contains? #{:beam :column} role))]
      (when-not element (throw (js/Error. "Select an element")))
      (when-not valid?
        (throw (js/Error. (str "Role " (name role) " is incompatible with "
                               (name (:kind element))))))
      (let [axis (when (contains? #{:beam :column} role)
                   (or (:structural/analytical-axis element)
                       (get-in element [:geometry :axis])))
            structural (family/structural-member
                        element {:role role :analytical-axis axis
                                 :section {:kind :rectangle
                                           :width-m (num "structural-width")
                                           :depth-m (num "structural-depth")}
                                 :material {:name "Structural Steel"
                                            :elastic-modulus-pa 2.0e11
                                            :yield-strength-pa 3.55e8
                                            :density-kg-m3 7850}})]
        (commit! (bim/update-element (:project @state) (element-storey-id (:id element))
                                     (:id element) (constantly structural)))
        (set! (.-textContent (.getElementById js/document "structural-model-status"))
              (str "Assigned " (name role) " to " (:id element)))))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str "Error: " (.-message error))))))
(defn- generate-structural-model! []
  (try
    (let [model (family/generate-structural-model (:project @state))]
      (swap! state assoc :structural-model model :structural-overlay nil)
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str (count (:structural/nodes model)) " nodes · "
                 (count (:structural/members model)) " members · "
                 (count (:structural/shells model)) " shells · "
                 (count (family/validate-structural-model model)) " issues")))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str "Error: " (.-message error))))))
(defn- generate-structural-loads! []
  (try
    (let [model (or (:structural-model @state)
                    (family/generate-structural-model (:project @state)))
          area-pressure (* 1000.0 (num "structural-area-load"))
          wind-pressure (* 1000.0 (num "structural-wind-pressure"))
          seismic-coefficient (num "structural-seismic-coefficient")
          diaphragm-pressures
          (into {} (keep (fn [shell]
                           (when (contains? #{:floor :diaphragm}
                                            (:structural.shell/role shell))
                             [(:structural.shell/id shell) area-pressure])))
                (:structural/shells model))
          area-case (family/structural-area-load-case
                     model {:id :live :name "Floor live load"
                            :pressures-pa diaphragm-pressures})
          wind-case (family/structural-wind-load-case
                     model {:id :wind :name "Wind +X" :pressure-pa wind-pressure
                            :direction [1 0 0]})
          node-elevations (map #(nth (:structural.node/point %) 2)
                               (:structural/nodes model))
          seismic-ready? (and (some :structural.member/density-kg-m3
                                    (:structural/members model))
                              (seq node-elevations)
                              (< (reduce min node-elevations) (reduce max node-elevations)))
          seismic-case (when seismic-ready?
                         (family/structural-seismic-load-case
                          model {:id :seismic :coefficient seismic-coefficient
                                 :direction [1 0 0]}))
          generated (cond-> [area-case wind-case] seismic-case (conj seismic-case))
          generated-ids (set (map :structural.load-case/id generated))
          cases (into (vec (remove #(contains? generated-ids
                                               (:structural.load-case/id %))
                                   (:structural/load-cases model))) generated)
          factors (cond-> {:dead 1.35 :live 1.5 :wind 1.5}
                    seismic-case (assoc :seismic 1.0))
          combination (family/structural-load-combination
                       {:id :design-uls :name "Generated ULS" :kind :ultimate
                        :factors factors})
          model (assoc model :structural/load-cases cases
                             :structural/combinations [combination])]
      (swap! state assoc :structural-model model)
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str (count cases) " load cases · "
                 (reduce + (map #(count (:structural.load-case/nodal-loads %)) generated))
                 " nodal loads · ULS ready")))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str "Error: " (.-message error))))))
(defn- run-structural-analysis! []
  (try
    (let [model (:structural-model @state)
          _ (when-not model (throw (js/Error. "Generate analytical model and loads first")))
          analysis (family/analyze-structural-combination model :design-uls)
          overlay (family/structural-result-overlay
                   model analysis {:deformation-scale (num "structural-deformation-scale")})
          utilizations (keep :structural.overlay/utilization
                             (:structural.overlay/members overlay))
          maximum (when (seq utilizations) (reduce max utilizations))]
      (swap! state assoc :structural-overlay overlay)
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str (count (:structural.overlay/members overlay)) " results · max utilization "
                 (if maximum (.toFixed maximum 3) "n/a")))
      (refresh!))
    (catch :default error
      (swap! state assoc :structural-overlay nil)
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str "Analysis error: " (.-message error))))))
(defn- run-3d-frame-analysis! []
  (try
    (let [model (:structural-model @state)
          _ (when-not model (throw (js/Error. "Generate analytical model and loads first")))
          analysis (family/analyze-3d-frame-combination model :design-uls)
          overlay (family/structural-result-overlay
                   model analysis {:deformation-scale (num "structural-deformation-scale")})
          reactions (vals (:structural.analysis/reactions analysis))
          max-vertical-reaction-n
          (when (seq reactions) (reduce max (map #(js/Math.abs (nth % 2)) reactions)))
          max-moment-nm
          (when (seq (:structural.analysis/member-results analysis))
            (reduce max (map :max-moment-nm (vals (:structural.analysis/member-results analysis)))))]
      (swap! state assoc :structural-overlay overlay)
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str (count (:structural.overlay/members overlay))
                 " six-DOF frame results · max vertical reaction "
                 (if max-vertical-reaction-n (.toFixed max-vertical-reaction-n 1) "n/a")
                 " N · max moment " (if max-moment-nm (.toFixed max-moment-nm 1) "n/a") " N·m"))
      (refresh!))
    (catch :default error
      (swap! state assoc :structural-overlay nil)
      (set! (.-textContent (.getElementById js/document "structural-model-status"))
            (str "3D frame analysis error: " (.-message error))))))
(defn- route-pipe! []
  (try
    (let [start (parse-point "pipe-start") end (parse-point "pipe-end")
          diameter (num "pipe-diameter")
          flow (/ (num "pipe-flow") 1000.0)
          slope (/ (num "pipe-slope") 100.0)
          obstacles (vec (keep element-obstacle (all-elements)))
          raw-path (family/route-mep start end obstacles
                                     {:step 0.25 :clearance diameter})]
      (if (< (count raw-path) 2)
        (set! (.-textContent (.getElementById js/document "engineering-status"))
              "No route found")
        (let [path (if (pos? slope)
                     (family/grade-mep-route raw-path slope)
                     raw-path)
              route-id (str "route-" (:next-id @state))
              assembly (mep/route-assembly
                        {:id route-id :system-id :hydronic :domain :piping
                         :shape :round :size diameter :points path})
              segments (:mep.assembly/segments assembly)
              fittings (:mep.assembly/fittings assembly)
              system (family/mep-system
                      {:id route-id :name route-id :kind :hydronic :medium :water
                       :design-flow flow :segments segments :fittings fittings})
              analysis (family/analyze-mep-system
                        system {:roughness-m 1.5e-6 :density-kg-m3 998.0
                                :viscosity-pa-s 0.001})
              elements (concat segments fittings)
              project (reduce #(bim/add-element %1 (:active-storey @state) %2)
                              (:project @state) elements)
              loss (:mep.analysis/total-pressure-loss-pa analysis)]
          (swap! state update :next-id inc)
          (select-only! (:id (first segments)))
          (set! (.-textContent (.getElementById js/document "engineering-status"))
                (str (count segments) " segments · " (count fittings)
                     " fittings · " (.toFixed loss 1) " Pa"))
          (commit! project)
          (retain-mep-system! system))))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Route error: " (.-message error))))))
(defn- point-distance [a b]
  (js/Math.sqrt (reduce + (map (fn [left right]
                                 (let [delta (- right left)] (* delta delta)))
                               a b))))
(defn- design-mep-network! []
  (try
    (let [source (parse-point "mep-network-source")
          junction (parse-point "mep-network-junction")
          terminal-a (parse-point "mep-network-terminal-a")
          terminal-b (parse-point "mep-network-terminal-b")
          demand-a (/ (num "mep-network-demand-a") 1000.0)
          demand-b (/ (num "mep-network-demand-b") 1000.0)
          max-velocity (num "mep-network-max-velocity")
          network-id (str "network-" (:next-id @state))
          edge-id #(str network-id "-" %)
          design (family/size-and-balance-mep-network
                  {:source-node :source
                   :segments [{:id (edge-id "main") :from :source :to :junction
                               :length-m (point-distance source junction)}
                              {:id (edge-id "branch-a") :from :junction :to :terminal-a
                               :length-m (point-distance junction terminal-a)
                               :minor-loss-coefficient 1.0}
                              {:id (edge-id "branch-b") :from :junction :to :terminal-b
                               :length-m (point-distance junction terminal-b)
                               :minor-loss-coefficient 1.0}]
                   :terminal-demands {:terminal-a demand-a :terminal-b demand-b}}
                  {:roughness-m 1.5e-6 :density-kg-m3 998.0 :viscosity-pa-s 0.001}
                  {:available-diameters-m [0.025 0.032 0.04 0.05 0.063 0.075
                                           0.09 0.1 0.125 0.15 0.2]
                   :max-velocity-m-s max-velocity
                   :equipment-efficiency 0.7 :pressure-safety-factor 1.1})
          sized-by-id (into {} (map (juxt :id identity)
                                    (:mep.network/segments design)))
          assembly (mep/network-assembly
                    {:id network-id :system-id :hydronic :domain :piping
                     :nodes {:source {:point source} :junction {:point junction}
                             :terminal-a {:point terminal-a}
                             :terminal-b {:point terminal-b}}
                     :edges (mapv (fn [{:keys [id from to] :as segment}]
                                    {:id id :from from :to to
                                     :size (:mep/diameter-m segment)})
                                  (:mep.network/segments design))})
          segments (mapv (fn [segment]
                           (let [sizing (sized-by-id (:id segment))]
                             (assoc segment
                                    :mep/design-flow (:segment/flow-m3-s sizing)
                                    :mep/velocity-m-s (:mep/velocity-m-s sizing)
                                    :mep/pressure-loss-pa (:mep/pressure-loss-pa sizing))))
                         (:mep.assembly/segments assembly))
          fittings (:mep.assembly/fittings assembly)
          system (family/mep-system
                  {:id network-id :name network-id :kind :hydronic :medium :water
                   :design-flow (:mep.network/source-flow-m3-s design)
                   :segments segments :fittings fittings})
          issues (family/validate-mep-system system)
          _ (when (seq issues)
              (throw (js/Error. (str "Invalid network: " (pr-str issues)))))
          project (reduce #(bim/add-element %1 (:active-storey @state) %2)
                          (:project @state) (concat segments fittings))
          pressure (:mep.network/required-equipment-pressure-pa design)
          power (:mep.network/equipment-power-w design)]
      (swap! state update :next-id inc)
      (select-only! (:id (first segments)))
      (commit! project)
      (retain-mep-system! system)
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Branched network · " (count segments) " segments · "
                 (count fittings) " tee/fittings · " (.toFixed pressure 1)
                 " Pa duty · " (.toFixed power 1) " W · balanced")))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Network error: " (.-message error))))))
(defn- design-closed-loop-mep-network! []
  (try
    (let [source (parse-point "closed-loop-source")
          left (parse-point "closed-loop-left")
          right (parse-point "closed-loop-right")
          terminal (parse-point "closed-loop-terminal")
          diameter (num "closed-loop-diameter")
          source-pressure (* 1000.0 (num "closed-loop-source-pressure"))
          demand (/ (num "closed-loop-demand") 1000.0)
          network-id (str "closed-loop-" (:next-id @state))
          edge-id #(str network-id "-" %)
          assembly (mep/network-assembly
                    {:id network-id :system-id :chilled-water :domain :piping
                     :default-size diameter
                     :nodes {:source {:point source} :left {:point left}
                             :right {:point right} :terminal {:point terminal}}
                     :edges [{:id (edge-id "sl") :from :source :to :left}
                             {:id (edge-id "lt") :from :left :to :terminal}
                             {:id (edge-id "sr") :from :source :to :right}
                             {:id (edge-id "rt") :from :right :to :terminal}]})
          segments (:mep.assembly/segments assembly)
          fittings (:mep.assembly/fittings assembly)
          system (family/mep-system
                  {:id network-id :name network-id :kind :hydronic :medium :chilled-water
                   :design-flow demand :segments segments :fittings fittings})
          design (family/analyze-closed-authored-mep-system
                  system {:source source-pressure} {:terminal demand}
                  {:density-kg-m3 998.0 :friction-factor 0.02} {})
          designed-system (:mep.design/system design)
          analysis (:mep.design/analysis design)
          designed-segments (:mep/segments designed-system)
          issues (family/validate-mep-system designed-system)
          _ (when (seq issues)
              (throw (js/Error. (str "Invalid closed-loop network: " (pr-str issues)))))
          project (reduce #(bim/add-element %1 (:active-storey @state) %2)
                          (:project @state) (concat designed-segments fittings))]
      (swap! state update :next-id inc)
      (select-only! (:id (first designed-segments)))
      (commit! project)
      (retain-mep-system! designed-system)
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Closed loop · " (count designed-segments) " segments · "
                 (:fluid.network/iterations analysis) " iterations · "
                 "residual " (.toExponential (:fluid.network/maximum-residual-m3-s analysis) 2)
                 " m³/s")))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Closed-loop network error: " (.-message error))))))
(defn- route-duct! []
  (try
    (let [start (parse-point "duct-start") end (parse-point "duct-end")
          flow (num "duct-flow") max-velocity (num "duct-max-velocity")
          sizing (family/size-rectangular-duct
                  flow max-velocity
                  [[0.2 0.15] [0.25 0.2] [0.3 0.2] [0.4 0.25]
                   [0.5 0.3] [0.5 0.4] [0.6 0.4] [0.8 0.5] [1.0 0.6]])
          width (:mep/width-m sizing) height (:mep/height-m sizing)
          obstacles (vec (keep element-obstacle (all-elements)))
          path (family/route-mep start end obstacles
                                 {:step 0.25 :clearance (/ (max width height) 2.0)})]
      (if (< (count path) 2)
        (set! (.-textContent (.getElementById js/document "engineering-status"))
              "No duct route found")
        (let [route-id (str "duct-" (:next-id @state))
              assembly (mep/rectangular-route-assembly
                        {:id route-id :system-id :supply-air :domain :hvac
                         :width width :height height :points path})
              raw-segments (:mep.assembly/segments assembly)
              fittings (:mep.assembly/fittings assembly)
              results (mapv (fn [index segment]
                              (let [[a b] (map :connector/point (:mep/connectors segment))]
                                (family/rectangular-duct-pressure-loss
                                 {:length-m (point-distance a b) :width-m width
                                  :height-m height :roughness-m 9.0e-5
                                  :flow-m3-s flow :density-kg-m3 1.204
                                  :viscosity-pa-s 1.81e-5
                                  :minor-loss-coefficient
                                  (if (< index (count fittings)) 0.5 0.0)})))
                            (range) raw-segments)
              segments (mapv (fn [segment result]
                               (assoc segment :mep/design-flow flow
                                      :mep/velocity-m-s (:mep/velocity-m-s result)
                                      :mep/pressure-loss-pa (:mep/pressure-loss-pa result)
                                      :mep/hydraulic-diameter-m
                                      (:mep/hydraulic-diameter-m result)))
                             raw-segments results)
              system (family/mep-system
                      {:id route-id :name route-id :kind :hvac :medium :air
                       :design-flow flow :segments segments :fittings fittings})
              issues (family/validate-mep-system system)
              _ (when (seq issues)
                  (throw (js/Error. (str "Invalid duct: " (pr-str issues)))))
              loss (reduce + (map :mep/pressure-loss-pa results))
              project (reduce #(bim/add-element %1 (:active-storey @state) %2)
                              (:project @state) (concat segments fittings))]
          (swap! state update :next-id inc)
          (select-only! (:id (first segments)))
          (commit! project)
          (retain-mep-system! system)
          (set! (.-textContent (.getElementById js/document "engineering-status"))
                (str "Supply duct " width " × " height " m · "
                     (.toFixed (:mep/velocity-m-s sizing) 2) " m/s · "
                     (count fittings) " elbows · " (.toFixed loss 1) " Pa")))))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Duct error: " (.-message error))))))
(defn- design-electrical-panel! []
  (try
    (let [id (:next-id @state) panel-id (str "panel-" id)
          point (parse-point "panel-point")
          demand-factor (/ (num "panel-demand-factor") 100.0)
          main-rating (num "panel-main-rating")
          circuit-specs [{:suffix "lighting" :name "Lighting"
                          :load (num "panel-load-a") :length (num "panel-length-a")}
                         {:suffix "sockets" :name "Sockets"
                          :load (num "panel-load-b") :length (num "panel-length-b")}
                         {:suffix "equipment" :name "Equipment"
                          :load (num "panel-load-c") :length (num "panel-length-c")}]
          conductor-catalog [{:id :cu-1.5 :area-mm2 1.5 :ampacity-a 16.0}
                             {:id :cu-2.5 :area-mm2 2.5 :ampacity-a 24.0}
                             {:id :cu-4 :area-mm2 4.0 :ampacity-a 32.0}
                             {:id :cu-6 :area-mm2 6.0 :ampacity-a 41.0}
                             {:id :cu-10 :area-mm2 10.0 :ampacity-a 57.0}
                             {:id :cu-16 :area-mm2 16.0 :ampacity-a 76.0}
                             {:id :cu-25 :area-mm2 25.0 :ampacity-a 101.0}]
          unsized (mapv (fn [{:keys [suffix name load length]}]
                          (mep/electrical-circuit
                           {:id (str panel-id "-" suffix) :name name
                            :apparent-power-va load :voltage-v 230.0
                            :power-factor 1.0 :poles 1 :length-m length}))
                        circuit-specs)
          selections (mapv #(mep/select-circuit-conductor
                             % conductor-catalog 3.0) unsized)
          circuits (mapv (fn [circuit selection]
                           (assoc circuit :circuit/conductor-area-mm2
                                  (:electrical.conductor/area-mm2 selection)
                                  :circuit/conductor selection))
                         unsized selections)
          panel-analysis (mep/analyze-panel
                          {:id panel-id :phases [:l1 :l2 :l3] :circuits circuits
                           :main-rating-a main-rating :demand-factor demand-factor
                           :max-imbalance-percent 50.0
                           :max-voltage-drop-percent 3.0})
          circuits (mapv #(assoc % :circuit/phase
                                 (get-in panel-analysis
                                         [:panel/assignments (:circuit/id %)]))
                         circuits)
          total-load (* demand-factor
                        (reduce + (map :circuit/apparent-power-va circuits)))
          feeder-circuit (mep/electrical-circuit
                          {:id (str panel-id "-feeder") :name "Panel feeder"
                           :apparent-power-va total-load :voltage-v 400.0
                           :power-factor 0.9 :poles 3 :length-m 30.0})
          feeder-conductor (mep/select-circuit-conductor
                            feeder-circuit conductor-catalog 3.0)
          feeder-entry (:electrical.conductor/catalog-entry feeder-conductor)
          feeder-area (:area-mm2 feeder-entry)
          feeder-analysis (mep/analyze-electrical-feeder
                           {:id (:circuit/id feeder-circuit) :phases 3
                            :apparent-power-va total-load :voltage-v 400.0
                            :power-factor 0.9 :length-m 30.0
                            :resistance-ohm-m (/ 1.724e-8 (* feeder-area 1.0e-6))
                            :reactance-ohm-m 8.0e-5
                            :source-resistance-ohm 0.01 :source-reactance-ohm 0.02
                            :cable-ampacity-a (:ampacity-a feeder-entry)
                            :max-voltage-drop-percent 3.0})
          protection (mep/select-protective-device
                      feeder-analysis
                      [{:id :mcb-10 :rating-a 10.0 :breaking-capacity-a 10000.0
                        :instantaneous-trip-multiple 10.0}
                       {:id :mcb-16 :rating-a 16.0 :breaking-capacity-a 10000.0
                        :instantaneous-trip-multiple 10.0}
                       {:id :mcb-20 :rating-a 20.0 :breaking-capacity-a 10000.0
                        :instantaneous-trip-multiple 10.0}
                       {:id :mcb-32 :rating-a 32.0 :breaking-capacity-a 10000.0
                        :instantaneous-trip-multiple 10.0}
                       {:id :mccb-50 :rating-a 50.0 :breaking-capacity-a 15000.0
                        :instantaneous-trip-multiple 10.0}
                       {:id :mccb-63 :rating-a 63.0 :breaking-capacity-a 15000.0
                        :instantaneous-trip-multiple 10.0}])
          connector (family/mep-connector
                     {:id (str panel-id "-main") :point point :direction [1 0 0]
                      :domain :electrical :shape :cable :size feeder-area
                      :flow-direction :in})
          panel (family/mep-equipment
                 {:id id :name panel-id :kind :panelboard :system-id :normal-power
                  :connectors [connector]
                  :geometry {:kind :extruded-area-solid
                             :profile {:kind :rectangle :x-dim 0.6 :y-dim 0.2}
                             :position {:location point} :direction [0 0 1] :depth 0.9}
                  :properties {:electrical/circuits circuits
                               :electrical/panel-analysis panel-analysis
                               :electrical/feeder-analysis feeder-analysis
                               :electrical/feeder-conductor feeder-conductor
                               :electrical/protection protection}})
          system (family/mep-system
                  {:id panel-id :name panel-id :kind :electrical
                   :medium :electricity :design-flow total-load
                   :equipment [panel]})
          breaker (get-in protection [:electrical.protection/device :rating-a])
          issue-count (+ (count (:panel/issues panel-analysis))
                         (count (:electrical.feeder/issues feeder-analysis)))]
      (swap! state update :next-id inc)
      (select-only! id)
      (commit! (bim/add-element (:project @state) (:active-storey @state) panel))
      (retain-mep-system! system)
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Panel " panel-id " · 3 circuits · feeder " feeder-area
                 " mm² · " breaker " A protection · imbalance "
                 (.toFixed (:panel/imbalance-percent panel-analysis) 1)
                 "% · " issue-count " issues")))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Electrical error: " (.-message error))))))
(defn- analyze-electrical-distribution! []
  (try
    (let [topology (reader/read-string
                    (.-value (.getElementById js/document "electrical-distribution-topology")))
          result (mep/analyze-electrical-distribution topology)
          coordinations (vals (:electrical.distribution/coordinations result))
          coordinated (count (filter :electrical.coordination/coordinated? coordinations))
          issue-count (count (:electrical.distribution/issues result))]
      (swap! state assoc :electrical-distribution result)
      (set! (.-textContent (.getElementById js/document "electrical-distribution-status"))
            (str (count (:electrical.distribution/panels result)) " panels · "
                 (count (:electrical.distribution/feeders result)) " feeders · "
                 (.toFixed (:electrical.distribution/total-load-va result) 0) " VA total · "
                 coordinated "/" (count coordinations) " coordinated · "
                 issue-count " issues")))
    (catch :default error
      (swap! state assoc :electrical-distribution nil)
      (set! (.-textContent (.getElementById js/document "electrical-distribution-status"))
            (str "Distribution error: " (.-message error))))))
(defn- add-mep-equipment! []
  (try
    (let [id (:next-id @state)
          point (parse-point "mep-equipment-point")
          kind (keyword (.-value (.getElementById js/document "mep-equipment-kind")))
          domain (keyword (.-value (.getElementById js/document "mep-connector-domain")))
          size (num "mep-connector-size")
          connector-id (str "equipment-" id "-port")
          flow (if (= :pump kind) :out :in)
          connector (family/mep-connector
                     {:id connector-id :point point
                      :direction (if (= :out flow) [1 0 0] [-1 0 0])
                      :domain domain :shape :round :size size :flow-direction flow})
          equipment (family/mep-equipment
                     {:id id :name (str (name kind) " " id) :kind kind
                      :system-id (if (= :hvac domain) :supply-air :hydronic)
                      :connectors [connector]
                      :geometry {:kind :swept-disk-solid
                                 :directrix [point (mapv + point [0.4 0 0])]
                                 :radius (max 0.1 size)}})]
      (swap! state update :next-id inc)
      (select-only! id)
      (commit! (bim/add-element (:project @state) (:active-storey @state) equipment))
      (set! (.-value (.getElementById js/document "mep-connector-a")) connector-id)
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Added " (name kind) " · " connector-id)))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Error: " (.-message error))))))
(defn- connect-mep-elements! []
  (try
    (let [connector-a (.-value (.getElementById js/document "mep-connector-a"))
          connector-b (.-value (.getElementById js/document "mep-connector-b"))
          result (family/connect-mep-elements (all-elements) connector-a connector-b)
          project (reduce (fn [project element]
                            (bim/update-element project (element-storey-id (:id element))
                                                (:id element) (constantly element)))
                          (:project @state) (:mep/elements result))]
      (commit! project)
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Connected " connector-a " ↔ " connector-b)))
    (catch :default error
      (set! (.-textContent (.getElementById js/document "engineering-status"))
            (str "Error: " (.-message error))))))
(defn- editable-target? [event]
  (let [target (.-target event) tag (some-> target .-tagName .toLowerCase)]
    (or (#{"input" "select" "textarea"} tag) (.-isContentEditable target))))
(def revit-shortcuts {"wa" "add-wall" "dr" "add-door" "wn" "add-window" "ll" "add-level"
                      "fl" "add-slab" "rf" "add-roof" "st" "add-stair" "rl" "add-railing"
                      "cw" "add-curtain"
                      "rm" "auto-rooms"
                      "mv" "move-element" "co" "copy-element"
                      "ro" "rotate-element" "mm" "mirror-element" "ar" "array-element"
                      "al" "align-elements" "of" "offset-walls" "tr" "trim-walls"
                      "wj" "join-walls" "wl" "apply-wall-layers"
                      "sl" "apply-slab-layers" "se" "apply-slab-shape"
                      "so" "add-shaft-opening" "rh" "rehost-element"})
(def profile-shortcuts
  {:archicad {"w" "add-wall" "d" "add-door" "n" "add-window" "l" "add-level"
              "f" "add-slab" "m" "move-element" "c" "copy-element"
              "r" "rotate-element" "i" "mirror-element" "a" "array-element"}
   :vectorworks {"2" "add-wall" "d" "add-door" "w" "add-window" "l" "add-level"
                 "f" "add-slab" "m" "move-element" "c" "copy-element"
                 "r" "rotate-element" "i" "mirror-element" "a" "array-element"}})
(def ^:private storage-key "kami.bim-editor.project.v2")
(def ^:private backup-key "kami.bim-editor.project.backup")
(defn- coordinated-project []
  (integration/coordinated-model (:project @state) (:structural-model @state)
                                 (:mep-systems @state)))
(defn- project-document []
  (let [{:keys [project-id project-name project active-storey selected azimuth elevation
                camera-target camera-distance profile drawing-views drawing-set print-setting]} @state]
    (project/document {:id project-id :name project-name :building-model project
                       :family-catalog (:family-catalog @state)
                       :drawings drawing-views
                       :drawing-set drawing-set
                       :print-setting print-setting
                       :structural-model (:structural-model @state)
                       :mep-systems (:mep-systems @state)
                       :review-topics (:review-topics @state)
                       :cloud-state (:cloud-state @state)
                       :cloud-workspace (:cloud-workspace @state)
                       :cloud-checkpoint (:cloud-checkpoint @state)
                       :stream-settings (:stream-settings @state)
                       :editor {:active-storey active-storey :selected selected
                                :selection (vec (selection-ids))}
                       :camera {:azimuth azimuth :elevation elevation :target camera-target
                                :distance camera-distance}
                       :interaction {:profile profile}})))
(defn- save-project! []
  (let [data (pr-str (project-document)) old (.getItem js/localStorage storage-key)]
    (when old (.setItem js/localStorage backup-key old)) (.setItem js/localStorage storage-key data)
    (swap! state assoc :save-status :saved) (refresh!)))
(defn- apply-project! [value]
  (let [p (project/open value) model (:project/building-model p) editor (:project/editor p)
        camera (:project/camera p) interaction (:project/interaction p)
        element-ids (map :id (mapcat :elements (mapcat :storeys (mapcat :buildings (:sites model)))))
        space-ids (map :id (mapcat :spaces (mapcat :storeys (mapcat :buildings (:sites model)))))
        storey-ids (map :id (mapcat :storeys (mapcat :buildings (:sites model))))]
    (swap! state assoc :project-id (:project/id p) :project-name (:project/name p) :project model
           :active-storey (:active-storey editor) :selected (:selected editor)
           :selection (set (or (:selection editor)
                               (when-let [selected (:selected editor)] [selected])))
           :azimuth (:azimuth camera)
           :elevation (:elevation camera)
           :camera-target (or (:target camera) [4.0 1.5 3.0])
           :camera-distance (or (:distance camera) 14.0)
           :profile (:profile interaction) :next-id (inc (reduce max 13 element-ids))
           :next-storey-id (inc (reduce max 3 storey-ids)) :next-space-id (inc (reduce max 999 space-ids))
           :family-catalog
           (update (initial-family-catalog) :family-catalog/families merge
                   (get-in p [:project/family-catalog :family-catalog/families] {}))
           :drawing-views (:project/drawings p)
           :drawing-set (:project/drawing-set p)
           :structural-model (:project/structural-model p)
           :structural-overlay nil
           :mep-systems (:project/mep-systems p)
           :electrical-distribution nil
           :review-topics (:project/review-topics p)
           :cloud-state (:project/cloud-state p)
           :cloud-workspace (:project/cloud-workspace p)
           :cloud-checkpoint (:project/cloud-checkpoint p)
           :stream-settings (merge {:cell-size 10.0 :batch-size 512
                                    :max-resident 10000 :max-loads 1000}
                                   (:project/stream-settings p))
           :stream-view nil
           :print-setting (if (seq (:project/print-setting p))
                            (:project/print-setting p)
                            (family/print-setting {:id :default :paper-size :a1
                                                   :orientation :landscape :scale 100}))
           :selected-annotation nil
           :next-annotation-id
           (inc (reduce max 0 (keep #(when (number? (:annotation/id %)) (:annotation/id %))
                                    (mapcat :view/annotations (vals (:project/drawings p))))))
           :selected-space nil :history [] :future [] :shortcut-buffer "" :save-status :saved)
    (when-let [range (get-in (:project/drawings p)
                             [(:active-storey editor) :view/view-range])]
      (doseq [[id value] [["view-range-top" (:top range)]
                          ["view-range-cut" (:cut-plane range)]
                          ["view-range-bottom" (:bottom range)]
                          ["view-range-depth" (:view-depth range)]]]
        (set! (.-value (.getElementById js/document id)) value)))
    (when-let [setting (not-empty (:project/print-setting p))]
      (set! (.-value (.getElementById js/document "print-paper"))
            (name (:print-setting/paper-size setting)))
      (set! (.-value (.getElementById js/document "print-orientation"))
            (name (:print-setting/orientation setting)))
      (set! (.-value (.getElementById js/document "print-scale"))
            (:print-setting/scale setting))
      (set! (.-value (.getElementById js/document "print-color"))
            (name (:print-setting/color-mode setting))))
    (refresh-family-definitions!)
    (refresh-annotation-families!)
    (refresh-family-types!)
    (set! (.-value (.getElementById js/document "profile")) (name (:profile interaction))) (refresh!)))
(defn- load-project! []
  (when-let [data (.getItem js/localStorage storage-key)]
    (try (apply-project! (reader/read-string data))
         (catch :default _ (when-let [backup (.getItem js/localStorage backup-key)] (apply-project! (reader/read-string backup)))))))
(defn- download-project! []
  (let [bundle (integration/coordinated-revision
                {:project (:project @state) :project-id (:project-id @state)
                 :project-name (:project-name @state) :revision (:revision @state) :events []
                 :structural-model (:structural-model @state)
                 :mep-systems (:mep-systems @state)})
        a (.createElement js/document "a")
        url (.createObjectURL js/URL (js/Blob. #js [(pr-str bundle)] #js {:type "application/edn"}))]
    (set! (.-href a) url) (set! (.-download a) "building.coordinated-bim.edn")
    (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- download-text! [filename mime value]
  (let [a (.createElement js/document "a")
        url (.createObjectURL js/URL (js/Blob. #js [value] #js {:type mime}))]
    (set! (.-href a) url) (set! (.-download a) filename)
    (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- download-bytes! [filename mime bytes]
  (let [a (.createElement js/document "a")
        data (js/Uint8Array. (clj->js bytes))
        url (.createObjectURL js/URL (js/Blob. #js [data] #js {:type mime}))]
    (set! (.-href a) url) (set! (.-download a) filename)
    (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- download-ifc! []
  (download-text! "building.ifc" "application/x-step"
                  (ifc/write-spf (integration/coordinated-ifc (coordinated-project)))))
(defn- workspace-status! [message]
  (set! (.-textContent (.getElementById js/document "workspace-status")) message))
(defn- refresh-workspace-status! []
  (let [status (integration/capability-status
                {:project (:project @state) :family-catalog (:family-catalog @state)
                 :structural-model (:structural-model @state)
                 :mep-systems (:mep-systems @state) :drawing-set (:drawing-set @state)
                 :review-topics (:review-topics @state) :cloud-state (:cloud-state @state)
                 :electrical-distribution (:electrical-distribution @state)})
        ready (count (filter true? (vals status)))]
    (workspace-status! (str ready "/" (count status) " workspace capabilities ready"))))
(defn- refresh-stream! []
  (try
    (let [target (:camera-target @state)
          distance (:camera-distance @state)
          camera (mapv + target [0.0 0.0 distance])
          view (or (integration/model-bounds (all-elements))
                   {:min (mapv #(- % 1.0) target) :max (mapv #(+ % 1.0) target)})
          result (integration/large-model-view
                  (vec (all-elements)) view camera 1000.0 (:stream-settings @state))
          report (:workspace/query result)
          delta (:workspace/stream-delta result)]
      (swap! state assoc :stream-view result)
      (workspace-status!
       (str (:spatial/returned-elements report) " visible · "
            (count (:stream/load delta)) " load · "
            (count (:stream/evict delta)) " evict")))
    (catch :default error
      (workspace-status! (str "LOD error: " (.-message error))))))
(defn- publish-local-opencde! []
  (try
    (let [actor "bim-editor"
          project-id (:project-id @state)
          timestamp (.now js/Date)
          initial (or (:cloud-state @state)
                      (cloud/register-opencde-project
                       (cloud/opencde-store) actor
                       {:id project-id :name (:project-name @state)
                        :memberships {actor :admin "cloud-itonami" :viewer}
                        :timestamp timestamp}))
          text (ifc/write-spf (integration/coordinated-ifc (coordinated-project)))
          existing (cloud/get-opencde-document initial project-id actor "federated-ifc")
          base-version (or (:document/version existing) 0)
          hash (cloud/checksum text)
          document (cloud/publish-model-version
                    initial project-id actor
                    {:document-id "federated-ifc" :name (str (:project-name @state) ".ifc")
                     :content-ref (str "local://" project-id "/ifc/" (inc base-version))
                     :content-hash hash :base-version base-version
                     :idempotency-key (str project-id ":ifc:" base-version ":" hash)
                     :metadata {:design/revision (:revision @state)} :timestamp timestamp})
          topic-heads (into {} (map (fn [entry]
                                      [(get-in entry [:topic/value :bcf.topic/guid])
                                       (:topic/revision entry)]))
                            (cloud/list-opencde-topics (:opencde/state document)
                                                       project-id actor))
          topics (cloud/publish-bcf-topics
                  (:opencde/state document) project-id actor (:review-topics @state)
                  {:expected-revisions topic-heads
                   :idempotency-prefix (str project-id ":bcf:" timestamp)
                   :timestamp timestamp})
          electrical (:electrical-distribution @state)
          electrical-content (when electrical (pr-str electrical))
          electrical-existing (when electrical
                                (cloud/get-opencde-document
                                 (:opencde/state topics) project-id actor
                                 "electrical-distribution"))
          electrical-base-version (or (:document/version electrical-existing) 0)
          electrical-document
          (when electrical
            (cloud/publish-model-version
             (:opencde/state topics) project-id actor
             {:document-id "electrical-distribution"
              :name (str (:project-name @state) "-electrical-distribution.edn")
              :media-type "application/edn"
              :content-ref (str "local://" project-id "/electrical-distribution/"
                                (inc electrical-base-version))
              :content-hash (cloud/checksum electrical-content)
              :base-version electrical-base-version
              :idempotency-key (str project-id ":electrical-distribution:"
                                    electrical-base-version ":"
                                    (cloud/checksum electrical-content))
              :metadata {:design/revision (:revision @state)} :timestamp timestamp}))]
      (swap! state assoc :cloud-state (:opencde/state (or electrical-document topics))
             :save-status :dirty)
      (workspace-status!
       (str "OpenCDE IFC v" (get-in document [:opencde/document :document/version])
            " · " (count (:opencde/results topics)) " BCF topics"
            (when electrical-document
              (str " · electrical distribution v"
                   (get-in electrical-document [:opencde/document :document/version]))))))
    (catch :default error
      (workspace-status! (str "OpenCDE error: " (.-message error))))))
(defn- sync-electrical-distribution-remote!
  "Publish the electrical distribution analysis, if present, as its own
  OpenCDE document bound to the same durable collaboration head as the IFC
  document just synced. Resolves to a status suffix string; never rejects,
  so a failure here does not undo the IFC sync that already succeeded."
  [envelope]
  (if-let [electrical (:electrical-distribution @state)]
    (try
      (let [actor "bim-editor"
            content (pr-str electrical)
            content-hash (cloud/checksum content)
            existing (when-let [cloud-state (:cloud-state @state)]
                       (cloud/get-opencde-document cloud-state (:project-id @state)
                                                   actor "electrical-distribution"))
            publication (integration/cloud-opencde-publication
                         envelope
                         {:document-id "electrical-distribution"
                          :name (str (:project-name @state) "-electrical-distribution.edn")
                          :content content :media-type "application/edn"
                          :content-ref (str "cloud-itonami://" (:project-id @state) "/"
                                            (:design/head-revision envelope)
                                            "/electrical-distribution")
                          :content-hash content-hash
                          :base-version (or (:document/version existing) 0)
                          :timestamp (.now js/Date) :topic-revisions {} :topics []})
            request (integration/cloud-sync-request
                     {:base-url (.-value (.getElementById js/document "cloud-base-url"))
                      :org (.-value (.getElementById js/document "cloud-org"))
                      :repo (.-value (.getElementById js/document "cloud-repo"))
                      :cacao (.-value (.getElementById js/document "cloud-cacao"))}
                     envelope publication)]
        (-> (js/fetch
             (:url request)
             #js {:method (:method request)
                  :headers (clj->js (:headers request))
                  :body (js/JSON.stringify (clj->js (:body request)))})
            (.then (fn [response]
                     (-> (.json response)
                         (.then (fn [body]
                                  (if (.-ok response)
                                    " · electrical distribution synced"
                                    (str " · electrical sync error: "
                                         (or (aget body "reason") (aget body "error")
                                             (str "HTTP " (.-status response))))))))))
            (.catch (fn [error] (str " · electrical sync error: " (.-message error))))))
      (catch :default error
        (js/Promise.resolve (str " · electrical sync error: " (.-message error)))))
    (js/Promise.resolve "")))
(defn- publish-opencde! []
  (try
    (let [actor "bim-editor"
          project (assoc (coordinated-project)
                         :id (:project-id @state) :name (:project-name @state))
          package (integration/cloud-sync-package
                   (:cloud-workspace @state) (:cloud-checkpoint @state) project actor)
          timestamp (.now js/Date)
          ifc-text (ifc/write-spf (integration/coordinated-ifc project))
          content-hash (cloud/checksum ifc-text)
          existing (when-let [cloud-state (:cloud-state @state)]
                     (cloud/get-opencde-document cloud-state (:project-id @state)
                                                 actor "federated-ifc"))
          topic-heads (if-let [cloud-state (:cloud-state @state)]
                        (into {} (map (fn [entry]
                                       [(get-in entry [:topic/value :bcf.topic/guid])
                                        (:topic/revision entry)]))
                              (cloud/list-opencde-topics cloud-state
                                                         (:project-id @state) actor))
                        {})
          publication (integration/cloud-opencde-publication
                       (:envelope package)
                       {:document-id "federated-ifc"
                        :name (str (:project-name @state) ".ifc")
                        :content ifc-text
                        :content-ref (str "cloud-itonami://" (:project-id @state) "/"
                                          (:design/head-revision (:envelope package)))
                        :content-hash content-hash
                        :base-version (or (:document/version existing) 0)
                        :timestamp timestamp :topic-revisions topic-heads
                        :topics (:review-topics @state)})
          request (integration/cloud-sync-request
                   {:base-url (.-value (.getElementById js/document "cloud-base-url"))
                    :org (.-value (.getElementById js/document "cloud-org"))
                    :repo (.-value (.getElementById js/document "cloud-repo"))
                    :cacao (.-value (.getElementById js/document "cloud-cacao"))}
                   (:envelope package) publication)]
      (workspace-status! "Synchronizing durable BIM revision…")
      (-> (js/fetch
           (:url request)
           #js {:method (:method request)
                :headers (clj->js (:headers request))
                :body (js/JSON.stringify (clj->js (:body request)))})
          (.then (fn [response]
                   (-> (.json response)
                       (.then (fn [body]
                                (if (.-ok response)
                                  body
                                  (throw (js/Error.
                                          (or (aget body "reason")
                                              (aget body "error")
                                              (str "HTTP " (.-status response)))))))))))
          (.then (fn [body]
                   (let [ack (reader/read-string (aget body "ack-edn"))]
                     (swap! state assoc
                            :cloud-workspace (:workspace package)
                            :cloud-checkpoint (:design/checkpoint ack)
                            :cloud-sync-ack ack
                            :cloud-publication
                            (some-> (aget body "publication-edn") reader/read-string)
                            :save-status :dirty)
                     (-> (sync-electrical-distribution-remote! (:envelope package))
                         (.then (fn [electrical-suffix]
                                  (publish-local-opencde!)
                                  (workspace-status!
                                   (str "Cloud synced " (:design/head-revision ack)
                                        " · IFC + " (count (:review-topics @state))
                                        " BCF topics" electrical-suffix))))))))
          (.catch (fn [error]
                    (workspace-status! (str "Cloud sync error: " (.-message error)))))))
    (catch :default error
      (workspace-status! (str "Cloud sync error: " (.-message error))))))
(defn- download-bcf! []
  (download-text! "building.bcf.json" "application/json"
                  (.stringify js/JSON (clj->js {:bcf/version "3.0"
                                               :bcf/topics (:review-topics @state)})
                              nil 2)))
(defn- drawing-set-view-graphic [model drawing-set view-id]
  (let [view (first (filter #(= view-id (:view/id %)) (:drawing/views drawing-set)))
        schedule (first (filter #(= view-id (:schedule/id %))
                                (:drawing/schedules drawing-set)))]
    (cond
      schedule (drawing/schedule-table schedule)
      (= :floor-plan (:view/kind view))
      (when-let [storey (bim/find-storey model (:view/storey-id view))]
        (drawing/documented-floor-plan
         storey {:annotations (:view/annotations view)
                 :view-range (:view/view-range view)}))
      (contains? #{:section :elevation} (:view/kind view))
      (when-let [building (or (bim/find-building model (:view/building-id view))
                              (some-> model :sites first :buildings first))]
        (drawing/orthographic-view
         building {:kind (:view/kind view)
                   :axis (or (get-in view [:view/cut-plane :axis]) :x)
                   :cut-position (or (get-in view [:view/cut-plane :position]) 0.0)
                   :depth 1000.0 :scale (or (:view/scale view) 100)
                   :title (:view/name view)}))
      :else nil)))
(defn- download-drawing! []
  (let [model (:project @state)
        storey (bim/find-storey model (:active-storey @state))
        building (some-> model :sites first :buildings first)
        kind (keyword (.-value (.getElementById js/document "drawing-kind")))
        format (keyword (.-value (.getElementById js/document "drawing-format")))
        active-view (active-drawing-view)
        annotations (:view/annotations active-view)
        view-options {:annotations annotations :view-range (:view/view-range active-view)}
        plan (when storey (drawing/documented-floor-plan storey view-options))
        section (when building (drawing/orthographic-view building
                                                           {:kind :section :axis :x
                                                            :cut-position 0.0 :depth 20.0
                                                            :title "Section A"}))
        [filename content]
        (case kind
          :section [(str "section-" (:id building) ".svg")
                    (drawing/orthographic-view-svg building {:kind :section :axis :x
                                                             :cut-position 0.0 :depth 20.0
                                                             :title "Section A"})]
          :elevation [(str "elevation-" (:id building) ".svg")
                      (drawing/orthographic-view-svg building {:kind :elevation :axis :x
                                                               :cut-position 0.0
                                                               :title "North Elevation"})]
          :sheet
          (if-let [semantic-sheet (first (get-in @state [:drawing-set :drawing/sheets]))]
            ["sheet-A-001.svg"
             (drawing/drawing-sheet-svg
              {:number (:sheet/number semantic-sheet) :name (:sheet/name semantic-sheet)
               :size (:sheet/size semantic-sheet) :revision "P01"
               :title-block (:sheet/title-block semantic-sheet)
               :print-setting (:print-setting @state)
               :viewports
               (mapv (fn [viewport]
                       {:view (drawing-set-view-graphic model (:drawing-set @state)
                                                        (:viewport/view-id viewport))
                        :x (:viewport/x viewport) :y (:viewport/y viewport)
                        :width (:viewport/width viewport) :height (:viewport/height viewport)
                        :title (:viewport/title viewport) :scale (:viewport/scale viewport)})
                     (:sheet/viewports semantic-sheet))})]
            ["sheet-A-001.svg"
             (drawing/drawing-sheet-svg
              {:number "A-001" :name "General Arrangements" :size :a1 :revision "P01"
               :print-setting (:print-setting @state)
               :viewports [{:view plan :x 20 :y 20 :width 380 :height 260
                            :title (:name storey) :scale 100}
                           {:view section :x 430 :y 20 :width 380 :height 260
                            :title "Section A" :scale 50}]})])
          [(str "floor-plan-" (:id storey) ".svg")
           (drawing/documented-floor-plan-svg storey view-options)])]
    (case format
      :dxf (let [{:keys [filename media-type content]}
                 (integration/export-drawing model (:active-storey @state) :dxf
                                             (:drawing-views @state))]
             (download-text! filename media-type content))
      :pdf (let [{:keys [filename media-type content]}
                 (integration/export-drawing model (:active-storey @state) :pdf
                                             (:drawing-views @state)
                                             (:print-setting @state)
                                             (not-empty (:drawing-set @state)))]
             (download-bytes! filename media-type content))
      (when content (download-text! filename "image/svg+xml" content)))))
(defn- import-ifc! [event]
  (when-let [file (aget (.. event -target -files) 0)]
    (-> (.text file) (.then #(apply-project! (integration/import-ifc-spf %))))))
(defn- import-project! [event]
  (when-let [file (aget (.. event -target -files) 0)] (-> (.text file) (.then #(apply-project! (reader/read-string %))))))
(defn- invoke-shortcut! [event]
  (when-not (editable-target? event)
    (let [key (.toLowerCase (.-key event)) ctrl (or (.-ctrlKey event) (.-metaKey event)) profile (:profile @state)]
      (cond
        (and ctrl (= key "z") (.-shiftKey event)) (do (.preventDefault event) (.click (.getElementById js/document "redo")))
        (and ctrl (= key "z")) (do (.preventDefault event) (.click (.getElementById js/document "undo")))
        (= profile :revit)
        (when (re-matches #"[a-z]" key)
          (let [buffer (str (:shortcut-buffer @state) key) command (get revit-shortcuts buffer)
                prefix? (some #(.startsWith % buffer) (keys revit-shortcuts))]
            (.preventDefault event)
            (cond command (do (swap! state assoc :shortcut-buffer "") (.click (.getElementById js/document command)))
                  prefix? (swap! state assoc :shortcut-buffer buffer)
                  :else (swap! state assoc :shortcut-buffer key))))
        :else (when-let [command (get-in profile-shortcuts [profile key])]
                (.preventDefault event) (.click (.getElementById js/document command)))))))
(defn- show-drag-measure! [delta evidence]
  (let [[dx dy dz] delta
        length (js/Math.hypot dx dy dz)
        overlay (.getElementById js/document "drag-measure")]
    (set! (.-display (.-style overlay)) "block")
    (set! (.-textContent overlay)
          (str "Δ " (.toFixed length 3) " m · X " (.toFixed dx 3)
               " · Y " (.toFixed dy 3) " · Z " (.toFixed dz 3)
               (when evidence (str " · " (name (:snap/kind evidence))))))))
(defn- hide-drag-measure! []
  (set! (.-display (.-style (.getElementById js/document "drag-measure"))) "none"))
(defn- show-snap-marker! [canvas client-x client-y evidence]
  (let [marker (.getElementById js/document "snap-marker")]
    (if evidence
      (let [rect (.getBoundingClientRect canvas)]
        (set! (.-display (.-style marker)) "block")
        (set! (.-left (.-style marker)) (str (- client-x (.-left rect)) "px"))
        (set! (.-top (.-style marker)) (str (- client-y (.-top rect)) "px"))
        (set! (.-title marker) (name (:snap/kind evidence))))
      (set! (.-display (.-style marker)) "none"))))
(defn- hide-snap-marker! []
  (set! (.-display (.-style (.getElementById js/document "snap-marker"))) "none"))
(defn- show-selection-box! [canvas [start-x start-y] [current-x current-y]]
  (let [rect (.getBoundingClientRect canvas) box (.getElementById js/document "selection-box")
        left (- (min start-x current-x) (.-left rect))
        top (- (min start-y current-y) (.-top rect))]
    (set! (.-display (.-style box)) "block")
    (set! (.-left (.-style box)) (str left "px"))
    (set! (.-top (.-style box)) (str top "px"))
    (set! (.-width (.-style box)) (str (js/Math.abs (- current-x start-x)) "px"))
    (set! (.-height (.-style box)) (str (js/Math.abs (- current-y start-y)) "px"))))
(defn- hide-selection-box! []
  (set! (.-display (.-style (.getElementById js/document "selection-box"))) "none"))
(defn ^:export init! [] (let [canvas (.getElementById js/document "gpu-canvas") drag (atom nil)]
 (-> (gpu/init-canvas! canvas) (.then (fn [v] (reset! viewport v) (refresh!) (set! (.-textContent (.getElementById js/document "gpu-status")) "") (draw!))))
 (.addEventListener (.getElementById js/document "profile") "change"
                    #(do (swap! state assoc :profile (keyword (.. % -target -value)) :shortcut-buffer "")
                         (set! (.-textContent (.getElementById js/document "profile-hint"))
                               (case (:profile @state) :archicad "W Wall · D Door · N Window · L Level · F Floor"
                                     :vectorworks "2 Wall · D Door · W Window · L Layer · F Floor"
                                     "WA Wall · DR Door · WN Window · LL Level · FL Floor")) (refresh!)))
 (.addEventListener js/window "keydown"
                    (fn [event]
                      (if (and (= "Escape" (.-key event)) @drag)
                        (do (.preventDefault event)
                            (when-let [project (:original-project @drag)]
                              (swap! state interaction/cancel-drag-state project))
                            (reset! drag nil) (hide-drag-measure!) (hide-snap-marker!)
                            (hide-selection-box!) (refresh!))
                        (invoke-shortcut! event))))
 (.addEventListener (.getElementById js/document "analyze-structure") "click" analyze-structure!)
 (.addEventListener (.getElementById js/document "assign-structural-role") "click"
                    assign-structural-role!)
 (.addEventListener (.getElementById js/document "generate-structural-model") "click"
                    generate-structural-model!)
 (.addEventListener (.getElementById js/document "generate-structural-loads") "click"
                    generate-structural-loads!)
 (.addEventListener (.getElementById js/document "run-structural-analysis") "click"
                    run-structural-analysis!)
 (.addEventListener (.getElementById js/document "run-3d-frame-analysis") "click"
                    run-3d-frame-analysis!)
 (.addEventListener (.getElementById js/document "route-pipe") "click" route-pipe!)
 (.addEventListener (.getElementById js/document "design-mep-network") "click"
                    design-mep-network!)
 (.addEventListener (.getElementById js/document "design-closed-loop-mep-network") "click"
                    design-closed-loop-mep-network!)
 (.addEventListener (.getElementById js/document "route-duct") "click" route-duct!)
 (.addEventListener (.getElementById js/document "design-electrical-panel") "click"
                    design-electrical-panel!)
 (.addEventListener (.getElementById js/document "analyze-electrical-distribution") "click"
                    analyze-electrical-distribution!)
 (.addEventListener (.getElementById js/document "add-mep-equipment") "click"
                    add-mep-equipment!)
 (.addEventListener (.getElementById js/document "connect-mep-elements") "click"
                    connect-mep-elements!)
 (.addEventListener (.getElementById js/document "save-drawing-annotation") "click"
                    save-drawing-annotation!)
 (.addEventListener (.getElementById js/document "new-drawing-annotation") "click"
                    new-drawing-annotation!)
 (.addEventListener (.getElementById js/document "auto-tag-elements") "click"
                    auto-tag-elements!)
 (.addEventListener (.getElementById js/document "apply-view-range") "click"
                    apply-view-range!)
 (.addEventListener (.getElementById js/document "apply-print-setting") "click"
                    apply-print-setting!)
 (.addEventListener (.getElementById js/document "generate-drawing-set") "click"
                    generate-drawing-set!)
 (.addEventListener (.getElementById js/document "family-definition") "change" refresh-family-types!)
 (refresh-family-definitions!)
 (refresh-annotation-families!)
 (refresh-family-types!)
 (.addEventListener (.getElementById js/document "save-family-definition") "click"
                    #(try
                       (let [id (.-value (.getElementById js/document "family-id"))
                             existing (get-in @state [:family-catalog :family-catalog/families
                                                      (family-editor/family-key id)])
                             base-definition
                             (family-editor/box-family
                              {:id id
                               :name (.-value (.getElementById js/document "family-name"))
                               :category (keyword (.-value (.getElementById js/document "family-category")))
                               :width (num "family-width") :depth (num "family-depth")
                               :height (num "family-height")
                               :material (.-value (.getElementById js/document "family-material"))
                               :shared? (.-checked (.getElementById js/document "family-shared"))
                               :types (:family/types existing)})
                             definition
                             (family-editor/apply-parametric-schema
                              base-definition
                              {:parameters (edn-field "family-parameters"
                                                      (:family/parameters base-definition))
                               :lookup-tables (edn-field "family-lookup-tables" {})
                               :formulas (edn-field "family-formulas" {})
                               :reference-planes
                               (edn-field "family-reference-planes" {})
                               :constraints (edn-field "family-constraints" [])
                               :sketches (edn-field "family-sketches" {})
                               :template (edn-field "family-template"
                                                    (:family/template base-definition))})]
                         (swap! state update :family-catalog family-editor/upsert-family definition)
                         (refresh-family-definitions!)
                         (set! (.-value (.getElementById js/document "family-definition"))
                               (:family/id definition))
                         (refresh-family-types!)
                         (set! (.-textContent (.getElementById js/document "family-status"))
                               (str "Saved " (:family/name definition))))
                       (catch :default error
                         (set! (.-textContent (.getElementById js/document "family-status"))
                               (str "Error: " (.-message error))))))
 (.addEventListener (.getElementById js/document "save-family-type") "click"
                    #(try
                       (let [family-id (.-value (.getElementById js/document "family-definition"))
                             type-name (.-value (.getElementById js/document "family-type-name"))]
                         (swap! state update :family-catalog family-editor/upsert-type
                                family-id type-name (num "family-width"))
                         (refresh-family-types!)
                         (set! (.-value (.getElementById js/document "family-type"))
                               (family-editor/family-key type-name))
                         (set! (.-textContent (.getElementById js/document "family-status"))
                               (str "Saved type " type-name)))
                       (catch :default error
                         (set! (.-textContent (.getElementById js/document "family-status"))
                               (str "Error: " (.-message error))))))
 (.addEventListener (.getElementById js/document "add-family-instance") "click"
                    #(let [id (:next-id @state)
                           family-id (.-value (.getElementById js/document "family-definition"))
                           type-key (keyword (.-value (.getElementById js/document "family-type")))
                           definition (selected-family)
                           params (:family/parameters definition)
                           overrides (cond-> {}
                                       (contains? params :depth) (assoc :depth (num "family-depth"))
                                       (contains? params :height) (assoc :height (num "family-height"))
                                       (contains? params :finish)
                                       (assoc :finish (.-value (.getElementById js/document "family-material"))))
                           instance (family/instantiate-family-type
                                     (:family-catalog @state) family-id type-key id
                                     overrides
                                     {:detail-level
                                      (keyword (.-value (.getElementById js/document "family-detail")))})]
                       (swap! state update :next-id inc)
                       (select-only! id)
                       (commit! (bim/add-element (:project @state) (:active-storey @state) instance))))
 (.addEventListener (.getElementById js/document "add-wall") "click" #(let [id (:next-id @state) storey-id (:active-storey @state)
                                                                                  z (:elevation (bim/find-storey (:project @state) storey-id))
                                                                                  y (+ 7 (- id 14)) w (wall id [0 y z] [4 y z])]
                                                                              (swap! state update :next-id inc) (select-only! id)
                                                                              (commit! (bim/add-element (:project @state) storey-id w))))
 (.addEventListener (.getElementById js/document "add-level") "click"
                    #(let [id (:next-storey-id @state) elevation (+ 3.2 (reduce max 0 (map :elevation (storeys))))
                           level (bim/storey {:id id :name (str "Level " (dec id)) :elevation elevation
                                              :height 3.2 :placement :identity :spaces [] :elements []})]
                       (swap! state assoc :next-storey-id (inc id) :active-storey id
                              :selected nil :selection #{})
                       (commit! (bim/add-storey (:project @state) 2 level))))
 (.addEventListener (.getElementById js/document "add-slab") "click"
                    #(let [id (:next-id @state) storey (bim/find-storey (:project @state) (:active-storey @state)) z (:elevation storey)
                           slab (bim/slab {:id id :name (str "Floor " (:name storey))
                                           :boundary [[0 0 z] [8 0 z] [8 6 z] [0 6 z]] :thickness 0.25})]
                       (swap! state assoc :next-id (inc id) :selected id :selection #{id})
                       (commit! (bim/add-element (:project @state) (:active-storey @state) slab))))
 (.addEventListener (.getElementById js/document "add-roof") "click"
                    (fn [_]
                      (authoring-commit!
                       "Roof added"
                       (fn []
                         (let [id (:next-id @state)
                               storey (bim/find-storey (:project @state) (:active-storey @state))
                               z (+ (:elevation storey) (:height storey))
                               width (num "roof-width") depth (num "roof-depth")
                               roof (bim/gable-roof
                                     {:id id :name (str "Roof " id)
                                      :boundary [[0 0 z] [width 0 z]
                                                 [width depth z] [0 depth z]]
                                      :slope-rad (* (num "roof-slope") (/ js/Math.PI 180.0))
                                      :thickness (num "roof-thickness")})]
                           (swap! state assoc :next-id (inc id) :selected id :selection #{id})
                           (bim/add-element (:project @state) (:active-storey @state) roof))))))
 (.addEventListener (.getElementById js/document "add-stair") "click"
                    (fn [_]
                      (authoring-commit!
                       "Stair added"
                       (fn []
                         (let [id (:next-id @state)
                               storey (bim/find-storey (:project @state) (:active-storey @state))
                               stair (bim/straight-stair
                                      {:id id :name (str "Stair " id)
                                       :start [0.0 0.0 (:elevation storey)]
                                       :direction [1.0 0.0 0.0]
                                       :width (num "stair-width")
                                       :run-length (num "stair-run")
                                       :total-rise (num "stair-rise")
                                       :riser-count (num "stair-risers")})]
                           (swap! state assoc :next-id (inc id) :selected id :selection #{id})
                           (bim/add-element (:project @state) (:active-storey @state) stair))))))
 (.addEventListener (.getElementById js/document "add-railing") "click"
                    (fn [_]
                      (authoring-commit!
                       "Railing added"
                       (fn []
                         (let [id (:next-id @state)
                               storey (bim/find-storey (:project @state) (:active-storey @state))
                               path (mapv #(update % 2 + (:elevation storey))
                                          (parse-path "railing-path"))
                               railing (bim/path-railing
                                        {:id id :name (str "Railing " id) :path path
                                         :height (num "railing-height")
                                         :post-spacing (num "railing-spacing")})]
                           (swap! state assoc :next-id (inc id) :selected id :selection #{id})
                           (bim/add-element (:project @state) (:active-storey @state) railing))))))
 (.addEventListener (.getElementById js/document "add-curtain") "click"
                    (fn [_]
                      (authoring-commit!
                       "Curtain wall added"
                       (fn []
                         (let [id (:next-id @state)
                               storey (bim/find-storey (:project @state) (:active-storey @state))
                               z (:elevation storey) width (num "curtain-width")
                               curtain (bim/curtain-wall
                                        {:id id :name (str "Curtain Wall " id)
                                         :start [0.0 0.0 z] :end [width 0.0 z]
                                         :height (num "curtain-height")
                                         :columns (num "curtain-columns")
                                         :rows (num "curtain-rows")})]
                           (swap! state assoc :next-id (inc id) :selected id :selection #{id})
                           (bim/add-element (:project @state) (:active-storey @state) curtain))))))
 (.addEventListener (.getElementById js/document "auto-rooms") "click"
                    (fn [_]
                      (authoring-commit!
                       "Enclosed rooms generated"
                       (fn []
                         (let [storey-id (:active-storey @state)
                               storey (bim/find-storey (:project @state) storey-id)
                               boundaries (bim/enclosed-wall-boundaries (:elements storey))
                               boundary-key (fn [boundary]
                                              (set (map (fn [point]
                                                          (mapv #(js/Math.round (* 1.0e6 %)) point))
                                                        boundary)))
                               existing-boundaries (set (map (comp boundary-key :boundary)
                                                             (:spaces storey)))
                               missing (vec (remove #(contains? existing-boundaries
                                                               (boundary-key %)) boundaries))
                               first-id (:next-space-id @state)
                               identities (mapv (fn [offset]
                                                  (let [id (+ first-id offset)]
                                                    {:id id :name (str "Room " id) :label (str id)}))
                                                (range (count missing)))
                               rooms (mapv (fn [boundary identity]
                                             (bim/room-space
                                              {:id (:id identity) :name (:name identity)
                                               :label (:label identity) :category :other
                                               :boundary boundary :height (:height storey)}))
                                           missing identities)]
                           (when (seq rooms)
                             (swap! state update :next-space-id + (count rooms))
                             (reduce #(bim/add-space %1 storey-id %2)
                                     (:project @state) rooms)))))))
 (.addEventListener (.getElementById js/document "add-room") "click"
                    #(let [id (:next-space-id @state) storey (bim/find-storey (:project @state) (:active-storey @state))
                           z (:elevation storey) width (max 0.1 (num "room-width")) depth (max 0.1 (num "room-depth"))
                           room (bim/room-space {:id id :name (str "Room " id) :label (str id) :category :residential
                                                 :boundary [[0 0 z] [width 0 z] [width depth z] [0 depth z]] :height (:height storey)})]
                       (swap! state assoc :next-space-id (inc id) :selected-space id)
                       (commit! (bim/add-space (:project @state) (:active-storey @state) room))))
 (.addEventListener (.getElementById js/document "apply-room") "click"
                    #(when-let [id (:selected-space @state)]
                       (let [old (first (filter (fn [s] (= id (:id s))) (spaces)))
                             room (bim/room-space {:id id :name (.-value (.getElementById js/document "room-name"))
                                                   :label (:label old) :category (keyword (.-value (.getElementById js/document "room-category")))
                                                   :boundary (:boundary old) :height (:height old)})]
                         (commit! (bim/update-space (:project @state) (:active-storey @state) id (constantly room))))))
 (.addEventListener (.getElementById js/document "delete-room") "click"
                    #(when-let [id (:selected-space @state)]
                       (swap! state assoc :selected-space nil)
                       (commit! (bim/delete-space (:project @state) (:active-storey @state) id))))
 (doseq [[button-id kind] [["add-door" :door] ["add-window" :window]]]
   (.addEventListener (.getElementById js/document button-id) "click"
                      #(when-let [host (selected)]
                         (when (= :wall (:kind host))
                           (let [id (:next-id @state) opening-id (+ 10000 id)
                                 width (if (= kind :door) 0.9 1.2) height (if (= kind :door) 2.1 1.2)
                                 sill (if (= kind :door) 0 0.9)
                                 length (get-in host [:quantities :length-m]) offset (max 0.1 (/ (- length width) 2))
                                 opening (bim/rectangular-opening {:id opening-id :offset offset :sill sill :width width :height height :filled-by id})
                                 hosted (bim/add-opening-to-wall host opening)
                                 fill ((if (= kind :door) bim/door bim/window) {:id id :host-id (:id host) :opening-id opening-id})
                                 storey-id (:active-storey @state)
                                 p (-> (:project @state)
                                       (bim/update-element storey-id (:id host) (constantly hosted))
                                       (bim/add-element storey-id fill))]
                             (swap! state update :next-id inc)
                             (select-only! id)
                             (commit! p))))))
 (.addEventListener (.getElementById js/document "apply") "click" #(when-let [e (selected)] (let [[[x y z] _] (get-in e [:geometry :axis]) len (num "length") updated (bim/wall {:id (:id e) :name (.-value (.getElementById js/document "name")) :start [x y z] :end [(+ x len) y z] :height (num "height") :thickness (num "thickness") :material (.-value (.getElementById js/document "material"))})] (commit! (bim/update-element (:project @state) (:active-storey @state) (:id e) (constantly updated))))))
 (.addEventListener (.getElementById js/document "move-element") "click"
                    #(let [chosen (filterv bim/element-mesh (selected-elements))]
                       (when (seq chosen)
                         (let [delta (snap-delta chosen
                                                 [(num "move-x") (num "move-y") (num "move-z")])]
                           (commit! (transform-project (:project @state) chosen
                                                       bim/translate-element delta))))))
 (.addEventListener (.getElementById js/document "copy-element") "click"
                    #(let [chosen (filterv bim/element-mesh (selected-elements))]
                       (when (seq chosen)
                         (let [start-id (:next-id @state)
                               delta (snap-delta chosen
                                                 [(num "move-x") (num "move-y") (num "move-z")])
                               copies (mapv (fn [offset element]
                                              (bim/duplicate-element element (+ start-id offset) delta))
                                            (range) chosen)
                               p (reduce (fn [project [source copy]]
                                           (bim/add-element project
                                                            (element-storey-id (:id source)) copy))
                                         (:project @state) (map vector chosen copies))
                               ids (set (map :id copies))]
                           (swap! state assoc :next-id (+ start-id (count copies))
                                  :selected (:id (first copies)) :selection ids)
                           (commit! p)))))
 (.addEventListener (.getElementById js/document "rotate-element") "click"
                    #(let [chosen (filterv bim/element-mesh (selected-elements))]
                       (when (seq chosen)
                         (let [pivot [(num "pivot-x") (num "pivot-y") (num "pivot-z")]
                               angle (* (num "transform-angle") (/ js/Math.PI 180.0))]
                           (commit! (transform-project (:project @state) chosen
                                                       bim/rotate-element-z pivot angle))))))
 (.addEventListener (.getElementById js/document "mirror-element") "click"
                    #(let [chosen (filterv bim/element-mesh (selected-elements))]
                       (when (seq chosen)
                         (let [origin [(num "pivot-x") (num "pivot-y") (num "pivot-z")]
                               normal [(num "mirror-x") (num "mirror-y") (num "mirror-z")]]
                           (commit! (transform-project (:project @state) chosen
                                                       bim/mirror-element origin normal))))))
 (.addEventListener (.getElementById js/document "array-element") "click"
                    #(when-let [e (selected)]
                       (when (bim/element-mesh e)
                         (let [count-value (num "array-count")]
                           (when (and (js/Number.isInteger count-value) (<= 1 count-value 100))
                             (let [start-id (:next-id @state)
                                   identities (mapv (fn [offset]
                                                      (let [id (+ start-id offset)]
                                                        {:id id :global-id (str id)}))
                                                    (range count-value))
                                   kind (keyword (.-value (.getElementById js/document "array-kind")))
                                   copies (if (= :radial kind)
                                            (bim/radial-array
                                             e identities
                                             [(num "pivot-x") (num "pivot-y") (num "pivot-z")]
                                             (* (num "transform-angle") (/ js/Math.PI 180.0)))
                                            (bim/linear-array
                                             e identities
                                             (snap-delta [e] [(num "move-x") (num "move-y")
                                                              (num "move-z")])))
                                   p (reduce (fn [project copy]
                                               (bim/add-element project (:active-storey @state) copy))
                                             (:project @state) copies)]
                               (swap! state assoc :next-id (+ start-id count-value)
                                      :selected (:id (first copies))
                                      :selection (set (map :id copies)))
                               (commit! p)))))))
 (.addEventListener (.getElementById js/document "align-elements") "click"
                    (fn [_]
                      (authoring-commit!
                       "Aligned"
                       (fn []
                         (let [chosen (selected-elements) reference (selected)]
                           (when (= 2 (count chosen))
                             (let [moving (first (remove #(= (:id reference) (:id %)) chosen))
                                   aligned (bim/align-element
                                            reference moving
                                            {:axis (keyword (.-value (.getElementById js/document "align-axis")))
                                             :reference-anchor
                                             (keyword (.-value (.getElementById js/document "align-reference-anchor")))
                                             :moving-anchor
                                             (keyword (.-value (.getElementById js/document "align-moving-anchor")))})]
                               (bim/update-element (:project @state)
                                                   (element-storey-id (:id moving)) (:id moving)
                                                   (constantly aligned)))))))))
 (.addEventListener (.getElementById js/document "offset-walls") "click"
                    (fn [_]
                      (authoring-commit!
                       "Walls offset"
                       (fn []
                         (let [walls (filterv #(= :wall (:kind %)) (selected-elements))]
                           (when (seq walls)
                             (transform-project (:project @state) walls bim/offset-wall
                                                (num "wall-offset"))))))))
 (doseq [[button-id label operation]
         [["trim-walls" "Walls trimmed / extended" bim/trim-extend-walls]
          ["join-walls" "Walls joined" bim/join-walls]]]
   (.addEventListener (.getElementById js/document button-id) "click"
                      (fn [_]
                        (authoring-commit!
                         label
                         (fn []
                           (let [walls (filterv #(= :wall (:kind %)) (selected-elements))]
                             (when (= 2 (count walls))
                               (when-let [[left right]
                                          (if (= button-id "join-walls")
                                            (operation
                                             (first walls) (second walls)
                                             {:style (keyword (.-value (.getElementById js/document
                                                                                         "wall-join-style")))
                                              :priority (keyword (.-value (.getElementById js/document
                                                                                            "wall-join-priority")))})
                                            (operation (first walls) (second walls)))]
                                 (-> (:project @state)
                                     (bim/update-element (element-storey-id (:id left)) (:id left)
                                                         (constantly left))
                                     (bim/update-element (element-storey-id (:id right)) (:id right)
                                                         (constantly right)))))))))))
 (.addEventListener (.getElementById js/document "apply-wall-layers") "click"
                    (fn [_]
                      (authoring-commit!
                       "Wall layers applied"
                       (fn []
                         (let [walls (filterv #(= :wall (:kind %)) (selected-elements))
                               layers (parse-wall-layers)]
                           (when (seq walls)
                             (transform-project (:project @state) walls
                                                bim/set-wall-layers layers)))))))
 (.addEventListener (.getElementById js/document "apply-slab-layers") "click"
                    (fn [_]
                      (authoring-commit!
                       "Slab layers applied"
                       (fn []
                         (let [slabs (filterv #(= :slab (:kind %)) (selected-elements))
                               layers (parse-material-layers "slab-layers")]
                           (when (seq slabs)
                             (transform-project (:project @state) slabs
                                                bim/set-slab-layers layers)))))))
 (.addEventListener (.getElementById js/document "apply-slab-shape") "click"
                    (fn [_]
                      (authoring-commit!
                       "Slab shape applied"
                       (fn []
                         (let [slabs (filterv #(= :slab (:kind %)) (selected-elements))
                               elevations (mapv #(js/parseFloat (string/trim %))
                                                (string/split
                                                 (.-value (.getElementById js/document
                                                                           "slab-elevations"))
                                                 #","))]
                           (when (seq slabs)
                             (transform-project (:project @state) slabs
                                                bim/set-slab-vertex-elevations elevations)))))))
 (.addEventListener (.getElementById js/document "add-shaft-opening") "click"
                    (fn [_]
                      (authoring-commit!
                       "Shaft opening added"
                       (fn []
                         (when-let [floor (first (filter #(= :slab (:kind %))
                                                        (selected-elements)))]
                           (let [id (:next-id @state) x (num "shaft-x") y (num "shaft-y")
                                 width (num "shaft-width") depth (num "shaft-depth")
                                 z (nth (first (get-in floor [:geometry :boundary])) 2)
                                 opening (bim/slab-opening
                                          {:id id :boundary [[x y z] [(+ x width) y z]
                                                             [(+ x width) (+ y depth) z]
                                                             [x (+ y depth) z]]})
                                 project (bim/update-element
                                          (:project @state) (element-storey-id (:id floor))
                                          (:id floor) bim/add-opening-to-slab opening)]
                             (swap! state update :next-id inc)
                             project))))))
 (.addEventListener (.getElementById js/document "rehost-element") "click"
                    (fn [_]
                      (authoring-commit!
                       "Hosted element moved"
                       (fn []
                         (let [chosen (selected-elements)
                               hosted (first (filter #(contains? #{:door :window} (:kind %)) chosen))
                               host (first (filter #(= :wall (:kind %)) chosen))]
                           (when (and hosted host)
                             (bim/rehost-wall-element
                              (:project @state) (element-storey-id (:id hosted))
                              (:id hosted) (:id host)
                              {:offset (num "host-offset") :sill (num "host-sill")})))))))
 (.addEventListener (.getElementById js/document "delete") "click"
                    #(when-let [e (selected)]
                       (let [p (if (#{:door :window} (:kind e))
                                 (let [[host-id opening-id] (:connected-to e)]
                                   (-> (:project @state)
                                       (bim/update-element (:active-storey @state) host-id bim/remove-opening-from-wall opening-id)
                                       (bim/delete-element (:active-storey @state) (:id e))))
                                 (bim/delete-element (:project @state) (:active-storey @state) (:id e)))]
                         (commit! p)
                         (select-only! (some-> (first (elements)) :id))
                         (refresh!))))
 (.addEventListener (.getElementById js/document "undo") "click" #(when-let [p (peek (:history @state))] (swap! state (fn [s] (assoc s :project p :history (pop (:history s)) :future (conj (:future s) (:project s))))) (refresh!)))
 (.addEventListener (.getElementById js/document "redo") "click" (fn [] (when-let [p (peek (:future @state))] (swap! state (fn [s] (assoc s :project p :future (pop (:future s)) :history (conj (:history s) (:project s))))) (refresh!))))
 (.addEventListener canvas "pointerdown"
                    (fn [event]
                      (let [client-x (.-clientX event) client-y (.-clientY event)
                            tool (keyword (.-value (.getElementById js/document "viewport-tool")))
                            mode (selection-mode event)]
                        (case tool
                          :move
                          (if-let [hit (hit-at canvas client-x client-y)]
                            (let [element (:element hit) element-id (:element/id hit)
                                  bounds (:pick/bounds hit)
                                  plane-z (/ (+ (get-in bounds [:min 2])
                                                (get-in bounds [:max 2])) 2.0)
                                  plane {:plane/origin [0.0 0.0 plane-z]
                                         :plane/normal [0.0 0.0 1.0]}
                                  ray (viewport-ray-at canvas client-x client-y)]
                              (if (not= :replace mode)
                                (do (apply-selection!
                                     (interaction/selection-after-click
                                      (selection-ids) element-id mode))
                                    (reset! drag nil))
                                (do (when-not (contains? (selection-ids) element-id)
                                      (select-only! element-id))
                                    (swap! state assoc :selected element-id
                                           :active-storey (element-storey-id element-id))
                                    (if (interaction/ray-plane-point ray plane)
                                      (reset! drag {:mode :move :start [client-x client-y]
                                                    :moved? false :start-ray ray :plane plane
                                                    :elements (selected-elements)
                                                    :anchor element
                                                    :original-project (:project @state)})
                                      (reset! drag nil))))
                              (refresh!))
                            (do (when (= :replace mode) (select-only! nil)) (refresh!)))
                          :box
                          (reset! drag {:mode :box :start [client-x client-y]
                                        :last [client-x client-y] :moved? false
                                        :selection-mode mode})
                          (reset! drag {:mode :orbit :start [client-x client-y]
                                        :last [client-x client-y] :moved? false
                                        :selection-mode mode})))))
 (.addEventListener js/window "pointerup"
                    (fn [_]
                      (when-let [{:keys [mode start moved? original-project selection-mode]} @drag]
                        (case mode
                          :orbit (when-not moved?
                                   (pick-at! canvas (first start) (second start) selection-mode))
                          :box (when moved?
                                 (let [[start-x start-y] start
                                       current (:last @drag)
                                       [current-x _] current
                                       a (viewport-ndc-at canvas start-x start-y)
                                       b (viewport-ndc-at canvas (first current) (second current))
                                       rect (.getBoundingClientRect canvas)
                                       {:keys [eye target]} (camera-state)
                                       ids (interaction/elements-in-screen-rect
                                            (all-elements)
                                            {:eye eye :target target
                                             :aspect (/ (.-width rect)
                                                        (max 1.0 (.-height rect)))}
                                            {:min (mapv min a b) :max (mapv max a b)}
                                            (if (>= current-x start-x) :window :crossing))]
                                   (apply-selection!
                                    (interaction/selection-after-box
                                     (selection-ids) ids selection-mode))
                                   (refresh!)))
                          :move (when moved?
                                  (swap! state interaction/commit-drag-state original-project)
                                  (refresh!))))
                      (reset! drag nil) (hide-drag-measure!) (hide-snap-marker!)
                      (hide-selection-box!)))
 (.addEventListener canvas "dblclick" (fn [event] (.preventDefault event) (frame-selected!)))
 (.addEventListener js/window "pointermove"
                    (fn [e]
                      (when-let [{:keys [mode start moved? start-ray plane elements anchor
                                         original-project]
                                  [x y] :last} @drag]
                        (let [[start-x start-y] start
                              client-x (.-clientX e) client-y (.-clientY e)
                              passed-threshold? (> (js/Math.hypot (- client-x start-x)
                                                                  (- client-y start-y)) 3.0)]
                          (case mode
                            :orbit
                            (do (swap! state update :azimuth + (* 0.008 (- client-x x)))
                                (swap! state update :elevation
                                       (fn [v] (max -1.2 (min 1.2
                                                             (+ v (* 0.008 (- client-y y)))))))
                                (swap! drag assoc :last [client-x client-y]
                                       :moved? (or moved? passed-threshold?)))
                            :box
                            (do (swap! drag assoc :last [client-x client-y]
                                      :moved? (or moved? passed-threshold?))
                                (when passed-threshold?
                                  (show-selection-box! canvas start [client-x client-y])))
                            :move
                            (when passed-threshold?
                              (when-let [motion (interaction/drag-delta
                                                 start-ray
                                                 (viewport-ray-at canvas client-x client-y)
                                                 plane)]
                                (let [delta (snap-delta elements (:drag/delta motion))
                                      preview (transform-project original-project elements
                                                                 bim/translate-element delta)]
                                  (swap! state assoc :project preview)
                                  (swap! drag assoc :moved? true :delta delta)
                                  (show-drag-measure! delta (:last-snap @state))
                                  (show-snap-marker! canvas client-x client-y (:last-snap @state))
                                  (refresh!)))))))))
 (.addEventListener js/window "pointercancel"
                    (fn [_]
                      (when-let [project (:original-project @drag)]
                        (swap! state interaction/cancel-drag-state project))
                      (reset! drag nil) (hide-drag-measure!) (hide-snap-marker!)
                      (hide-selection-box!) (refresh!)))
 (.addEventListener (.getElementById js/document "save-project") "click" save-project!)
 (.addEventListener (.getElementById js/document "load-project") "click" load-project!)
 (.addEventListener (.getElementById js/document "import") "click" #(.click (.getElementById js/document "import-file")))
 (.addEventListener (.getElementById js/document "import-file") "change" import-project!)
 (.addEventListener (.getElementById js/document "export-schedule") "click" download-schedule!)
 (.addEventListener (.getElementById js/document "run-clashes") "click" run-clash-check!)
 (.addEventListener (.getElementById js/document "export-clashes") "click" download-clashes!)
 (.addEventListener (.getElementById js/document "export-ifc") "click" download-ifc!)
 (.addEventListener (.getElementById js/document "publish-opencde") "click" publish-opencde!)
 (.addEventListener (.getElementById js/document "export-bcf") "click" download-bcf!)
 (.addEventListener (.getElementById js/document "refresh-stream") "click" refresh-stream!)
 (.addEventListener (.getElementById js/document "export-drawing") "click" download-drawing!)
 (.addEventListener (.getElementById js/document "import-ifc") "click" #(.click (.getElementById js/document "import-ifc-file")))
 (.addEventListener (.getElementById js/document "import-ifc-file") "change" import-ifc!)
 (.addEventListener (.getElementById js/document "export") "click" download-project!)
 (refresh-workspace-status!)))
