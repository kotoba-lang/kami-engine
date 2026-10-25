(ns kami.cad.app (:require [cljs.reader :as reader] [kotoba.lang.text :as string]
                           [kami.cad :as cad] [kami.cad.project :as project] [kami.cad.dimensional-sketch :as sketch] [kami.webgpu.mesh :as gpu]))
(defn sections [] [(cad/curve [[-2 0 0] [0 2 0] [2 0 0]] [1 1 1]) (cad/curve [[-2 0 2] [0 3 2] [2 0 2]] [1 1 1])])
(defn- feature-model [sections segments]
  (let [sources (mapv (fn [index section] (cad/feature (keyword (str "section-" index)) :source [] {:value section})) (range) sections)]
    (cad/feature-model (conj sources (cad/feature :loft :loft (mapv :feature/id sources) {:segments segments})))))
(defonce state (atom {:sections (sections) :segments 16 :selected-section 0 :selected-point 1
                      :history [] :future [] :azimuth 0.7 :elevation 0.45
                      :profile :rhino :last-command nil :command-status "Ready" :snap 0.001 :sketch-width 4.0 :sketch-height 2.0
                      :measurement-tolerance 0.000001 :feature-model (feature-model (sections) 16) :selected-feature :loft
                      :solid nil :solid-b nil :view-mode :loft :extrude-height 2.0
                      :project-id "untitled-cad" :project-name "Untitled CAD" :revision 0 :save-status :clean}))
(defonce viewport (atom nil))
(defn- mesh [] (if (and (= :solid (:view-mode @state)) (:solid @state))
                 (cad/solid-mesh (:solid @state))
                 (cad/loft-mesh (cad/loft (:sections @state) (:segments @state)))))
