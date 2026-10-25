(ns kami.modeler.app
  (:require [cljs.reader :as reader]
            [goog.object :as gobj]
            [kami.modeling :as modeling]
            [kami.modeler.project :as project]
            [kami.modeler.vehicle :as modeler-vehicle]
            [kami.webgpu :as instanced-gpu]
            [kami.webgpu.ir :as render-ir]
            [kami.webgpu.mesh :as gpu-mesh]))

(def cube (modeling/cube 2))
(def initial-scene (modeling/scene [(modeling/object 1 "Cube" cube)]))

(defonce state (atom {:scene initial-scene :selected-object 1 :next-id 2 :selected-face 1 :selected-faces #{1} :distance 0.5 :snap 0.25 :transform-axis :z :selection-tool :pick
                      :component-mode :face :selected-vertex nil :selected-vertices #{} :selected-edge nil :selected-edges #{}
                      :history [initial-scene] :future [] :azimuth 0.7 :elevation 0.45
                      :mode :edit :profile :blender :project-id "untitled-model"
                      :project-name "Untitled Model" :revision 0 :save-status :clean}))
(defonce runtime (atom nil))
(defonce box-drag (atom nil))
(defonce large-scene-context (atom nil))
(defonce autosave-timer (atom nil))

(defn- report-error! [context error]
  (let [banner (or (.getElementById js/document "error-banner")
                   (let [node (.createElement js/document "div")]
                     (set! (.-id node) "error-banner")
                     (.setAttribute node "role" "alert")
                     (set! (.-cssText (.-style node)) "position:fixed;z-index:1000;left:16px;right:16px;top:64px;padding:12px 16px;border:1px solid #ff6b6b;border-radius:8px;background:#401b2bcc;color:#fff")
                     (.appendChild (.-body js/document) node)
                     node))]
    (set! (.-textContent banner) (str context ": " (or (.-message error) error)))
    (js/setTimeout #(when (.-isConnected banner) (.remove banner)) 8000)))

(defn- percentile-95 [values]
  (let [ordered (vec (sort values))]
    (nth ordered (dec (js/Math.ceil (* 0.95 (count ordered)))))))

(defn- measure-animation-frames! [draw-frame!]
  (js/Promise.
   (fn [resolve _]
     (let [samples (atom []) submits (atom []) previous (atom nil) remaining (atom 120)
           tick (fn tick [timestamp]
                  (when-let [last @previous] (swap! samples conj (- timestamp last)))
                  (reset! previous timestamp)
                  (let [start (.now js/performance)]
                    (draw-frame!)
                    (swap! submits conj (- (.now js/performance) start)))
                  (if (pos? (swap! remaining dec))
                    (js/requestAnimationFrame tick)
                    (resolve {:rafFrames (count @samples) :p95FrameMs (percentile-95 @samples)
                              :maxFrameMs (apply max @samples)
                              :sampleFrames (count @submits) :p95SubmitMs (percentile-95 @submits)
                              :maxSubmitMs (apply max @submits)})))]
       (js/requestAnimationFrame tick)))))

(defn- large-scene-stress! []
  (let [canvas (.getElementById js/document "large-scene-canvas")
        instances (mapv (fn [i] (assoc (render-ir/instance [(mod i 200) 0 (quot i 200)]
                                                            [0.25 0.58 0.95] [0.75 0.75 0.75]
                                                            :roughness 0.55) :geo :sphere)) (range 20000))
        frame (render-ir/render-ir (render-ir/sky [0.08 0.12 0.2] [-0.4 -0.85 -0.35] [1 0.96 0.85])
                                   instances [130 110 130] [50 0 50])
        measure! (fn [context]
                   (let [long-tasks (atom [])
                         observer (when (exists? js/PerformanceObserver)
                                    (let [o (js/PerformanceObserver.
                                             (fn [list]
                                               (doseq [entry (array-seq (.getEntries list))]
                                                 (swap! long-tasks conj (.-duration entry)))))]
                                      (try (.observe o #js {:type "longtask" :buffered true})
                                           (catch :default _ nil)) o))
                         first-start (.now js/performance)
                         _ (instanced-gpu/draw! context frame)
                         first-ms (- (.now js/performance) first-start)
                         second-start (.now js/performance)
                         _ (instanced-gpu/draw! context frame)
                         second-ms (- (.now js/performance) second-start)
                         triangles (/ (or (get-in context [:geos :sphere :idx-count])
                                          (get-in context [:geos :sphere :index-count])) 3)
                         capacity (if (= :webgpu (:backend context))
                                    (instanced-gpu/inst-buffer-capacity context) (count instances))
                         base {:backend (name (:backend context)) :instances (count instances)
                               :capacity capacity :instanceBufferBytes (* capacity 96)
                               :geometryKinds 1 :drawCalls 1 :trianglesPerInstance triangles
                               :residentTriangles (* triangles (count instances))
                               :pickingIds [1 10000 20000] :provenanceRevision "modeler-large-scene-v1"
                               :firstMs first-ms :cachedSecondMs second-ms}]
                     (-> (measure-animation-frames! #(instanced-gpu/draw! context frame))
                         (.then (fn [raf]
                                  (when observer (.disconnect observer))
                                  (clj->js (merge base raf
                                                  {:longTaskCount (count @long-tasks)
                                                   :maxLongTaskMs (apply max 0 @long-tasks)
                                                   :usedJSHeapBytes (some-> js/performance .-memory .-usedJSHeapSize)})))))))]
    (if-let [context @large-scene-context] (measure! context)
      (-> (instanced-gpu/init! canvas)
          (.then (fn [context] (reset! large-scene-context context) (measure! context)))))))

(defn- sub3 [a b] (mapv - a b))
(defn- cross [[ax ay az] [bx by bz]] [(- (* ay bz) (* az by)) (- (* az bx) (* ax bz)) (- (* ax by) (* ay bx))])
(defn- norm [v]
  (let [l (max 1.0e-8 (js/Math.hypot (nth v 0) (nth v 1) (nth v 2)))]
    (mapv #(/ % l) v)))
(defn- render-geometry [{:mesh/keys [vertices faces uvs]}]
  (let [triangles (mapcat (fn [f] (map (fn [i] [(first f) (nth f i) (nth f (inc i))]) (range 1 (dec (count f))))) faces)
        positions (vec (mapcat (fn [tri] (map #(nth vertices %) tri)) triangles))
        normals (vec (mapcat (fn [[a b c]] (repeat 3 (norm (cross (sub3 (nth vertices b) (nth vertices a)) (sub3 (nth vertices c) (nth vertices a)))))) triangles))]
    {:positions positions :normals normals
     :uvs (when uvs (vec (mapcat (fn [tri] (map #(nth uvs %) tri)) triangles)))
     :indices (vec (range (count positions)))}))
(defn- selected-object [] (modeling/find-object (:scene @state) (:selected-object @state)))
(defn- selected-mesh [] (:object/mesh (selected-object)))
(defn- channel->hex [value]
  (let [hex (.toString (js/Math.round (* 255 value)) 16)] (if (= 1 (count hex)) (str "0" hex) hex)))
(defn- color->hex [[r g b]] (str "#" (channel->hex r) (channel->hex g) (channel->hex b)))
(defn- hex->color [hex]
  (let [n (js/parseInt (subs hex 1) 16)]
    [(/ (bit-and (bit-shift-right n 16) 255) 255)
     (/ (bit-and (bit-shift-right n 8) 255) 255)
     (/ (bit-and n 255) 255) 1.0]))
(declare commit-scene! apply-project! base64->buffer refresh-mesh! update-ui! selected-vertex-set)

(defn- texture-payload [data-uri]
  (let [mime-type (subs data-uri 5 (.indexOf data-uri ";"))
        bytes (js/Uint8Array. (base64->buffer data-uri))]
    {:mime-type mime-type :bytes (vec (array-seq bytes))}))

(defn- refresh-mesh! []
  (let [combined (modeling/scene-mesh (:scene @state))]
  (set! (.-__kami_modeler_mesh js/window) (clj->js combined))
  (when-let [{:keys [mesh-context texture-cache]} @runtime]
    (swap! runtime assoc :draws
           (mapv (fn [{:object/keys [id material mesh]}]
                   (let [data-uri (:material/base-color-texture material)
                         texture (get texture-cache data-uri)]
                   {:object-id id
                    :color (subvec (vec (:material/base-color material)) 0 3)
                    :material {:metallic (:material/metallic material)
                               :roughness (:material/roughness material)
                               :texture (when-not (= :loading texture) texture)}
                    :buffers (gpu-mesh/upload-mesh! mesh-context (render-geometry mesh))}))
                 (modeling/scene-renderables (:scene @state))))
    (doseq [{:object/keys [material]} (modeling/scene-renderables (:scene @state))
            :let [data-uri (:material/base-color-texture material)]
            :when (and data-uri (not (contains? texture-cache data-uri)) (:device mesh-context))]
      (swap! runtime assoc-in [:texture-cache data-uri] :loading)
      (-> (gpu-mesh/upload-texture! mesh-context (texture-payload data-uri))
          (.then (fn [texture]
                   (swap! runtime assoc-in [:texture-cache data-uri] texture)
                   (set! (.-textContent (.getElementById js/document "texture-status")) "Texture loaded")
                   (refresh-mesh!) (update-ui!)))
          (.catch (fn [error]
                    (swap! runtime update :texture-cache dissoc data-uri)
                    (set! (.-textContent (.getElementById js/document "texture-status")) (str "Texture error: " error)))))))))

(defn- update-ui! []
  (let [{:keys [scene distance history future profile selected-face selected-faces selected-vertex selected-vertices selected-edge selected-edges component-mode mode save-status revision]} @state
        selected-id (:selected-object @state) mesh (selected-mesh) object (selected-object)]
    (set! (.-textContent (.getElementById js/document "distanceValue")) (.toFixed distance 2))
    (set! (.-textContent (.getElementById js/document "meshStats"))
          (str (count (:scene/objects scene)) " objects · " (count (:mesh/vertices (modeling/scene-mesh scene))) " vertices"))
    (set! (.-textContent (.getElementById js/document "selection"))
          (if (= mode :edit) (case component-mode
                               :vertex (if (seq selected-vertices)
                                         (if (= 1 (count selected-vertices)) (str "Vertex " selected-vertex " selected") (str (count selected-vertices) " vertices selected"))
                                         "No vertex selected")
                               :edge (if (seq selected-edges)
                                       (if (= 1 (count selected-edges)) (str "Edge " (pr-str selected-edge) " selected") (str (count selected-edges) " edges selected"))
                                       "No edge selected")
                               (if (seq selected-faces)
                                 (if (= 1 (count selected-faces)) (str "Face " selected-face " selected")
                                     (str (count selected-faces) " faces selected"))
                                 "No face selected"))
              (str "Object · " (:object/name object))))
    (set! (.-textContent (.getElementById js/document "mode-toggle")) (if (= mode :edit) "Edit Mode" "Object Mode"))
    (set! (.-textContent (.getElementById js/document "tool")) (if (= mode :edit) (str (name component-mode) " Select") "Object Select"))
    (doseq [kind [:vertex :edge :face]]
      (.toggle (.-classList (.getElementById js/document (str "component-" (name kind)))) "selected" (= kind component-mode)))
    (doseq [axis [:x :y :z]]
      (doseq [prefix ["axis-" "gizmo-"]]
        (.toggle (.-classList (.getElementById js/document (str prefix (name axis)))) "selected" (= axis (:transform-axis @state)))))
    (doseq [id ["extrude" "inset" "bevel" "loop-cut" "knife" "bridge" "flip-normals" "scale" "move" "delete-face"]]
      (set! (.-disabled (.getElementById js/document id)) (or (not= mode :edit) (not= component-mode :face) (:object/locked? object))))
    (set! (.-disabled (.getElementById js/document "move")) (or (not= mode :edit) (:object/locked? object)))
    (set! (.-disabled (.getElementById js/document "bridge"))
          (or (not= mode :edit) (not= component-mode :face) (not= 2 (count selected-faces)) (:object/locked? object)))
    (let [tree (.getElementById js/document "scene-tree")]
      (set! (.-innerHTML tree) "")
      (doseq [o (:scene/objects scene)]
        (let [row (.createElement js/document "div") b (.createElement js/document "button")
              visible (.createElement js/document "button") locked (.createElement js/document "button")]
          (set! (.-className row) "outliner-row")
          (set! (.-textContent b) (str "◆ " (:object/name o)))
          (when (= selected-id (:object/id o)) (.add (.-classList b) "selected"))
          (.addEventListener b "click" #(when-not (:object/locked? o) (swap! state assoc :selected-object (:object/id o) :selected-face 0) (update-ui!)))
          (set! (.-textContent visible) (if (:object/visible? o) "◉" "○")) (set! (.-title visible) "Toggle visibility")
          (.addEventListener visible "click" #(commit-scene! (modeling/set-object-visible (:scene @state) (:object/id o) (not (:object/visible? o)))))
          (set! (.-textContent locked) (if (:object/locked? o) "🔒" "🔓")) (set! (.-title locked) "Toggle lock")
          (.addEventListener locked "click" #(commit-scene! (modeling/set-object-locked (:scene @state) (:object/id o) (not (:object/locked? o)))))
          (doseq [node [b visible locked]] (.appendChild row node)) (.appendChild tree row))))
    (when object
      (set! (.-value (.getElementById js/document "object-name")) (:object/name object))
      (doseq [[id value] (map vector ["tx" "ty" "tz"] (:object/translation object))]
        (set! (.-value (.getElementById js/document id)) value))
      (let [material (:object/material object)]
        (set! (.-value (.getElementById js/document "base-color"))
              (color->hex (:material/base-color material)))
        (doseq [[id key] [["metallic" :material/metallic] ["roughness" :material/roughness]]]
          (set! (.-value (.getElementById js/document id)) (get material key))
          (set! (.-textContent (.getElementById js/document (str id "-value")))
                (.toFixed (get material key) 2))))
      (let [parent-select (.getElementById js/document "object-parent")]
        (set! (.-innerHTML parent-select) "")
        (doseq [[value label] (concat [["" "None"]]
                                     (map (fn [o] [(str (:object/id o)) (:object/name o)])
                                          (remove #(= (:object/id %) selected-id) (:scene/objects scene))))]
          (let [option (.createElement js/document "option")] (set! (.-value option) value) (set! (.-textContent option) label) (.appendChild parent-select option)))
        (set! (.-value parent-select) (str (or (:object/parent object) ""))))
      (let [target-select (.getElementById js/document "boolean-target")
            targets (remove #(= (:object/id %) selected-id) (:scene/objects scene))]
        (set! (.-innerHTML target-select) "")
        (doseq [target targets]
          (let [option (.createElement js/document "option")]
            (set! (.-value option) (:object/id target))
            (set! (.-textContent option) (:object/name target))
            (.appendChild target-select option)))
        (doseq [id ["boolean-union" "boolean-intersection" "boolean-difference"]]
          (set! (.-disabled (.getElementById js/document id)) (empty? targets))))
      (doseq [id ["object-name" "tx" "ty" "tz" "object-parent" "apply-transform" "add-mirror" "add-subdivision" "add-array" "modifier-kind" "add-catalog-modifier" "delete-object"]]
        (set! (.-disabled (.getElementById js/document id)) (:object/locked? object)))
      (let [stack (.getElementById js/document "modifier-stack")]
        (set! (.-innerHTML stack) "")
        (doseq [[index mod] (map-indexed vector (:object/modifiers object))]
          (let [row (.createElement js/document "div") label (.createElement js/document "span")
                up (.createElement js/document "button") down (.createElement js/document "button") remove (.createElement js/document "button")]
            (set! (.-className row) "modifier-row")
            (set! (.-textContent label) (str (name (:modifier/kind mod)) " " (pr-str (:modifier/options mod))))
            (set! (.-textContent up) "↑") (set! (.-disabled up) (zero? index))
            (.addEventListener up "click" #(commit-scene! (modeling/update-object (:scene @state) (:selected-object @state) modeling/move-modifier (:modifier/id mod) (dec index))))
            (set! (.-textContent down) "↓") (set! (.-disabled down) (= index (dec (count (:object/modifiers object)))))
            (.addEventListener down "click" #(commit-scene! (modeling/update-object (:scene @state) (:selected-object @state) modeling/move-modifier (:modifier/id mod) (inc index))))
            (set! (.-textContent remove) "×")
            (.addEventListener remove "click" #(commit-scene! (modeling/update-object (:scene @state) (:selected-object @state) modeling/remove-modifier (:modifier/id mod))))
            (doseq [node [label up down remove]] (.appendChild row node)) (.appendChild stack row)))))
    (set! (.-textContent (.getElementById js/document "debug-state"))
          (js/JSON.stringify (clj->js {:objectCount (count (:scene/objects scene)) :selectedObject selected-id
                                       :faceCount (count (:mesh/faces mesh))
                                       :modifierCount (count (:object/modifiers object))
                                       :evaluatedVertices (count (:mesh/vertices (modeling/evaluated-object-mesh object)))
                                       :mode (name mode) :profile (name profile)
                                       :componentMode (name component-mode) :selectedVertex selected-vertex :selectedEdge selected-edge
                                       :visible (:object/visible? object) :locked (:object/locked? object) :parent (:object/parent object)
                                       :material (:object/material object)
                                       :textureLoaded (let [texture (get-in @runtime [:texture-cache (get-in object [:object/material :material/base-color-texture])])]
                                                        (boolean (and texture (not= :loading texture))))
                                       :textureCacheCount (count (:texture-cache @runtime))
                                       :textureDevice (boolean (get-in @runtime [:mesh-context :device]))
                                       :uvCount (count (:mesh/uvs mesh))
                                       :firstUV (first (:mesh/uvs mesh))
                                       :signedVolume (modeling/signed-volume mesh)
                                       :projectVersion project/current-version :revision revision :saveStatus (name save-status)})))
    (set! (.-textContent (.getElementById js/document "project-status"))
          (str (name save-status) " · r" revision))
    (set! (.-disabled (.getElementById js/document "undo")) (= 1 (count history)))
    (set! (.-disabled (.getElementById js/document "redo")) (empty? future))
    (set! (.-textContent (.getElementById js/document "shortcutHint"))
          (case profile :maya "Ctrl+E Extrude · W Move · R Scale · Ctrl+D Duplicate"
                :max "Alt+E Extrude · W Move · R Scale · Ctrl+V Duplicate"
                :c4d "D Extrude · E Move · T Scale · Ctrl+drag Orbit"
                "E Extrude · I Inset · G Move · S Scale · X Delete · Tab Mode"))))

(defn- commit-scene! [scene]
  (swap! state (fn [s] (-> s (assoc :scene scene :future [] :save-status :dirty)
                            (update :history conj scene) (update :revision inc))))
  (refresh-mesh!) (update-ui!))
(defn- commit-mesh! [m]
  (commit-scene! (modeling/update-object (:scene @state) (:selected-object @state) assoc :object/mesh m)))
(defn- edit-mode? [] (and (= :edit (:mode @state)) (not (:object/locked? (selected-object)))))
(defn- face-edit? [] (and (edit-mode?) (= :face (:component-mode @state))))
(defn- extrude! []
  (when (face-edit?)
  (let [{:keys [selected-face distance]} @state mesh (selected-mesh)
        next-selected-face (dec (count (:mesh/faces mesh)))
        delta (mapv #(* distance %) (modeling/face-normal mesh selected-face))]
    (swap! state assoc :selected-face next-selected-face :selected-faces #{next-selected-face})
    (commit-mesh! (modeling/extrude-face mesh selected-face delta)))))
(defn- inset! [] (let [{:keys [selected-face distance]} @state mesh (selected-mesh)] (when (and (face-edit?) (some? selected-face)) (commit-mesh! (modeling/inset-face mesh selected-face (max 0.05 (min 0.95 distance)))))))
(defn- bevel! []
  (let [{:keys [selected-face distance]} @state mesh (selected-mesh)]
    (when (and (face-edit?) (some? selected-face))
      (commit-mesh! (modeling/bevel-face mesh selected-face
                                         (max 0.01 (min 0.49 (/ distance 4.0))) distance)))))
(defn- loop-cut! []
  (let [{:keys [selected-face distance]} @state mesh (selected-mesh)]
    (when (and (face-edit?) (some? selected-face))
      (commit-mesh! (modeling/loop-cut-face mesh selected-face
                                            (max 0.05 (min 0.95 (/ distance 2.0))))))))
(defn- bridge! []
  (let [{:keys [selected-faces]} @state mesh (selected-mesh)]
    (when (and (face-edit?) (= 2 (count selected-faces)))
      (let [[a b] (sort selected-faces)]
        (commit-mesh! (modeling/bridge-faces mesh a b))
        (swap! state assoc :selected-face 0 :selected-faces #{0})
        (update-ui!)))))
(defn- knife! []
  (let [{:keys [selected-face]} @state mesh (selected-mesh)
        edge-count (count (get-in mesh [:mesh/faces selected-face]))]
    (when (and (face-edit?) (some? selected-face) (>= edge-count 4))
      (commit-mesh! (modeling/knife-face mesh selected-face 0 (quot edge-count 2) 0.5 0.5)))))
(defn- scale! [] (let [{:keys [selected-faces distance]} @state mesh (selected-mesh)] (when (and (face-edit?) (seq selected-faces)) (commit-mesh! (modeling/scale-faces mesh selected-faces distance)))))
(defn- move! []
  (let [{:keys [component-mode selected-faces selected-vertices selected-edges distance transform-axis]} @state mesh (selected-mesh)
        delta (case transform-axis :x [distance 0 0] :y [0 distance 0] [0 0 distance])]
    (when (edit-mode?)
      (case component-mode :vertex (when (seq selected-vertices) (commit-mesh! (modeling/translate-vertices mesh selected-vertices delta)))
            :edge (when (seq selected-edges) (commit-mesh! (modeling/translate-edges mesh selected-edges delta)))
            :face (when (seq selected-faces) (commit-mesh! (modeling/translate-faces mesh selected-faces delta))) nil))))
(defn- flip-normals! []
  (when (and (face-edit?) (seq (:selected-faces @state)))
    (commit-mesh! (modeling/flip-faces (selected-mesh) (:selected-faces @state)))))
(defn- orient-outward! []
  (when (edit-mode?) (commit-mesh! (modeling/orient-outward (selected-mesh)))))
(defn- import-texture! [event]
  (when-let [file (aget (.. event -target -files) 0)]
    (let [reader (js/FileReader.)]
      (set! (.-onload reader)
            #(let [material (assoc (:object/material (selected-object)) :material/base-color-texture (.. % -target -result))]
               (commit-scene! (modeling/set-object-material (:scene @state) (:selected-object @state) material))
               (set! (.-textContent (.getElementById js/document "texture-status")) (.-name file))))
      (.readAsDataURL reader file))))
(defn- remove-texture! []
  (let [material (dissoc (:object/material (selected-object)) :material/base-color-texture)]
    (commit-scene! (modeling/set-object-material (:scene @state) (:selected-object @state) material))
    (set! (.-textContent (.getElementById js/document "texture-status")) "No texture")))
(defn- input-number [id] (js/parseFloat (.-value (.getElementById js/document id))))
(defn- transform-uv-selection! []
  (let [indices (selected-vertex-set)
        options {:offset [(input-number "uv-offset-u") (input-number "uv-offset-v")]
                 :scale [(input-number "uv-scale-u") (input-number "uv-scale-v")]
                 :rotation (* (/ js/Math.PI 180) (input-number "uv-rotation"))}]
    (when (seq indices) (commit-mesh! (modeling/transform-uvs (selected-mesh) indices options)))))
(defn- boolean-objects! [operation]
  (let [scene (:scene @state) source-id (:selected-object @state)
        target-id (js/parseInt (.-value (.getElementById js/document "boolean-target")))
        renderables (modeling/scene-renderables scene)
        source-mesh (:object/mesh (first (filter #(= source-id (:object/id %)) renderables)))
        target-mesh (:object/mesh (first (filter #(= target-id (:object/id %)) renderables)))]
    (when (and source-mesh target-mesh)
      (let [result (modeling/boolean-mesh source-mesh target-mesh operation)
            updated (-> scene
                        (modeling/update-object source-id
                                                (fn [object] (assoc object :object/mesh result :object/modifiers []
                                                                    :object/parent nil :object/translation [0 0 0]
                                                                    :object/rotation [0 0 0] :object/scale [1 1 1])))
                        (modeling/delete-object target-id))]
        (swap! state assoc :selected-face 0 :selected-faces #{0})
        (commit-scene! updated)))))
(defn- delete-face! [] (let [{:keys [selected-face]} @state mesh (selected-mesh)] (when (and (face-edit?) (some? selected-face) (> (count (:mesh/faces mesh)) 1)) (commit-mesh! (modeling/delete-face mesh selected-face)) (swap! state assoc :selected-face 0 :selected-faces #{0}) (update-ui!))))
(defn- undo! [] (when (> (count (:history @state)) 1) (swap! state (fn [s] (let [h (:history s) current (peek h) h' (pop h)] (assoc s :history h' :scene (peek h') :future (conj (:future s) current))))) (refresh-mesh!) (update-ui!)))
(defn- redo! [] (when-let [scene (peek (:future @state))] (swap! state (fn [s] (assoc s :scene scene :history (conj (:history s) scene) :future (pop (:future s))))) (refresh-mesh!) (update-ui!)))
(defn- duplicate-object! []
  (let [id (:next-id @state)]
    (commit-scene! (modeling/duplicate-object (:scene @state) (:selected-object @state) id))
    (swap! state assoc :selected-object id :next-id (inc id)) (update-ui!)))
(defn- delete-object! []
  (when (> (count (:scene/objects (:scene @state))) 1)
    (let [scene (modeling/delete-object (:scene @state) (:selected-object @state)) next-id (:object/id (first (:scene/objects scene)))]
      (swap! state assoc :selected-object next-id :selected-face 0) (commit-scene! scene))))
(defn- toggle-mode! [] (swap! state update :mode #(if (= % :edit) :object :edit)) (update-ui!))
(defn- set-component-mode! [kind]
  (swap! state assoc :component-mode kind)
  (update-ui!))
(defn- set-transform-axis! [axis]
  (swap! state assoc :transform-axis axis :save-status :dirty)
  (update-ui!))
(defn- select-all-components! []
  (when (edit-mode?)
    (case (:component-mode @state)
      :vertex (let [selected (set (range (count (:mesh/vertices (selected-mesh)))))]
                (swap! state assoc :selected-vertices selected :selected-vertex (first selected)))
      :edge (let [selected (set (modeling/mesh-edges (selected-mesh)))]
              (swap! state assoc :selected-edges selected :selected-edge (first selected)))
      :face (let [selected (set (range (count (:mesh/faces (selected-mesh)))))]
              (swap! state assoc :selected-faces selected :selected-face (first selected))) nil)
    (update-ui!)))
(defn- selected-vertex-set []
  (let [{:keys [component-mode selected-vertices selected-edges selected-faces]} @state]
    (case component-mode
      :vertex selected-vertices
      :edge (set (mapcat identity selected-edges))
      :face (if (seq selected-faces) (set (modeling/selected-vertex-indices (selected-mesh) selected-faces)) #{})
      #{})))
(defn- snap-selection! []
  (let [indices (selected-vertex-set) increment (:snap @state)]
    (when (seq indices) (commit-mesh! (modeling/snap-vertices (selected-mesh) indices increment)))))

(defn- editable-target? [event]
  (let [target (.-target event) tag (some-> target .-tagName .toLowerCase)]
    (or (#{"input" "select" "textarea"} tag) (.-isContentEditable target))))
(defn- command-for-event [event]
  (let [key (.toLowerCase (.-key event)) ctrl (or (.-ctrlKey event) (.-metaKey event)) alt (.-altKey event)
        profile (:profile @state)]
    (cond
      (and ctrl (= key "z") (.-shiftKey event)) :redo
      (and ctrl (= key "z")) :undo
      (and ctrl (= key "y")) :redo
      (and (= profile :blender) ctrl (= key "r")) :loop-cut
      (= key "tab") :toggle-mode
      (= key "a") :select-all-components
      (= key "1") :vertex-mode (= key "2") :edge-mode (= key "3") :face-mode
      (= profile :blender) ({"e" :extrude "i" :inset "b" :bevel "k" :knife "g" :move "s" :scale "x" :delete-face} key)
      (= profile :maya) (cond (and ctrl (= key "e")) :extrude (and ctrl (= key "d")) :duplicate-object
                              (= key "w") :move (= key "r") :scale (= key "delete") :delete-face)
      (= profile :max) (cond (and alt (= key "e")) :extrude (and ctrl (= key "v")) :duplicate-object
                             (= key "w") :move (= key "r") :scale (= key "delete") :delete-face)
      (= profile :c4d) ({"d" :extrude "i" :inset "e" :move "t" :scale "backspace" :delete-face} key))))
(defn- execute-command! [command]
  (case command :extrude (extrude!) :inset (inset!) :bevel (bevel!) :loop-cut (loop-cut!) :knife (knife!) :bridge (bridge!) :move (move!) :scale (scale!)
        :delete-face (delete-face!) :duplicate-object (duplicate-object!) :undo (undo!) :redo (redo!)
        :toggle-mode (toggle-mode!) :vertex-mode (set-component-mode! :vertex)
        :edge-mode (set-component-mode! :edge) :face-mode (set-component-mode! :face)
        :select-all-components (select-all-components!) nil))

(def ^:private storage-key "kami.modeler.project.v2")
(def ^:private backup-key "kami.modeler.project.backup")

(defn- project-document []
  (let [{:keys [project-id project-name scene selected-object selected-face selected-faces selected-vertex selected-vertices selected-edge selected-edges component-mode mode
                azimuth elevation profile snap transform-axis]} @state]
    (project/document {:id project-id :name project-name :scene scene
                       :selection {:object-id selected-object :face-id selected-face :face-ids selected-faces
                                   :vertex-id selected-vertex :vertex-ids selected-vertices
                                   :edge selected-edge :edges selected-edges :component-mode component-mode :mode mode}
                       :camera {:azimuth azimuth :elevation elevation}
                       :interaction {:profile profile :snap snap :transform-axis transform-axis}})))

(defn- save-project! []
  (try
    (let [serialized (pr-str (project-document))
          previous (.getItem js/localStorage storage-key)]
      (when previous (.setItem js/localStorage backup-key previous))
      (.setItem js/localStorage storage-key serialized)
      (swap! state assoc :save-status :saved)
      (update-ui!))
    (catch :default error (report-error! "Project could not be saved" error))))

(defn- schedule-autosave! []
  (when @autosave-timer (js/clearTimeout @autosave-timer))
  (reset! autosave-timer (js/setTimeout #(do (reset! autosave-timer nil) (save-project!)) 750)))

(defn- apply-project! [value]
  (let [p (project/open value) scene (:project/scene p)
        selection (:project/selection p) camera (:project/camera p)
        interaction (:project/interaction p)
        object-id (or (:object-id selection) (:object/id (first (:scene/objects scene))))]
    (swap! state assoc :project-id (:project/id p) :project-name (:project/name p)
           :scene scene :selected-object object-id :selected-face (:face-id selection)
           :selected-faces (set (or (:face-ids selection) [(:face-id selection)]))
           :selected-vertex (:vertex-id selection) :selected-vertices (set (or (:vertex-ids selection) (some-> (:vertex-id selection) vector)))
           :selected-edge (:edge selection) :selected-edges (set (or (:edges selection) (some-> (:edge selection) vector)))
           :component-mode (:component-mode selection :face) :mode (:mode selection)
           :azimuth (:azimuth camera) :elevation (:elevation camera) :snap (:snap interaction 0.25)
           :transform-axis (:transform-axis interaction :z)
           :profile (:profile interaction) :history [scene] :future [] :save-status :saved)
    (set! (.-value (.getElementById js/document "profile")) (name (:profile interaction)))
    (set! (.-value (.getElementById js/document "snap-increment")) (:snap interaction 0.25))
    (refresh-mesh!) (update-ui!)))

(defn- load-project! []
  (when-let [serialized (.getItem js/localStorage storage-key)]
    (try (apply-project! (reader/read-string serialized))
         (catch :default _
           (when-let [backup (.getItem js/localStorage backup-key)]
             (apply-project! (reader/read-string backup)))))))

(defn- download-project! []
  (let [a (.createElement js/document "a")
        url (.createObjectURL js/URL (js/Blob. #js [(pr-str (project-document))]
                                             #js {:type "application/edn"}))]
    (set! (.-href a) url) (set! (.-download a) "model.kami-modeler.edn") (.click a)
    (js/setTimeout #(.revokeObjectURL js/URL url) 0)))

(defn- bytes->base64 [bytes]
  (let [chunk 8192 parts (array)]
    (loop [offset 0]
      (when (< offset (.-length bytes))
        (let [end (min (.-length bytes) (+ offset chunk)) chars (array)]
          (doseq [i (range offset end)] (.push chars (.fromCharCode js/String (aget bytes i))))
          (.push parts (.join chars "")) (recur end))))
    (js/btoa (.join parts ""))))

(defn- base64->buffer [uri]
  (let [encoded (second (.split uri ",")) binary (js/atob encoded)
        bytes (js/Uint8Array. (.-length binary))]
    (dotimes [i (.-length binary)] (aset bytes i (.charCodeAt binary i)))
    (.-buffer bytes)))

(defn- gltf-accessor [doc buffer accessor-index]
  (let [accessor (aget (gobj/get doc "accessors") accessor-index)
        view-spec (aget (gobj/get doc "bufferViews") (gobj/get accessor "bufferView"))
        offset (+ (or (gobj/get view-spec "byteOffset") 0) (or (gobj/get accessor "byteOffset") 0))
        count (gobj/get accessor "count") components ({"SCALAR" 1 "VEC2" 2 "VEC3" 3 "VEC4" 4} (gobj/get accessor "type"))
        component-type (gobj/get accessor "componentType")
        component-size ({5121 1 5123 2 5125 4 5126 4} component-type)
        stride (or (gobj/get view-spec "byteStride") (* components component-size)) view (js/DataView. buffer)
        read-one (fn [byte-offset]
                   (case component-type 5121 (.getUint8 view byte-offset)
                         5123 (.getUint16 view byte-offset true)
                         5125 (.getUint32 view byte-offset true)
                         5126 (.getFloat32 view byte-offset true)
                         (throw (js/Error. (str "Unsupported glTF component type " component-type)))))]
    (mapv (fn [i]
            (let [values (mapv #(read-one (+ offset (* i stride) (* % component-size))) (range components))]
              (if (= 1 components) (first values) values))) (range count))))

(defn- quaternion->euler [[x y z w]]
  (let [sinr (* 2 (+ (* w x) (* y z))) cosr (- 1 (* 2 (+ (* x x) (* y y))))
        sinp (* 2 (- (* w y) (* z x)))
        rx (js/Math.atan2 sinr cosr)
        ry (if (>= (js/Math.abs sinp) 1) (* (if (neg? sinp) -1 1) (/ js/Math.PI 2)) (js/Math.asin sinp))
        siny (* 2 (+ (* w z) (* x y))) cosy (- 1 (* 2 (+ (* y y) (* z z))))]
    [rx ry (js/Math.atan2 siny cosy)]))

(defn- gltf-primitive-object [doc buffer mesh-spec primitive id primitive-index options]
  (let [attrs (gobj/get primitive "attributes") positions (gltf-accessor doc buffer (gobj/get attrs "POSITION"))
        indices (gltf-accessor doc buffer (gobj/get primitive "indices")) faces (mapv vec (partition 3 indices))
        uvs (when (some? (gobj/get attrs "TEXCOORD_0")) (gltf-accessor doc buffer (gobj/get attrs "TEXCOORD_0")))
        mesh (modeling/mesh positions faces uvs) materials (gobj/get doc "materials")
        material-spec (when (and materials (some? (gobj/get primitive "material"))) (aget materials (gobj/get primitive "material")))
        pbr (when material-spec (gobj/get material-spec "pbrMetallicRoughness"))
        color (vec (or (some-> pbr (gobj/get "baseColorFactor") array-seq) [0.35 0.58 1.0 1.0]))
        material {:material/base-color color :material/metallic (or (some-> pbr (gobj/get "metallicFactor")) 0.0)
                  :material/roughness (or (some-> pbr (gobj/get "roughnessFactor")) 0.5)}
        base-name (or (gobj/get mesh-spec "name") "Imported glTF")]
    (modeling/object id (if (> (.-length (gobj/get mesh-spec "primitives")) 1)
                          (str base-name " · " (inc primitive-index)) base-name) mesh (assoc options :material material))))

(defn- import-gltf! [event]
  (when-let [file (aget (.. event -target -files) 0)]
    (-> (.text file)
        (.then (fn [text]
                 (let [doc (js/JSON.parse text) buffer-spec (aget (gobj/get doc "buffers") 0)
                       buffer (base64->buffer (gobj/get buffer-spec "uri")) meshes (gobj/get doc "meshes")
                       nodes (vec (array-seq (or (gobj/get doc "nodes") #js [])))
                       parent-node (reduce (fn [result [parent-index node]]
                                             (reduce #(assoc %1 %2 parent-index) result
                                                     (array-seq (or (gobj/get node "children") #js [])))) {} (map-indexed vector nodes))
                       node-specs (vec (mapcat (fn [[node-index node]]
                                                (when-let [mesh-index (gobj/get node "mesh")]
                                                  (let [mesh-spec (aget meshes mesh-index)]
                                                    (map-indexed (fn [primitive-index primitive]
                                                                   {:node-index node-index :node node :mesh-spec mesh-spec
                                                                    :primitive primitive :primitive-index primitive-index})
                                                                 (array-seq (gobj/get mesh-spec "primitives"))))))
                                              (map-indexed vector nodes)))
                       specs (if (seq node-specs) node-specs
                               (vec (mapcat (fn [[mesh-index mesh-spec]]
                                              (map-indexed (fn [primitive-index primitive]
                                                             {:node-index mesh-index :node #js {} :mesh-spec mesh-spec
                                                              :primitive primitive :primitive-index primitive-index})
                                                           (array-seq (gobj/get mesh-spec "primitives"))))
                                            (map-indexed vector (array-seq meshes)))))
                       node-first-id (reduce (fn [result [id spec]] (if (contains? result (:node-index spec)) result
                                                                      (assoc result (:node-index spec) id)))
                                             {} (map vector (range 1 (inc (count specs))) specs))
                       objects (mapv (fn [id {:keys [node-index node mesh-spec primitive primitive-index]}]
                                       (let [parent-index (get parent-node node-index)
                                             anchor-id (get node-first-id node-index)
                                             parent-id (if (pos? primitive-index) anchor-id (get node-first-id parent-index))
                                             options {:translation (vec (or (some-> (gobj/get node "translation") array-seq) [0 0 0]))
                                                      :rotation (quaternion->euler (vec (or (some-> (gobj/get node "rotation") array-seq) [0 0 0 1])))
                                                      :scale (vec (or (some-> (gobj/get node "scale") array-seq) [1 1 1]))
                                                      :parent parent-id}]
                                         (gltf-primitive-object doc buffer mesh-spec primitive id primitive-index options)))
                                     (range 1 (inc (count specs))) specs)]
                   (when (empty? objects) (throw (js/Error. "glTF contains no mesh primitives")))
                   (apply-project! (modeling/scene objects))
                   (swap! state assoc :next-id (inc (count objects))))))
        (.catch #(js/console.error "glTF import failed" %)))))

(defn- download-gltf! []
  (let [{:keys [positions normals indices]} (render-geometry (modeling/scene-mesh (:scene @state)))
        position-flat (vec (mapcat identity positions)) normal-flat (vec (mapcat identity normals))
        position-bytes (* 4 (count position-flat)) normal-bytes (* 4 (count normal-flat)) index-bytes (* 4 (count indices))
        buffer (js/ArrayBuffer. (+ position-bytes normal-bytes index-bytes)) view (js/DataView. buffer)]
    (doseq [[i value] (map-indexed vector position-flat)] (.setFloat32 view (* i 4) value true))
    (doseq [[i value] (map-indexed vector normal-flat)] (.setFloat32 view (+ position-bytes (* i 4)) value true))
    (doseq [[i value] (map-indexed vector indices)] (.setUint32 view (+ position-bytes normal-bytes (* i 4)) value true))
    (let [axes (apply map vector positions) minima (mapv #(reduce min %) axes) maxima (mapv #(reduce max %) axes)
          doc {:asset {:version "2.0" :generator "Kami Modeler / kami-engine"}
               :scene 0 :scenes [{:nodes [0]}] :nodes [{:name (:project-name @state) :mesh 0}]
               :meshes [{:name (:project-name @state) :primitives [{:attributes {:POSITION 0 :NORMAL 1} :indices 2
                                                                      :material 0 :mode 4}]}]
               :materials [{:name "Kami Material" :pbrMetallicRoughness {:baseColorFactor [0.35 0.58 1.0 1.0]
                                                                           :metallicFactor 0.0 :roughnessFactor 0.5}}]
               :buffers [{:byteLength (.-byteLength buffer)
                          :uri (str "data:application/octet-stream;base64," (bytes->base64 (js/Uint8Array. buffer)))}]
               :bufferViews [{:buffer 0 :byteOffset 0 :byteLength position-bytes :target 34962}
                             {:buffer 0 :byteOffset position-bytes :byteLength normal-bytes :target 34962}
                             {:buffer 0 :byteOffset (+ position-bytes normal-bytes) :byteLength index-bytes :target 34963}]
               :accessors [{:bufferView 0 :componentType 5126 :count (count positions) :type "VEC3" :min minima :max maxima}
                           {:bufferView 1 :componentType 5126 :count (count normals) :type "VEC3"}
                           {:bufferView 2 :componentType 5125 :count (count indices) :type "SCALAR"}]}
          a (.createElement js/document "a") url (.createObjectURL js/URL (js/Blob. #js [(js/JSON.stringify (clj->js doc) nil 2)] #js {:type "model/gltf+json"}))]
      (set! (.-href a) url) (set! (.-download a) "model.gltf") (.click a)
      (js/setTimeout #(.revokeObjectURL js/URL url) 0))))

(defn- draw! []
  (when-let [{:keys [draws] :as viewport} @runtime]
    (when (seq draws)
      (let [{:keys [azimuth elevation]} @state
            d 6.0
            eye [(* d (js/Math.cos elevation) (js/Math.cos azimuth))
                 (* d (js/Math.sin elevation))
                 (* d (js/Math.cos elevation) (js/Math.sin azimuth))]]
        (gpu-mesh/render-scene! viewport draws eye [0 0 0]))))
  (js/requestAnimationFrame draw!))

(defn- init-gpu! [canvas]
  (-> (gpu-mesh/init-canvas! canvas)
      (.then (fn [viewport]
               (reset! runtime (assoc viewport :texture-cache {}))
               (refresh-mesh!)
               (set! (.-textContent (.getElementById js/document "gpu-status")) "")
               (set! (.-__kami_modeler_ready js/window) true)
               (draw!)))
      (.catch (fn [e]
                (set! (.-textContent (.getElementById js/document "gpu-status"))
                      (str "WebGPU unavailable: " e))))))

(defn- normalize [v] (let [l (max 1.0e-9 (js/Math.hypot (nth v 0) (nth v 1) (nth v 2)))] (mapv #(/ % l) v)))
(defn- add3 [& vs] (apply mapv + vs))
(defn- mul3 [v s] (mapv #(* % s) v))
(defn- dot3 [a b] (reduce + (map * a b)))
(defn- camera-eye [] (let [{:keys [azimuth elevation]} @state d 6] [(* d (js/Math.cos elevation) (js/Math.cos azimuth)) (* d (js/Math.sin elevation)) (* d (js/Math.cos elevation) (js/Math.sin azimuth))]))
(defn- project-point [canvas point]
  (let [rect (.getBoundingClientRect canvas) eye (camera-eye) forward (normalize (mapv - [0 0 0] eye))
        right (normalize (cross forward [0 1 0])) up (cross right forward) relative (mapv - point eye)
        depth (dot3 relative forward) tan (js/Math.tan (/ js/Math.PI 6)) aspect (/ (.-width rect) (.-height rect))]
    (when (pos? depth)
      [(+ (.-left rect) (* (.-width rect) (/ (inc (/ (dot3 relative right) (* depth tan aspect))) 2)))
       (+ (.-top rect) (* (.-height rect) (/ (- 1 (/ (dot3 relative up) (* depth tan))) 2)))])))
(defn- inside-box? [[x y] [x0 y0 x1 y1]] (and (<= x0 x x1) (<= y0 y y1)))
(defn- box-select! [canvas [ax ay] [bx by]]
  (let [bounds [(min ax bx) (min ay by) (max ax bx) (max ay by)] object (selected-object)
        mesh (:object/mesh object) scene (:scene @state) object-id (:object/id object)
        projected (mapv #(project-point canvas (modeling/transform-point-world scene object-id %)) (:mesh/vertices mesh))
        vertices (set (keep-indexed (fn [index point] (when (and point (inside-box? point bounds)) index)) projected))]
    (case (:component-mode @state)
      :vertex (swap! state assoc :selected-vertices vertices :selected-vertex (first vertices))
      :edge (let [edges (set (filter #(every? vertices %) (modeling/mesh-edges mesh)))]
              (swap! state assoc :selected-edges edges :selected-edge (first edges)))
      :face (let [faces (set (keep-indexed (fn [index face] (when (every? vertices face) index)) (:mesh/faces mesh)))]
              (swap! state assoc :selected-faces faces :selected-face (first faces))) nil)
    (update-ui!)))
(defn- update-selection-box! [[ax ay] [bx by]]
  (let [box (.getElementById js/document "selection-box")]
    (set! (.-display (.-style box)) "block")
    (set! (.-left (.-style box)) (str (min ax bx) "px"))
    (set! (.-top (.-style box)) (str (min ay by) "px"))
    (set! (.-width (.-style box)) (str (js/Math.abs (- bx ax)) "px"))
    (set! (.-height (.-style box)) (str (js/Math.abs (- by ay)) "px"))))
(defn- pick-at! [canvas event]
  (let [r (.getBoundingClientRect canvas) x (- (* 2 (/ (- (.-clientX event) (.-left r)) (.-width r))) 1)
        y (- 1 (* 2 (/ (- (.-clientY event) (.-top r)) (.-height r)))) eye (camera-eye)
        f (normalize (mapv - [0 0 0] eye)) right (normalize (cross f [0 1 0])) up (cross right f)
        tan (js/Math.tan (/ js/Math.PI 6)) dir (normalize (add3 f (mul3 right (* x (/ (.-width r) (.-height r)) tan)) (mul3 up (* y tan))))
        object (selected-object) local-eye (mapv - eye (:object/translation object))
        kind (:component-mode @state) picked (modeling/pick-element (:object/mesh object) local-eye dir kind)]
    (when (some? picked)
      (case kind :vertex (if (.-shiftKey event)
                          (swap! state (fn [s] (let [next (if (contains? (:selected-vertices s) picked)
                                                            (disj (:selected-vertices s) picked) (conj (:selected-vertices s) picked))]
                                               (assoc s :selected-vertex (when (seq next) picked) :selected-vertices next))))
                          (swap! state assoc :selected-vertex picked :selected-vertices #{picked}))
            :edge (if (.-shiftKey event)
                    (swap! state (fn [s] (let [next (if (contains? (:selected-edges s) picked)
                                                      (disj (:selected-edges s) picked) (conj (:selected-edges s) picked))]
                                         (assoc s :selected-edge (when (seq next) picked) :selected-edges next))))
                    (swap! state assoc :selected-edge picked :selected-edges #{picked}))
            :face (if (.-shiftKey event)
                    (swap! state (fn [s] (let [selected (:selected-faces s)
                                               next (if (contains? selected picked) (disj selected picked) (conj selected picked))]
                                           (assoc s :selected-face (if (seq next) picked nil) :selected-faces next))))
                    (swap! state assoc :selected-face picked :selected-faces #{picked})) nil)
      (update-ui!))))

(defn ^:export init! []
  (let [canvas (.getElementById js/document "gpu-canvas") drag (atom nil)]
    (add-watch state ::autosave
               (fn [_ _ previous current]
                 (when (and (= :dirty (:save-status current))
                            (not= (:revision previous) (:revision current)))
                   (schedule-autosave!))))
    (.addEventListener js/window "error" #(report-error! "Unexpected application error" (.-error %)))
    (.addEventListener js/window "unhandledrejection" #(report-error! "Operation failed" (.-reason %)))
    (.addEventListener js/window "beforeunload"
                       #(when (= :dirty (:save-status @state)) (save-project!)))
    (set! (.-__kami_large_scene_stress js/window) large-scene-stress!)
    (.addEventListener (.getElementById js/document "extrude") "click" extrude!)
    (.addEventListener (.getElementById js/document "inset") "click" inset!)
    (.addEventListener (.getElementById js/document "bevel") "click" bevel!)
    (.addEventListener (.getElementById js/document "loop-cut") "click" loop-cut!)
    (.addEventListener (.getElementById js/document "knife") "click" knife!)
    (.addEventListener (.getElementById js/document "flip-normals") "click" flip-normals!)
    (.addEventListener (.getElementById js/document "orient-outward") "click" orient-outward!)
    (.addEventListener (.getElementById js/document "texture-file") "change" import-texture!)
    (.addEventListener (.getElementById js/document "remove-texture") "click" remove-texture!)
    (.addEventListener (.getElementById js/document "transform-uv") "click" transform-uv-selection!)
    (doseq [[id operation] [["boolean-union" :union] ["boolean-intersection" :intersection] ["boolean-difference" :difference]]]
      (.addEventListener (.getElementById js/document id) "click" #(boolean-objects! operation)))
    (.addEventListener (.getElementById js/document "bridge") "click" bridge!)
    (.addEventListener (.getElementById js/document "select-all-faces") "click" select-all-components!)
    (.addEventListener (.getElementById js/document "box-select") "click"
                       #(do (swap! state update :selection-tool (fn [tool] (if (= tool :box) :pick :box)))
                            (.toggle (.-classList (.getElementById js/document "box-select")) "selected" (= :box (:selection-tool @state)))))
    (.addEventListener (.getElementById js/document "snap-selection") "click" snap-selection!)
    (.addEventListener (.getElementById js/document "snap-increment") "change"
                       #(swap! state assoc :snap (max 1.0e-6 (js/parseFloat (.. % -target -value))) :save-status :dirty))
    (doseq [axis [:x :y :z]]
      (doseq [prefix ["axis-" "gizmo-"]]
        (.addEventListener (.getElementById js/document (str prefix (name axis))) "click" #(set-transform-axis! axis))))
    (.addEventListener (.getElementById js/document "scale") "click" scale!)
    (.addEventListener (.getElementById js/document "move") "click" move!)
    (.addEventListener (.getElementById js/document "delete-face") "click" delete-face!)
    (.addEventListener (.getElementById js/document "undo") "click" undo!)
    (.addEventListener (.getElementById js/document "redo") "click" redo!)
    (.addEventListener (.getElementById js/document "save-project") "click" save-project!)
    (.addEventListener (.getElementById js/document "load-project") "click" load-project!)
    (.addEventListener (.getElementById js/document "distance") "input" #(swap! state assoc :distance (js/parseFloat (.. % -target -value))))
    (.addEventListener (.getElementById js/document "profile") "change" #(do (swap! state assoc :profile (keyword (.. % -target -value))) (update-ui!)))
    (.addEventListener canvas "contextmenu" #(.preventDefault %))
    (.addEventListener canvas "pointerdown"
                       #(cond (or (= 1 (.-button %)) (.-altKey %)) (reset! drag [(.-clientX %) (.-clientY %)])
                              (= :box (:selection-tool @state)) (do (reset! box-drag [(.-clientX %) (.-clientY %)])
                                                                    (update-selection-box! @box-drag @box-drag))
                              (edit-mode?) (pick-at! canvas %)))
    (.addEventListener js/window "pointerup"
                       #(do (reset! drag nil)
                            (when-let [start @box-drag]
                              (box-select! canvas start [(.-clientX %) (.-clientY %)])
                              (set! (.-display (.-style (.getElementById js/document "selection-box"))) "none")
                              (reset! box-drag nil))))
    (.addEventListener js/window "pointermove"
      (fn [event]
        (when-let [[x y] @drag]
          (swap! state update :azimuth + (* 0.008 (- (.-clientX event) x)))
          (swap! state update :elevation
                 (fn [e] (max -1.3 (min 1.3 (+ e (* 0.008 (- (.-clientY event) y)))))))
          (reset! drag [(.-clientX event) (.-clientY event)]))))
    (.addEventListener js/window "pointermove"
                       #(when-let [start @box-drag] (update-selection-box! start [(.-clientX %) (.-clientY %)])))
    (.addEventListener js/window "keydown"
                       #(when-not (editable-target? %)
                          (when-let [command (command-for-event %)]
                            (.preventDefault %) (execute-command! command))))
    (.addEventListener (.getElementById js/document "mode-toggle") "click" toggle-mode!)
    (doseq [kind [:vertex :edge :face]]
      (.addEventListener (.getElementById js/document (str "component-" (name kind))) "click" #(set-component-mode! kind)))
    (.addEventListener (.getElementById js/document "new-cube") "click"
                       #(let [id (:next-id @state) o (modeling/object id (str "Cube " id) cube {:translation [(* id 0.5) 0 0]})]
                          (swap! state assoc :selected-object id :selected-face 1 :selected-faces #{1} :next-id (inc id))
                          (commit-scene! (modeling/add-object (:scene @state) o))))
    (.addEventListener (.getElementById js/document "duplicate-object") "click" duplicate-object!)
    (.addEventListener (.getElementById js/document "delete-object") "click" delete-object!)
    (.addEventListener (.getElementById js/document "apply-transform") "click"
                       #(let [translation (mapv (fn [id] (js/parseFloat (.-value (.getElementById js/document id)))) ["tx" "ty" "tz"])
                              name (.-value (.getElementById js/document "object-name"))]
                          (commit-scene! (modeling/update-object (:scene @state) (:selected-object @state)
                                                                 (fn [o] (-> o (assoc :object/name name) (modeling/set-object-transform {:translation translation})))))))
    (.addEventListener (.getElementById js/document "object-parent") "change"
                       #(let [raw (.. % -target -value) parent-id (when (seq raw) (js/parseInt raw))]
                          (commit-scene! (modeling/reparent-object (:scene @state) (:selected-object @state) parent-id))))
    (.addEventListener (.getElementById js/document "base-color") "change"
                       #(let [material (assoc (:object/material (selected-object))
                                             :material/base-color (hex->color (.. % -target -value)))]
                          (commit-scene! (modeling/set-object-material (:scene @state) (:selected-object @state) material))))
    (doseq [[id key] [["metallic" :material/metallic] ["roughness" :material/roughness]]]
      (.addEventListener (.getElementById js/document id) "change"
                         #(let [material (assoc (:object/material (selected-object)) key
                                               (js/parseFloat (.. % -target -value)))]
                            (commit-scene! (modeling/set-object-material (:scene @state) (:selected-object @state) material)))))
    (doseq [[id kind options] [["add-mirror" :mirror {:axis :x}]
                               ["add-subdivision" :subdivision {:levels 1}]
                               ["add-array" :array {:count 3 :offset [2.5 0 0]}]]]
      (.addEventListener (.getElementById js/document id) "click"
                         #(commit-scene! (modeling/update-object (:scene @state) (:selected-object @state)
                                                                  modeling/add-modifier (modeling/modifier kind options)))))
    (let [presets {:translate {:offset [0.25 0 0]} :scale {:factors [1.1 1.1 1.1]}
                   :triangulate {} :flip-normals {} :weld {:tolerance 0.001}
                   :solidify {:thickness 0.1} :planar-unwrap {:axis :z}
                   :rotate {:angles [0 0 0.25]} :shear {:xy 0.1 :xz 0 :yx 0 :yz 0 :zx 0 :zy 0}
                   :taper {:axis :z :factor 0.1} :twist {:axis :z :factor 0.25}
                   :bend {:axis :x :factor 0.1} :spherize {:factor 0.5 :radius 2}
                   :decimate {:ratio 0.75} :remove-degenerate {:epsilon 1.0e-9}
                   :orient-outward {} :snap-grid {:size 0.1} :axis-project {:axis :z :value 0}
                   :center-origin {} :clamp {:min [-2 -2 -2] :max [2 2 2]}
                   :radial-wave {:amplitude 0.1 :frequency 2}
                   :deterministic-jitter {:amplitude 0.01 :seed 42}}]
      (.addEventListener (.getElementById js/document "add-catalog-modifier") "click"
                         #(let [kind (keyword (.-value (.getElementById js/document "modifier-kind")))]
                            (commit-scene! (modeling/update-object (:scene @state) (:selected-object @state)
                                                                   modeling/add-modifier
                                                                   (modeling/modifier kind (get presets kind)))))))
    (.addEventListener (.getElementById js/document "unwrap-uv") "click"
                       #(let [axis (keyword (.-value (.getElementById js/document "unwrap-axis")))]
                          (commit-mesh! (modeling/planar-unwrap (selected-mesh) axis))))
    (.addEventListener (.getElementById js/document "import") "click" #(.click (.getElementById js/document "import-file")))
    (.addEventListener (.getElementById js/document "import-gltf") "click" #(.click (.getElementById js/document "import-gltf-file")))
    (.addEventListener (.getElementById js/document "import-gltf-file") "change" import-gltf!)
    (.addEventListener (.getElementById js/document "import-file") "change"
                       (fn [event]
                         (when-let [file (aget (.. event -target -files) 0)]
                           (-> (.text file)
                               (.then (fn [text]
                                        (let [m (reader/read-string text)]
                                          (if (modeling/valid-mesh? m)
                                            (apply-project! (modeling/scene [(modeling/object 1 "Imported" m)]))
                                            (apply-project! m)))))))))
    (.addEventListener (.getElementById js/document "export") "click" download-project!)
    (.addEventListener (.getElementById js/document "export-gltf") "click" download-gltf!)
    (when (= "vehicle-physics" (.get (js/URLSearchParams. (.-search js/location)) "workspace"))
      (let [profile (.getElementById js/document "profile")
            option (.createElement js/document "option")]
        (set! (.-value option) "vehicle-physics")
        (set! (.-textContent option) "Vehicle Physics")
        (.appendChild profile option))
      (apply-project! (modeler-vehicle/open-document (modeler-vehicle/sample-document))))
    (update-ui!)
    (init-gpu! canvas)))
