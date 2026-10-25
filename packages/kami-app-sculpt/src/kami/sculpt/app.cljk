(ns kami.sculpt.app (:require [cljs.reader :as reader] [kami.sculpt :as sculpt] [kami.sculpt.project :as project] [kami.webgpu.mesh :as gpu]))
(def base (sculpt/sphere-mesh 1.5 32 20))
(def initial-document (sculpt/sculpt-document base))
(defonce state (atom {:document initial-document :mode :inflate :radius 0.6 :strength 0.12 :spacing 0.12 :remesh-cell-size 0.15
                      :symmetry [:x] :profile :zbrush :shortcut-buffer "" :temporary-mode nil
                      :history [] :future [] :strokes 0 :project-id "untitled-sculpt" :project-name "Untitled Sculpt"
                      :revision 0 :save-status :clean}))
(defonce viewport (atom nil))
(declare checkpoint! upload!)
(defn- mesh [] (sculpt/evaluate-document (:document @state)))
(defn- refresh-layers! []
  (let [container (.getElementById js/document "layers") doc (:document @state)]
    (set! (.-innerHTML container) "")
    (doseq [layer (:sculpt/layers doc)]
      (let [b (.createElement js/document "button") id (:sculpt.layer/id layer)]
        (set! (.-textContent b) (str (if (:sculpt.layer/visible? layer) "◉ " "○ ") (:sculpt.layer/name layer)
                                     " · " (.toFixed (:sculpt.layer/opacity layer) 2)))
        (when (= id (:sculpt/active-layer doc)) (.add (.-classList b) "selected"))
        (.addEventListener b "click" #(do (swap! state assoc-in [:document :sculpt/active-layer] id)
                                           (set! (.-value (.getElementById js/document "layer-opacity")) (:sculpt.layer/opacity layer))
                                           (set! (.-value (.getElementById js/document "layer-name")) (:sculpt.layer/name layer))
                                           (refresh-layers!)))
        (.addEventListener b "dblclick" #(do (checkpoint!) (swap! state update :document sculpt/update-layer id update :sculpt.layer/visible? not) (upload!)))
        (.appendChild container b)))))
(defn- upload! []
  (when-let [v @viewport]
    (let [mesh (mesh) masked (count (filter pos? (:masks mesh))) topology (sculpt/topology-diagnostics mesh)]
      (swap! viewport assoc :buffers (gpu/upload-mesh! (:mesh-context v) mesh))
      (set! (.-textContent (.getElementById js/document "stats")) (str (count (:positions mesh)) " vertices · " (/ (count (:indices mesh)) 3) " triangles · " (:strokes @state) " strokes"))
      (set! (.-textContent (.getElementById js/document "debug-state"))
            (js/JSON.stringify (clj->js {:vertices (count (:positions mesh)) :triangles (/ (count (:indices mesh)) 3)
                                         :maskedVertices masked :mode (name (:mode @state)) :symmetry (:symmetry @state)
                                         :maskMin (reduce min 1 (:masks mesh)) :maskMax (reduce max 0 (:masks mesh))
                                         :profile (name (:profile @state)) :shortcutBuffer (:shortcut-buffer @state)
                                         :projectVersion project/current-version :revision (:revision @state) :saveStatus (name (:save-status @state))
                                         :layerCount (count (get-in @state [:document :sculpt/layers]))
                                         :layerNames (mapv :sculpt.layer/name (get-in @state [:document :sculpt/layers]))
                                         :activeLayer (get-in @state [:document :sculpt/active-layer])
                                         :remesh (get-in @state [:document :sculpt/base :remesh])
                                         :repair (get-in @state [:document :sculpt/base :repair])
                                         :holeFill (get-in @state [:document :sculpt/base :hole-fill])
                                         :remeshCellSize (:remesh-cell-size @state)
                                         :topology {:manifold (:topology/manifold? topology) :closed (:topology/closed? topology)
                                                    :boundaryEdges (count (:topology/boundary-edges topology))
                                                    :nonManifoldEdges (count (:topology/non-manifold-edges topology))
                                                    :degenerateFaces (count (:topology/degenerate-faces topology))
                                                    :isolatedVertices (count (:topology/isolated-vertices topology))}})))
      (let [info (get-in @state [:document :sculpt/base :remesh])]
        (set! (.-textContent (.getElementById js/document "remesh-result"))
              (if info (str (:source-vertices info) " → " (:result-vertices info) " vertices · " (:result-triangles info) " triangles")
                "Not remeshed")))
      (set! (.-textContent (.getElementById js/document "topology-result"))
            (str (if (:topology/manifold? topology) "Manifold" "Topology issues")
                 " · boundary " (count (:topology/boundary-edges topology))
                 " · non-manifold " (count (:topology/non-manifold-edges topology))
                 " · degenerate " (count (:topology/degenerate-faces topology))))
      (let [repair (get-in @state [:document :sculpt/base :repair])]
        (set! (.-textContent (.getElementById js/document "repair-result"))
              (if repair (str "Removed " (:removed-triangles repair) " triangles") "Not repaired")))
      (let [fill (get-in @state [:document :sculpt/base :hole-fill])]
        (set! (.-textContent (.getElementById js/document "hole-fill-result"))
              (if fill (str "Filled " (:holes fill) " holes · " (:triangles fill) " triangles") "No fill operation")))
    (refresh-layers!))))