(defn- measurements []
  (let [selected (nth (:sections @state) (:selected-section @state))
        box (cad/bounds (mapcat #(cad/tessellate % 128) (:sections @state)))]
    {:length (cad/curve-length selected (:measurement-tolerance @state)) :bounds box}))
(defn- upload! []
  (when-let [v @viewport]
    (let [rebuilt (cad/recompute-feature-model (:feature-model @state))
          _ (swap! state assoc :feature-model rebuilt)
          surface (get-in rebuilt [:feature-model/results :loft])
          m (when surface (cad/loft-mesh surface)) measurement (measurements)]
      (when m (swap! viewport assoc :buffers (gpu/upload-mesh! (:mesh-context v) m)))
      (set! (.-textContent (.getElementById js/document "stats")) (str (count (:sections @state)) " sections · " (count (:positions m)) " vertices · " (/ (count (:indices m)) 3) " triangles"))
      (set! (.-textContent (.getElementById js/document "measurement-result"))
            (str "Length " (.toFixed (:length measurement) 6) " m · Size " (string/join " × " (map #(.toFixed % 4) (get-in measurement [:bounds :size]))) " m"))
      (set! (.-textContent (.getElementById js/document "debug-state"))
            (js/JSON.stringify (clj->js {:segments (:segments @state) :snap (:snap @state) :sectionCount (count (:sections @state))
                                         :selectedSection (:selected-section @state) :profile (name (:profile @state))
                                         :lastCommand (:last-command @state) :commandStatus (:command-status @state)
                                         :projectVersion project/current-version :revision (:revision @state) :saveStatus (name (:save-status @state))
                                         :measuredLength (:length measurement) :boundsSize (get-in measurement [:bounds :size])
                                         :featureStatuses (:feature-model/statuses rebuilt) :selectedFeature (name (:selected-feature @state))
                                         :viewMode (name (:view-mode @state)) :solidVolume (when-let [solid (:solid @state)] (cad/solid-volume solid))
                                         :sectionWidth (- (first (last (:cad/control-points (first (:sections @state)))))
                                                          (first (first (:cad/control-points (first (:sections @state))))))
                                         :controlPoints (mapv :cad/control-points (:sections @state))})))
      (let [tree (.getElementById js/document "feature-tree")]
        (set! (.-innerHTML tree) "")
        (doseq [feature (:feature-model/features rebuilt)]
          (let [button (.createElement js/document "button") id (:feature/id feature)
                status (get-in rebuilt [:feature-model/statuses id :status])]
            (set! (.-textContent button) (str (if (:feature/suppressed? feature) "⊘ " "◆ ") (name id) " · " (name status)))
            (when (= id (:selected-feature @state)) (.add (.-classList button) "primary"))
            (.addEventListener button "click" #(do (swap! state assoc :selected-feature id) (upload!)))
            (.appendChild tree button)))))))
(defn- commit! [sections] (swap! state (fn [s] (-> s (update :history conj (:sections s)) (assoc :sections sections :feature-model (feature-model sections (:segments s)) :future [] :save-status :dirty) (update :revision inc)))) (upload!))
(defn- draw! [] (when-let [{:keys [buffers] :as v} @viewport] (when buffers (let [{:keys [azimuth elevation]} @state d 8 eye [(* d (js/Math.cos elevation) (js/Math.cos azimuth)) (* d (js/Math.sin elevation)) (* d (js/Math.cos elevation) (js/Math.sin azimuth))]] (gpu/render-frame! v buffers eye [0 1 1] [0.45 0.7 1.0])))) (js/requestAnimationFrame draw!))
(defn- num [id] (js/parseFloat (.-value (.getElementById js/document id))))
(defn- sync-point-fields! []
  (let [{:keys [sections selected-section selected-point]} @state
        point (get-in sections [selected-section :cad/control-points selected-point])
        weight (get-in sections [selected-section :cad/weights selected-point])]
    (doseq [[id value] (map vector ["cp-x" "cp-y" "cp-z"] point)]
      (set! (.-value (.getElementById js/document id)) value))
    (set! (.-value (.getElementById js/document "weight")) weight)))
(defn- sync-section-options! []
  (let [select (.getElementById js/document "section-index") selected (:selected-section @state)]
    (set! (.-innerHTML select) "")
    (doseq [index (range (count (:sections @state)))]
      (let [option (.createElement js/document "option")]
        (set! (.-value option) index) (set! (.-textContent option) (str "Section " (inc index)))
        (.appendChild select option)))
    (set! (.-value select) selected)))
(defn- rounded-rect-solid
  "Extrude a width x depth rectangle into a watertight solid, rounding all
  four corners with `radius` (clamped to a safe fraction of the shorter side)
  when positive."
  [width depth height radius]
  (let [w (/ width 2) d (/ depth 2)]
    (if (<= radius 0)
      (cad/extrude-polygon [[(- w) (- d) 0] [w (- d) 0] [w d 0] [(- w) d 0]] [0 0 height])
      (let [base (cad/sketch [(cad/sketch-point :r0 (- w) (- d) true) (cad/sketch-point :r1 w (- d) true)
                              (cad/sketch-point :r2 w d true) (cad/sketch-point :r3 (- w) d true)]
                             [(cad/sketch-line :re0 :r0 :r1) (cad/sketch-line :re1 :r1 :r2)
                              (cad/sketch-line :re2 :r2 :r3) (cad/sketch-line :re3 :r3 :r0)]
                             [])
            clamped (min radius (* 0.2 (min width depth)))
            rounded (reduce (fn [s corner]
                              (cad/fillet-sketch s corner clamped
                                                 (keyword (str (name corner) "-fs"))
                                                 (keyword (str (name corner) "-fe"))
                                                 (keyword (str (name corner) "-farc"))))
                            base [:r0 :r1 :r2 :r3])]
        (cad/extrude-polygon (cad/sketch-loop->polygon rounded 8) [0 0 height])))))
(defn- offset-section [section dz]
  (update section :cad/control-points #(mapv (fn [[x y z]] [x y (+ z dz)]) %)))
(defn- duplicate-section! []
  (let [{:keys [sections selected-section]} @state source (nth sections selected-section)
        zs (mapcat #(map (fn [p] (nth p 2)) (:cad/control-points %)) sections)
        dz (max 0.1 (/ (- (reduce max zs) (reduce min zs)) (max 1 (dec (count sections)))))
        insert-at (inc selected-section) result (vec (concat (subvec sections 0 insert-at)
                                                             [(offset-section source dz)] (subvec sections insert-at)))]
    (swap! state assoc :selected-section insert-at) (commit! result) (sync-section-options!) (sync-point-fields!)))
(defn- delete-section! []
  (when (> (count (:sections @state)) 2)
    (let [index (:selected-section @state) result (vec (concat (subvec (:sections @state) 0 index)
                                                               (subvec (:sections @state) (inc index))))]
      (swap! state assoc :selected-section (min index (dec (count result))))
      (commit! result) (sync-section-options!) (sync-point-fields!))))
(defn- move-section! [delta]
  (let [index (:selected-section @state) target (+ index delta) sections (:sections @state)]
    (when (< -1 target (count sections))
      (let [a (nth sections index) b (nth sections target) result (assoc sections index b target a)]
        (swap! state assoc :selected-section target) (commit! result) (sync-section-options!) (sync-point-fields!)))))
(def command-aliases
  {:rhino {"trim" :trim "fit" :fit "zoom extents" :fit "reset" :reset "solvewidth" :solve-width "undo" :undo "redo" :redo}
   :autocad {"tr" :trim "trim" :trim "z" :fit "ze" :fit "regen" :reset "dim" :solve-width "u" :undo "redo" :redo}
   :fusion {"trim" :trim "fit" :fit "home" :fit "reset" :reset "dimension" :solve-width "undo" :undo "redo" :redo}
   :plasticity {"t" :trim "trim" :trim "f" :fit "fit" :fit "reset" :reset "d" :solve-width "undo" :undo "redo" :redo}})
(defn- run-command! [text]
  (let [normalized (-> text .trim .toLowerCase) command (get-in command-aliases [(:profile @state) normalized])
        button-id ({:trim "trim" :fit "fit" :reset "reset" :solve-width "solve-width" :undo "undo" :redo "redo"} command)]
    (if button-id
      (do (swap! state assoc :last-command normalized :command-status (str "Executed " normalized))
          (.click (.getElementById js/document button-id)))
      (swap! state assoc :command-status (str "Unknown command: " normalized)))
    (set! (.-textContent (.getElementById js/document "command-status")) (:command-status @state))
    (set! (.-value (.getElementById js/document "command")) "")
    (upload!)))
(defn- editable-target? [event]
  (let [target (.-target event) tag (some-> target .-tagName .toLowerCase)]
    (or (#{"input" "select" "textarea"} tag) (.-isContentEditable target))))
(def ^:private storage-key "kami.cad.project.v2")
(def ^:private backup-key "kami.cad.project.backup")
(defn- project-document []
  (let [{:keys [project-id project-name sections segments selected-section selected-point azimuth elevation snap sketch-width sketch-height measurement-tolerance profile feature-model solid view-mode]} @state]
    (project/document {:id project-id :name project-name :sections sections :tessellation segments
                       :selection {:section selected-section :point selected-point} :camera {:azimuth azimuth :elevation elevation}
                       :precision {:snap snap :sketch-width sketch-width :measurement-tolerance measurement-tolerance} :interaction {:profile profile}
                       :feature-model feature-model :solid solid :view-mode view-mode :sketch {:width sketch-width :height sketch-height}})))
(defn- save-project! []
  (let [data (pr-str (project-document)) old (.getItem js/localStorage storage-key)]
    (when old (.setItem js/localStorage backup-key old)) (.setItem js/localStorage storage-key data)
    (swap! state assoc :save-status :saved) (upload!)))
(defn- apply-project! [value]
  (let [p (project/open value) selection (:project/selection p) camera (:project/camera p)
        precision (:project/precision p) interaction (:project/interaction p)]
    (swap! state assoc :project-id (:project/id p) :project-name (:project/name p) :sections (:project/sections p)
           :segments (:project/tessellation p) :selected-section (:section selection) :selected-point (:point selection)
           :azimuth (:azimuth camera) :elevation (:elevation camera) :snap (:snap precision)
           :sketch-width (get-in p [:project/sketch :width]) :sketch-height (get-in p [:project/sketch :height])
           :measurement-tolerance (:measurement-tolerance precision 0.000001)
           :feature-model (or (:project/feature-model p) (feature-model (:project/sections p) (:project/tessellation p)))
           :solid (:project/solid p) :view-mode (:project/view-mode p :loft)
           :profile (:profile interaction) :history [] :future [] :save-status :saved)
    (doseq [[id value] [["segments" (:project/tessellation p)] ["section-index" (:section selection)] ["point-index" (:point selection)]
                        ["snap" (:snap precision)] ["sketch-width" (get-in p [:project/sketch :width])]
                        ["sketch-height" (get-in p [:project/sketch :height])]
                        ["measurement-tolerance" (:measurement-tolerance precision 0.000001)] ["profile" (name (:profile interaction))]]]
      (set! (.-value (.getElementById js/document id)) value))
    (sync-section-options!) (sync-point-fields!) (upload!)))
(defn- load-project! []
  (when-let [data (.getItem js/localStorage storage-key)]
    (try (apply-project! (reader/read-string data))
         (catch :default _ (when-let [backup (.getItem js/localStorage backup-key)] (apply-project! (reader/read-string backup)))))))
(defn- download-project! []
  (let [a (.createElement js/document "a") url (.createObjectURL js/URL (js/Blob. #js [(pr-str (project-document))] #js {:type "application/edn"}))]
    (set! (.-href a) url) (set! (.-download a) "design.kami-cad.edn") (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- import-project! [event]
  (when-let [file (aget (.. event -target -files) 0)] (-> (.text file) (.then #(apply-project! (reader/read-string %))))))
(defn- download-measurements! []
  (let [{:keys [length bounds]} (measurements) a (.createElement js/document "a")
        data (str "property,x,y,z\nmin," (string/join "," (:min bounds)) "\nmax," (string/join "," (:max bounds))
                  "\nsize," (string/join "," (:size bounds)) "\ncurve_length," length ",,")
        url (.createObjectURL js/URL (js/Blob. #js [data] #js {:type "text/csv;charset=utf-8"}))]
    (set! (.-href a) url) (set! (.-download a) "cad-measurements.csv") (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn ^:export init! []
 (let [canvas (.getElementById js/document "gpu-canvas") drag (atom nil)]
  (.setAttribute (.getElementById js/document "snap") "aria-label" "Precision snap increment")
  (-> (gpu/init-canvas! canvas) (.then (fn [v] (reset! viewport v) (upload!) (set! (.-textContent (.getElementById js/document "gpu-status")) "") (draw!))))
  (.addEventListener (.getElementById js/document "apply") "click"
                     #(let [inc (num "snap") p (cad/snap-point [(num "cp-x") (num "cp-y") (num "cp-z")] inc)
                            w (num "weight") {:keys [sections selected-section selected-point]} @state]
                        (commit! (update sections selected-section
                                         (fn [section] (-> section (cad/move-control-point selected-point p) (cad/set-weight selected-point w)))))))
  (doseq [id ["section-index" "point-index"]]
    (.addEventListener (.getElementById js/document id) "change"
                       #(do (swap! state assoc
                                   :selected-section (js/parseInt (.-value (.getElementById js/document "section-index")))
                                   :selected-point (js/parseInt (.-value (.getElementById js/document "point-index"))))
                            (sync-point-fields!))))
  (.addEventListener (.getElementById js/document "duplicate-section") "click" duplicate-section!)
  (.addEventListener (.getElementById js/document "delete-section") "click" delete-section!)
  (.addEventListener (.getElementById js/document "section-up") "click" #(move-section! -1))
  (.addEventListener (.getElementById js/document "section-down") "click" #(move-section! 1))
  (.addEventListener (.getElementById js/document "trim") "click"
                     #(let [t0 (num "trim-start") t1 (num "trim-end")]
                        (commit! (mapv (fn [section] (cad/trim-curve section t0 t1)) (:sections @state)))
                        (sync-point-fields!)))
  (.addEventListener (.getElementById js/document "solve-width") "click"
                     #(let [dimensions {:width (max 0.1 (num "sketch-width")) :height (max 0.1 (num "sketch-height"))}
                            {:keys [sections solver]} (sketch/solve-sections (:sections @state) dimensions)]
                        (set! (.-textContent (.getElementById js/document "solver-status"))
                              (if (:converged? solver)
                                (str "Fully constrained · 6 constraints · residual " (.toExponential (:max-residual solver) 2))
                                (str "Constraint conflict · residual " (.toExponential (:max-residual solver) 2))))
                        (swap! state assoc :sketch-width (:width dimensions) :sketch-height (:height dimensions))
                        (commit! sections)
                        (sync-point-fields!)))
  (.addEventListener (.getElementById js/document "reset") "click" #(do (swap! state assoc :selected-section 0) (commit! (sections)) (sync-section-options!) (sync-point-fields!)))
  (.addEventListener (.getElementById js/document "extrude-solid") "click"
                     #(let [width (max 0.1 (num "sketch-width")) depth (max 0.1 (num "solid-depth")) height (max 0.01 (num "extrude-height"))
                            radius (max 0 (num "corner-radius"))
                            solid (rounded-rect-solid width depth height radius)]
                        (swap! state assoc :solid solid :view-mode :solid :extrude-height height :save-status :dirty)
                        (set! (.-textContent (.getElementById js/document "solid-result"))
                              (str "Volume " (.toFixed (cad/solid-volume solid) 3) " m³ · watertight"))
                        (upload!)))
  (.addEventListener (.getElementById js/document "stash-solid-b") "click"
                     #(do (swap! state assoc :solid-b (:solid @state))
                          (set! (.-textContent (.getElementById js/document "solid-b-status"))
                                (if (:solid-b @state)
                                  (str "Solid B stashed · Volume " (.toFixed (cad/solid-volume (:solid-b @state)) 3) " m³")
                                  "No solid B"))))
  ;; Boolean ops are scoped to axis-aligned coaxial convex-polygon prisms
  ;; (see kami.cad's boolean-union/-difference/-intersect docstrings and
  ;; ADR-26071224xx); union/difference additionally require A and B to share
  ;; an identical cross-section profile. Each op returns a vector of 0-2
  ;; solids -- this UI shows only the first result (`(first results)`) since
  ;; the viewport can only display a single solid at a time.
  (.addEventListener (.getElementById js/document "apply-boolean") "click"
                     #(let [op (keyword (.-value (.getElementById js/document "boolean-op")))
                            a (:solid @state) b (:solid-b @state) result-el (.getElementById js/document "boolean-result")]
                        (if (and a b)
                          (try
                            (let [results (case op
                                            :intersect (cad/boolean-intersect a b)
                                            :union (cad/boolean-union a b)
                                            :difference (cad/boolean-difference a b))]
                              (if (seq results)
                                (do (swap! state assoc :solid (first results) :view-mode :solid :save-status :dirty)
                                    (set! (.-textContent result-el)
                                          (str (count results) " result solid(s) · showing 1st · Volume "
                                               (.toFixed (cad/solid-volume (first results)) 3) " m³"))
                                    (upload!))
                                (set! (.-textContent result-el) "Empty result (no overlap)")))
                            (catch :default e (set! (.-textContent result-el) (str "Unsupported: " (.-message e)))))
                          (set! (.-textContent result-el) "Need both Solid A (extrude) and a stashed Solid B"))))
  (.addEventListener (.getElementById js/document "show-loft") "click" #(do (swap! state assoc :view-mode :loft) (upload!)))
  (.addEventListener (.getElementById js/document "show-solid") "click" #(when (:solid @state) (swap! state assoc :view-mode :solid) (upload!)))
  (.addEventListener (.getElementById js/document "fit") "click" #(swap! state assoc :azimuth 0.7 :elevation 0.45))
  (.addEventListener (.getElementById js/document "segments") "input" #(do (swap! state assoc :segments (js/parseInt (.. % -target -value)) :save-status :dirty) (upload!)))
  (.addEventListener (.getElementById js/document "toggle-feature") "click"
                     #(let [id (:selected-feature @state)
                            feature (first (filter (fn [f] (= id (:feature/id f))) (get-in @state [:feature-model :feature-model/features])))]
                        (swap! state update :feature-model cad/suppress-feature id (not (:feature/suppressed? feature)))
                        (swap! state assoc :save-status :dirty) (upload!)))
  (.addEventListener (.getElementById js/document "recompute-features") "click" upload!)
  (.addEventListener (.getElementById js/document "snap") "change" #(swap! state assoc :snap (num "snap") :save-status :dirty))
  (.addEventListener (.getElementById js/document "sketch-width") "change" #(swap! state assoc :sketch-width (num "sketch-width") :save-status :dirty))
  (.addEventListener (.getElementById js/document "sketch-height") "change" #(swap! state assoc :sketch-height (num "sketch-height") :save-status :dirty))
  (.addEventListener (.getElementById js/document "measurement-tolerance") "change"
                     #(do (swap! state assoc :measurement-tolerance (max 1.0e-10 (num "measurement-tolerance"))) (upload!)))
  (.addEventListener (.getElementById js/document "export-measurements") "click" download-measurements!)
  (.addEventListener (.getElementById js/document "profile") "change"
                     #(do (swap! state assoc :profile (keyword (.. % -target -value)))
                          (set! (.-textContent (.getElementById js/document "profile-hint"))
                                (case (:profile @state) :autocad "TR Trim · Z/ZE Fit · DIM Solve"
                                      :fusion "Trim · Fit · Dimension"
                                      :plasticity "T Trim · F Fit · D Dimension"
                                      "Trim · Zoom Extents · SolveWidth")) (upload!)))
  (.addEventListener (.getElementById js/document "command") "keydown"
                     #(when (= "Enter" (.-key %)) (.preventDefault %) (run-command! (.. % -target -value))))
  (.addEventListener js/window "keydown"
                     #(when (and (not (editable-target? %)) (= "/" (.-key %)))
                        (.preventDefault %) (.focus (.getElementById js/document "command"))))
  (.addEventListener (.getElementById js/document "undo") "click" #(when-let [prev (peek (:history @state))] (swap! state (fn [s] (assoc s :sections prev :selected-section (min (:selected-section s) (dec (count prev))) :history (pop (:history s)) :future (conj (:future s) (:sections s))))) (sync-section-options!) (sync-point-fields!) (upload!)))
  (.addEventListener (.getElementById js/document "redo") "click" (fn [] (when-let [next (peek (:future @state))] (swap! state (fn [s] (assoc s :sections next :selected-section (min (:selected-section s) (dec (count next))) :future (pop (:future s)) :history (conj (:history s) (:sections s))))) (sync-section-options!) (sync-point-fields!) (upload!))))
  (.addEventListener canvas "pointerdown" #(reset! drag [(.-clientX %) (.-clientY %)])) (.addEventListener js/window "pointerup" #(reset! drag nil))
  (.addEventListener js/window "pointermove" (fn [e] (when-let [[x y] @drag] (swap! state update :azimuth + (* 0.008 (- (.-clientX e) x))) (swap! state update :elevation #(max -1.2 (min 1.2 (+ % (* 0.008 (- (.-clientY e) y)))))) (reset! drag [(.-clientX e) (.-clientY e)]))))
  (.addEventListener (.getElementById js/document "save-project") "click" save-project!)
  (.addEventListener (.getElementById js/document "load-project") "click" load-project!)
  (.addEventListener (.getElementById js/document "import") "click" #(.click (.getElementById js/document "import-file")))
  (.addEventListener (.getElementById js/document "import-file") "change" import-project!)
  (.addEventListener (.getElementById js/document "export") "click" download-project!)))