(defn- draw! [] (when-let [{:keys [buffers] :as v} @viewport] (when buffers (gpu/render-frame! v buffers [0 1 5] [0 0 0] [0.85 0.42 0.58]))) (js/requestAnimationFrame draw!))
(defn- pointer-center [canvas e]
  (let [r (.getBoundingClientRect canvas) nx (- (* 2 (/ (- (.-clientX e) (.-left r)) (.-width r))) 1)
        ny (- 1 (* 2 (/ (- (.-clientY e) (.-top r)) (.-height r)))) z (js/Math.sqrt (max 0 (- 1 (* nx nx) (* ny ny))))]
    [(* 1.5 nx) (* 1.5 ny) (* 1.5 z)]))
(defn- brush-at! [center]
  (let [b (sculpt/brush center (:radius @state) (:strength @state) (:mode @state))]
    (swap! state update :document sculpt/apply-layer-stroke b (:symmetry @state)) (upload!)))
(defn- checkpoint! [] (swap! state (fn [s] (-> s (update :history conj (:document s))
                                               (assoc :future [] :save-status :dirty) (update :revision inc)))))
(defn- set-mode! [mode]
  (swap! state assoc :mode mode :shortcut-buffer "")
  (doseq [button (array-seq (.querySelectorAll js/document "[data-mode]"))]
    (.toggle (.-classList button) "selected" (= (name mode) (.getAttribute button "data-mode"))))
  (upload!))
(defn- editable-target? [event]
  (let [target (.-target event) tag (some-> target .-tagName .toLowerCase)]
    (or (#{"input" "select" "textarea"} tag) (.-isContentEditable target))))
(def zbrush-sequences {"bi" :inflate "bs" :smooth "bp" :pinch})
(def direct-modes {:blender {"i" :inflate "s" :smooth "p" :pinch "m" :mask "e" :mask-erase}
                   :mudbox {"1" :inflate "2" :smooth "3" :pinch "m" :mask "e" :mask-erase}})
(def ^:private storage-key "kami.sculpt.project.v2")
(def ^:private backup-key "kami.sculpt.project.backup")
(defn- project-document []
  (let [{:keys [project-id project-name document mode radius strength spacing remesh-cell-size symmetry profile strokes]} @state]
    (project/document {:id project-id :name project-name :sculpt-document document
                       :brush {:mode mode :radius radius :strength strength :spacing spacing :remesh-cell-size remesh-cell-size}
                       :symmetry symmetry :interaction {:profile profile} :strokes strokes})))
(defn- save-project! []
  (let [data (pr-str (project-document)) old (.getItem js/localStorage storage-key)]
    (when old (.setItem js/localStorage backup-key old)) (.setItem js/localStorage storage-key data)
    (swap! state assoc :save-status :saved) (upload!)))
(defn- sync-controls! []
  (let [{:keys [radius strength spacing remesh-cell-size symmetry profile]} @state]
    (doseq [[id value] [["radius" radius] ["strength" strength] ["spacing" spacing] ["remesh-cell-size" remesh-cell-size] ["profile" (name profile)]]]
      (set! (.-value (.getElementById js/document id)) value))
    (set! (.-textContent (.getElementById js/document "radius-value")) (.toFixed radius 2))
    (set! (.-textContent (.getElementById js/document "strength-value")) (.toFixed strength 2))
    (doseq [axis [:x :y :z]] (set! (.-checked (.getElementById js/document (str "symmetry-" (name axis)))) (some #{axis} symmetry)))))
(defn- apply-project! [value]
  (let [p (project/open value) brush (:project/brush p) interaction (:project/interaction p)]
    (swap! state assoc :project-id (:project/id p) :project-name (:project/name p) :document (:project/sculpt p)
           :mode (:mode brush) :radius (:radius brush) :strength (:strength brush) :spacing (:spacing brush)
           :remesh-cell-size (:remesh-cell-size brush 0.15)
           :symmetry (:project/symmetry p) :profile (:profile interaction) :strokes (:project/strokes p)
           :history [] :future [] :shortcut-buffer "" :temporary-mode nil :save-status :saved)
    (sync-controls!) (set-mode! (:mode brush)) (upload!)))
(defn- load-project! []
  (when-let [data (.getItem js/localStorage storage-key)]
    (try (apply-project! (reader/read-string data))
         (catch :default _ (when-let [backup (.getItem js/localStorage backup-key)] (apply-project! (reader/read-string backup)))))))
(defn- download-project! []
  (let [a (.createElement js/document "a") url (.createObjectURL js/URL (js/Blob. #js [(pr-str (project-document))] #js {:type "application/edn"}))]
    (set! (.-href a) url) (set! (.-download a) "sculpt.kami-sculpt.edn") (.click a) (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- import-project! [event]
  (when-let [file (aget (.. event -target -files) 0)] (-> (.text file) (.then #(apply-project! (reader/read-string %))))))
(defn- change-radius! [delta]
  (let [radius (max 0.1 (min 1.5 (+ (:radius @state) delta)))]
    (swap! state assoc :radius radius) (set! (.-value (.getElementById js/document "radius")) radius)
    (set! (.-textContent (.getElementById js/document "radius-value")) (.toFixed radius 2)) (upload!)))
(defn- on-key-down! [event]
  (when-not (editable-target? event)
    (let [key (.toLowerCase (.-key event)) ctrl (or (.-ctrlKey event) (.-metaKey event)) profile (:profile @state)]
      (cond
        (and ctrl (.-shiftKey event) (= key "n")) (do (.preventDefault event) (.click (.getElementById js/document "add-layer")))
        (and ctrl (= key "z")) (do (.preventDefault event) (.click (.getElementById js/document "undo")))
        (= key "[") (do (.preventDefault event) (change-radius! -0.05))
        (= key "]") (do (.preventDefault event) (change-radius! 0.05))
        (= key "shift") (when-not (:temporary-mode @state)
                           (swap! state assoc :temporary-mode (:mode @state)) (set-mode! :smooth))
        (= profile :zbrush)
        (cond
          (= key "m") (set-mode! :mask) (= key "e") (set-mode! :mask-erase)
          (re-matches #"[a-z]" key)
          (let [buffer (str (:shortcut-buffer @state) key) mode (get zbrush-sequences buffer)
                prefix? (some #(.startsWith % buffer) (keys zbrush-sequences))]
            (.preventDefault event)
            (cond mode (set-mode! mode) prefix? (swap! state assoc :shortcut-buffer buffer)
                  :else (swap! state assoc :shortcut-buffer key))))
        :else (when-let [mode (get-in direct-modes [profile key])] (.preventDefault event) (set-mode! mode))))))
(defn- on-key-up! [event]
  (when (and (= "Shift" (.-key event)) (:temporary-mode @state))
    (let [mode (:temporary-mode @state)] (swap! state assoc :temporary-mode nil) (set-mode! mode))))
(defn ^:export init! [] (let [canvas (.getElementById js/document "gpu-canvas") dragging (atom false) last-center (atom nil)]
 (-> (gpu/init-canvas! canvas) (.then (fn [v] (reset! viewport v) (upload!) (set! (.-textContent (.getElementById js/document "gpu-status")) "") (draw!))))
 (doseq [b (array-seq (.querySelectorAll js/document "[data-mode]"))] (.addEventListener b "click" #(set-mode! (keyword (.getAttribute b "data-mode")))))
 (.addEventListener (.getElementById js/document "profile") "change"
                    #(do (swap! state assoc :profile (keyword (.. % -target -value)) :shortcut-buffer "")
                         (set! (.-textContent (.getElementById js/document "profile-hint"))
                               (case (:profile @state) :blender "I Inflate · S Smooth · P Pinch · M Mask"
                                     :mudbox "1 Sculpt · 2 Smooth · 3 Pinch · M Mask"
                                     "B,I Inflate · B,S Smooth · B,P Pinch · M Mask")) (upload!)))
 (.addEventListener js/window "keydown" on-key-down!)
 (.addEventListener js/window "keyup" on-key-up!)
 (.addEventListener canvas "pointerdown" #(let [center (pointer-center canvas %)] (reset! dragging true) (reset! last-center center) (checkpoint!) (swap! state update :strokes inc) (brush-at! center)))
 (.addEventListener js/window "pointerup" #(do (reset! dragging false) (reset! last-center nil)))
 (.addEventListener canvas "pointermove" #(when @dragging
                                             (let [center (pointer-center canvas %) delta (mapv - center @last-center)
                                                   distance (js/Math.sqrt (reduce + (map (fn [x] (* x x)) delta)))]
                                               (when (>= distance (:spacing @state)) (reset! last-center center) (brush-at! center)))))
 (.addEventListener (.getElementById js/document "radius") "input" #(let [v (js/parseFloat (.. % -target -value))] (swap! state assoc :radius v) (set! (.-textContent (.getElementById js/document "radius-value")) (.toFixed v 2))))
 (.addEventListener (.getElementById js/document "strength") "input" #(let [v (js/parseFloat (.. % -target -value))] (swap! state assoc :strength v) (set! (.-textContent (.getElementById js/document "strength-value")) (.toFixed v 2))))
 (doseq [axis [:x :y :z]]
   (.addEventListener (.getElementById js/document (str "symmetry-" (name axis))) "change"
                      #(swap! state assoc :symmetry (vec (filter (fn [a] (.-checked (.getElementById js/document (str "symmetry-" (name a))))) [:x :y :z])))))
 (.addEventListener (.getElementById js/document "spacing") "input" #(swap! state assoc :spacing (js/parseFloat (.. % -target -value))))
 (.addEventListener (.getElementById js/document "clear-mask") "click" #(do (checkpoint!) (swap! state update-in [:document :sculpt/base] sculpt/clear-mask) (upload!)))
 (.addEventListener (.getElementById js/document "invert-mask") "click" #(do (checkpoint!) (swap! state update-in [:document :sculpt/base] sculpt/invert-mask) (upload!)))
 (doseq [[button-id operation] [["blur-mask" :blur] ["sharpen-mask" :sharpen] ["grow-mask" :grow] ["shrink-mask" :shrink]]]
   (.addEventListener (.getElementById js/document button-id) "click"
                      #(do (checkpoint!) (swap! state update-in [:document :sculpt/base] sculpt/filter-mask operation) (upload!))))
 (.addEventListener (.getElementById js/document "subdivide") "click" #(do (checkpoint!) (swap! state update :document sculpt/subdivide-document) (upload!)))
 (.addEventListener (.getElementById js/document "remesh-cell-size") "change"
                    #(swap! state assoc :remesh-cell-size (max 0.01 (js/parseFloat (.. % -target -value))) :save-status :dirty))
 (.addEventListener (.getElementById js/document "voxel-remesh") "click"
                    #(do (checkpoint!) (swap! state update :document sculpt/remesh-document (:remesh-cell-size @state)) (upload!)))
 (.addEventListener (.getElementById js/document "repair-topology") "click"
                    #(do (checkpoint!) (swap! state update :document sculpt/repair-document) (upload!)))
 (.addEventListener (.getElementById js/document "fill-holes") "click"
                    #(try (do (checkpoint!) (swap! state update :document sculpt/fill-holes-document) (upload!))
                          (catch :default error (set! (.-textContent (.getElementById js/document "hole-fill-result")) (.-message error)))))
 (.addEventListener (.getElementById js/document "add-layer") "click" #(do (checkpoint!) (swap! state update :document sculpt/add-layer (str "Detail " (inc (count (get-in @state [:document :sculpt/layers]))))) (upload!)))
 (.addEventListener (.getElementById js/document "delete-layer") "click" #(when (> (count (get-in @state [:document :sculpt/layers])) 1) (checkpoint!) (swap! state update :document sculpt/delete-layer (get-in @state [:document :sculpt/active-layer])) (upload!)))
 (.addEventListener (.getElementById js/document "rename-layer") "click"
                    #(let [id (get-in @state [:document :sculpt/active-layer]) name (.trim (.-value (.getElementById js/document "layer-name")))]
                       (when (seq name) (checkpoint!) (swap! state update :document sculpt/update-layer id assoc :sculpt.layer/name name) (upload!))))
 (.addEventListener (.getElementById js/document "duplicate-layer") "click"
                    #(do (checkpoint!) (swap! state update :document sculpt/duplicate-layer (get-in @state [:document :sculpt/active-layer])) (upload!)))
 (doseq [[button-id delta] [["layer-up" -1] ["layer-down" 1]]]
   (.addEventListener (.getElementById js/document button-id) "click"
                      #(let [id (get-in @state [:document :sculpt/active-layer]) layers (get-in @state [:document :sculpt/layers])
                             index (first (keep-indexed (fn [i layer] (when (= id (:sculpt.layer/id layer)) i)) layers)) target (+ index delta)]
                         (when (< -1 target (count layers)) (checkpoint!) (swap! state update :document sculpt/move-layer id target) (upload!)))))
 (.addEventListener (.getElementById js/document "bake-layer") "click"
                    #(do (checkpoint!) (swap! state update :document sculpt/bake-layer (get-in @state [:document :sculpt/active-layer])) (upload!)))
 (.addEventListener (.getElementById js/document "layer-opacity") "change" #(let [value (js/parseFloat (.. % -target -value)) id (get-in @state [:document :sculpt/active-layer])] (checkpoint!) (swap! state update :document sculpt/update-layer id assoc :sculpt.layer/opacity value) (upload!)))
 (.addEventListener (.getElementById js/document "undo") "click" #(when-let [doc (peek (:history @state))] (swap! state (fn [s] (assoc s :document doc :history (pop (:history s)) :future (conj (:future s) (:document s))))) (upload!)))
 (.addEventListener (.getElementById js/document "redo") "click" (fn [] (when-let [doc (peek (:future @state))] (swap! state (fn [s] (assoc s :document doc :future (pop (:future s)) :history (conj (:history s) (:document s))))) (upload!))))
 (.addEventListener (.getElementById js/document "save-project") "click" save-project!)
 (.addEventListener (.getElementById js/document "load-project") "click" load-project!)
 (.addEventListener (.getElementById js/document "import") "click" #(.click (.getElementById js/document "import-file")))
 (.addEventListener (.getElementById js/document "import-file") "change" import-project!)
 (.addEventListener (.getElementById js/document "export") "click" download-project!)))
