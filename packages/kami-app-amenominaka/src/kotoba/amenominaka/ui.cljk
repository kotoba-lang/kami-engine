(ns kotoba.amenominaka.ui
  "M5 (ADR-2607100100): the interactive UI shell — real environment-preset
  controls (weather/terrain/postfx/vegetation) and an orbit/zoom camera
  over the WebGPU viewport, wired to M0-M2's already-landed pipeline
  (previously only reachable via a static single-frame demo).

  M8: a second camera mode, free-fly (WASD movement + mouse-look), toggled
  alongside orbit. Needed no new `kami.webgpu.ir`/`kami.webgpu` API at all
  — `rig->camera` was always just one way to produce `:eye`/`:target`;
  [[fly-eye-target]] below computes them directly from a
  position+yaw+pitch state the same way, and `apply-camera!` only ever
  cared about the resulting `:eye`/`:target` map, not which mode produced
  it — so M4's instance-buffer cache stays hot in fly mode exactly like
  it does in orbit mode (camera-only frames never touch `:instances`).

  M9: `omni.timeline` parity (D7's own scoping — \"minimal keyframe/camera-
  path\", see `kotoba.amenominaka.timeline`'s docstring). \"Record
  Keyframe\" captures wherever the camera currently is (orbit OR fly, both
  just produce an :eye/:target the same way M8 already established) into
  an ordered path; \"Play\" drives a real requestAnimationFrame loop that
  interpolates along it and overrides the render-IR's camera each frame —
  a third, temporary camera source layered over orbit/fly (see
  [[apply-timeline-at!]]), not a third `:camera-mode` value, since
  playback is a transient override, not a persistent mode the user
  toggles the same way.

  Built on the org's DEFAULT UI/UX design system per ADR-2607022800:
  `shitsuke` (structure) + `liquid-glass-ui` (material) + `kotoba-ui`
  (single require-point) + `appkit` (desktop binding — this is a panels/
  toolbar/sliders desktop-style app, not touch/card-first, so `appkit`
  over `uikit`). This is the FIRST consumer wiring that design system to
  a WebGPU-canvas-centric app (confirmed via repo-wide grep before
  starting) — but the pattern itself (DOM chrome siblings around a
  `<canvas>` leaf node) is precedented by `kami-engine-hud`, and
  shitsuke/kotoba-ui are DOM-only by design (`liquid-glass.gpu`, true
  in-canvas glass rendering, is explicitly future work upstream) which is
  exactly the shape this app needs, not a mismatch.

  `kotoba-ui`'s `menu-select`/`button` are uncontrolled-in-React (real,
  confirmed bugs — a `<select>`/`<option>` with no `:value`/`:key` React
  props, and `button` has no `:on-click` at all, only shitsuke's SSR
  `:act` contract). [[preset-select]]/[[btn]] below are small hand-rolled
  controlled replacements with the SAME DOM shape (so the generated CSS
  still targets them correctly) — this exact workaround, including the
  DOM-shape-preservation reasoning, is copied from
  `murakumo-studio/src/murakumo_studio/ui.cljs`'s real, live-reproduced
  fix for the identical bug."
  (:require [cljs.reader :as reader]
            [kotoba.lang.text :as string]
            [reagent.core :as r]
            [reagent.dom.client :as rdomc]
            [appkit.core :as shape]
            [kotoba-ui.core :as ui]
            [bim]
            [kotoba.amenominaka.scene :as amenominaka-scene]
            [kotoba.amenominaka.project :as project]
            [kotoba.amenominaka.render-ir :as render-ir]
            [kotoba.amenominaka.usd-export :as usd-export]
            [kotoba.amenominaka.gltf-export :as gltf-export]
            [kotoba.amenominaka.timeline :as tl]
            [kami.webgpu :as webgpu]
            [kami.webgpu.ir :as ir]))

;; ── controlled-component workarounds (see ns docstring) ──

(defn- btn
  ([label on-click] (btn label on-click nil))
  ([label on-click id]
   [:button {:id id :class (ui/class-name :button) :type "button" :on-click on-click} label]))

(defn- preset-select [options {:keys [id value on-change]}]
  [:div {:class (ui/class-name :menu-select)}
   [:select {:id id :value (or value "") :on-change on-change}
    (for [[v l] options] ^{:key v} [:option {:value v} l])]
   [:span {:aria-hidden true :class (ui/class-name :specular)}]])

;; ── the sample building (same fixture M0-M4's own tests/demos use) ──

(defn- sample-building []
  (-> (bim/project "Quarry Walk Lodge")
      (update :sites conj
              (bim/site
               {:id 1 :name "Site 1" :geo nil :placement :identity
                :buildings
                [(bim/building
                  {:id 2 :name "Lodge" :placement :identity :reference-elevation 0.0
                   :storeys
                   [(bim/storey
                     {:id 3 :name "L1" :elevation 0.0 :height 3.5 :placement :identity
                      :spaces []
                      :elements
                      [(bim/element
                        {:id 4 :kind :wall :name "Wall 1" :global-id "GUID-0004"
                         :placement :identity
                         :geometry (bim/axis-sweep-geometry
                                    [[0.0 0.0 0.0] [10.0 0.0 0.0]]
                                    (bim/rectangle-profile 0.2 3.5))})]})]})]}))))

;; ── real preset ids, per each domain's own shipped EDN (verified by
;; reading kami-atmosphere-scene/kami-terrain-scene/kami-vegetation-scene/
;; kami-postfx-scene's own `*-edn` source, not guessed) ──

(def weather-options [["overcast" "Overcast"] ["clear" "Clear"]])
(def terrain-options [["plains" "Plains"] ["quarry" "Quarry"] ["desert" "Desert"] ["tundra" "Tundra"]])
(def postfx-options [["nintendo" "Nintendo"] ["retro" "Retro"]
                      ["final-fantasy" "Final Fantasy"] ["baminiku-character" "Baminiku Character"]])
(def vegetation-options [["grass" "Grass"] ["fern" "Fern"] ["palm" "Palm"]
                          ["conifer" "Conifer"] ["bush" "Bush"] ["cactus" "Cactus"] ["moss" "Moss"]])

;; ── state: plain reagent atom + swap! (the real pattern in every actual
;; kotoba-ui/appkit consumer found — shitsuke.re-frame.core exists as a
;; portable seam but has zero real consumers) ──

(defonce state
  (r/atom {:weather "overcast" :terrain "plains" :postfx "nintendo" :vegetation ["grass"]
           :building (sample-building) :selected-element 4 :next-element-id 5
           :project-id "quarry-walk-lodge" :project-name "Quarry Walk Lodge" :project-revision 0 :save-status :clean
           :render-export-status :idle
           :video-export-status :idle :video-recorder nil :video-fps 30 :video-bitrate-mbps 8
           :building-history [] :building-future []
           :interaction-profile :twinmotion
           :camera-mode :orbit
           :camera {:azimuth 0.785 :distance 64.0 :height 55.0}
           :fly {:pos [0.0 5.0 0.0] :yaw 0.0 :pitch 0.0}
           :fly-keys #{}
           :timeline [] :playhead 0.0 :playing? false :selected-keyframe nil
           :pivot nil
           :render-ir nil
           :webgpu-ctx nil
           :dragging? false :last-x 0 :last-y 0}))

;; ── scene recompute + camera application ──
;;
;; apply-camera! only assoc-in's :eye/:target into the ALREADY-COMPUTED
;; render-ir — it never touches :instances, so kotoba-lang/webgpu's M4
;; instance-buffer cache (ADR-2607100100, ~30-40x at scale) stays hot
;; across every camera-only frame (drag/wheel); only a real preset change
;; (recompute-scene!) rebuilds :instances and pays a fresh upload.

(defn- fly-eye-target
  "Free-fly `:eye`/`:target` from a `{:pos [x y z] :yaw :pitch}` state —
  the same job `ir/rig->camera` does for orbit, just a different (and
  simpler) mapping: `:eye` is `:pos` directly, `:target` is one unit ahead
  along the look direction (yaw about Y, pitch about the local right
  axis). Only the direction matters for a look-at target, so the fixed
  1-unit distance is arbitrary."
  [{:keys [pos yaw pitch]}]
  (let [[px py pz] pos
        cp (js/Math.cos pitch) sp (js/Math.sin pitch)
        sy (js/Math.sin yaw) cy (js/Math.cos yaw)]
    {:eye pos
     :target [(+ px (* cp sy)) (+ py sp) (+ pz (* cp cy))]}))

(defn- apply-camera! []
  (let [{:keys [render-ir camera camera-mode pivot fly webgpu-ctx]} @state]
    (when (and render-ir (or pivot (= camera-mode :fly)))
      (let [{:keys [eye target]} (if (= camera-mode :fly)
                                    (fly-eye-target fly)
                                    (ir/rig->camera camera pivot))
            ir' (-> render-ir (assoc-in [:globals :eye] eye) (assoc-in [:globals :target] target))]
        (swap! state assoc :render-ir ir')
        (when webgpu-ctx (webgpu/draw! webgpu-ctx ir'))))))

(defn- current-scene []
  (let [{:keys [weather terrain postfx vegetation building]} @state]
    (amenominaka-scene/compose {:building building
                                 :weather weather :terrain terrain :postfx postfx
                                 :vegetation vegetation})))

(defn- recompute-scene! []
  (let [base-ir (render-ir/scene->render-ir (current-scene))
        target (get-in base-ir [:globals :target])
        pivot [(nth target 0) (nth target 2)]]
    (swap! state assoc :render-ir base-ir :pivot pivot)
    (apply-camera!)))

;; ── semantic scene editing ──

(defn- current-storey [] (bim/find-storey (:building @state) 3))
(defn- scene-elements [] (:elements (current-storey)))
(defn- selected-element []
  (first (filter #(= (:selected-element @state) (:id %)) (scene-elements))))
(defn- project-document []
  (let [{:keys [project-id project-name building weather terrain postfx vegetation camera-mode camera fly timeline]} @state]
    (project/document {:id project-id :name project-name :building building
                       :environment {:weather weather :terrain terrain :postfx postfx :vegetation vegetation}
                       :camera {:mode camera-mode :orbit camera :fly fly}
                       :timeline timeline :updated-at (:project-revision @state)})))
(defn- persist-project! []
  (let [primary "amenominaka.project.v2" backup "amenominaka.project.backup"
        serialized (pr-str (project-document)) old (.getItem js/localStorage primary)]
    (when old (.setItem js/localStorage backup old))
    (.setItem js/localStorage primary serialized)
    (swap! state assoc :save-status :saved)))
(defn- mark-changed! []
  (swap! state (fn [s] (-> s (update :project-revision inc) (assoc :save-status :dirty))))
  (persist-project!))
(defn- commit-building! [building]
  (swap! state (fn [s] (-> s
                            (update :building-history conj (:building s))
                            (assoc :building building :building-future []))))
  (recompute-scene!)
  (mark-changed!))
(defn- add-wall! []
  (let [id (:next-element-id @state) y (* 1.5 (- id 4))
        wall (bim/wall {:id id :name (str "Wall " id) :start [0 y 0] :end [8 y 0]
                        :thickness 0.2 :height 3.5 :material "Concrete"})]
    (swap! state assoc :selected-element id)
    (swap! state update :next-element-id inc)
    (commit-building! (bim/add-element (:building @state) 3 wall))))
(defn- delete-selected! []
  (when-let [e (selected-element)]
    (commit-building! (bim/delete-element (:building @state) 3 (:id e)))
    (swap! state assoc :selected-element (some-> (first (scene-elements)) :id))))
(defn- update-selected! [field value]
  (when-let [e (selected-element)]
    (let [[[x y z] [x1 _ _]] (get-in e [:geometry :axis] [[0 0 0] [8 0 0]])
          length (if (= field :length) value (- x1 x))
          height (if (= field :height) value (get-in e [:geometry :profile :height] 3.5))
          thickness (if (= field :thickness) value (get-in e [:geometry :profile :thickness] 0.2))
          name (if (= field :name) value (:name e))
          updated (bim/wall {:id (:id e) :name name :start [x y z] :end [(+ x length) y z]
                             :height height :thickness thickness :material "Concrete"})]
      (commit-building! (bim/update-element (:building @state) 3 (:id e) (constantly updated))))))
(defn- undo-building! []
  (when-let [previous (peek (:building-history @state))]
    (swap! state (fn [s] (assoc s :building previous
                                :building-history (pop (:building-history s))
                                :building-future (conj (:building-future s) (:building s)))))
    (recompute-scene!)
    (mark-changed!)))
(defn- redo-building! []
  (when-let [next (peek (:building-future @state))]
    (swap! state (fn [s] (assoc s :building next
                                :building-future (pop (:building-future s))
                                :building-history (conj (:building-history s) (:building s)))))
    (recompute-scene!)
    (mark-changed!)))
(defn- save-project! [] (mark-changed!))
(defn- apply-document! [doc]
  (let [doc (project/migrate doc) env (:project/environment doc) cam (:project/camera doc)
        building (:project/building doc)]
    (swap! state assoc :project-id (:project/id doc) :project-name (:project/name doc)
           :project-revision (:project/updated-at doc) :save-status :loaded
           :building building :weather (:weather env "overcast") :terrain (:terrain env "plains")
           :postfx (:postfx env "nintendo") :vegetation (:vegetation env ["grass"])
           :camera-mode (:mode cam :orbit) :camera (:orbit cam (:camera @state)) :fly (:fly cam (:fly @state))
           :timeline (:project/timeline doc) :building-history [] :building-future []
           :selected-element (some-> (first (:elements (bim/find-storey building 3))) :id))
    (recompute-scene!)))
(defn- load-project! []
  (when-let [stored (or (.getItem js/localStorage "amenominaka.project.v2")
                        (.getItem js/localStorage "amenominaka.project.backup")
                        (.getItem js/localStorage "amenominaka.project"))]
    (try (apply-document! (reader/read-string stored))
         (catch :default _
           (when-let [backup (.getItem js/localStorage "amenominaka.project.backup")]
             (apply-document! (reader/read-string backup)))))))
(defn- export-project! []
  (let [a (.createElement js/document "a") blob (js/Blob. #js [(pr-str (project-document))] #js {:type "application/edn"})]
    (set! (.-href a) (.createObjectURL js/URL blob)) (set! (.-download a) (str (:project-id @state) ".amenominaka.edn")) (.click a)))
(defn- import-project! [event]
  (when-let [file (aget (.. event -target -files) 0)]
    (-> (.text file) (.then #(apply-document! (reader/read-string %))))))

;; ── mouse/wheel orbit camera (M2's own gap: "interactive orbit/fly
;; camera controls are not implemented" — closed here (orbit), extended
;; to a second free-fly mode in M8) ──

(defn- on-mouse-down [e]
  (swap! state assoc :dragging? true :last-x (.-clientX e) :last-y (.-clientY e)))

(defn- on-mouse-move [e]
  (when (:dragging? @state)
    (let [{:keys [last-x last-y camera-mode]} @state
          dx (- (.-clientX e) last-x)
          dy (- (.-clientY e) last-y)]
      (swap! state assoc :last-x (.-clientX e) :last-y (.-clientY e))
      (if (= camera-mode :fly)
        ;; mouse-look: drag right -> look right (yaw+), drag up -> look up
        ;; (pitch+, screen Y grows downward so this is a subtraction).
        ;; Pitch clamped short of ±90° to avoid a gimbal flip through the pole.
        (swap! state update :fly
               (fn [f] (-> f
                           (update :yaw + (* dx 0.005))
                           (update :pitch #(-> % (- (* dy 0.005)) (max -1.5) (min 1.5))))))
        (swap! state update :camera
               (fn [c] (-> c
                           (update :azimuth + (* dx 0.01))
                           (update :height #(max 5.0 (- % (* dy 0.5))))))))
      (apply-camera!))))

(defn- on-mouse-up [_e] (swap! state assoc :dragging? false))

(defn- on-wheel [e]
  (.preventDefault e)
  (when (= (:camera-mode @state) :orbit)
    (swap! state update :camera update :distance
           (fn [d] (max 10.0 (min 400.0 (+ d (* (.-deltaY e) 0.1))))))
    (apply-camera!)))

;; ── free-fly keyboard movement (M8) ──
;;
;; WASD moves relative to the current look direction, projected onto the
;; horizontal plane (yaw only — ignoring pitch, so looking down while
;; pressing W walks forward instead of flying into the ground, standard
;; FPS-camera behavior); Space/Shift move straight up/down. A real
;; requestAnimationFrame loop (not per-keydown deltas) so movement is
;; smooth and framerate-independent while a key is held — started when
;; entering fly mode, self-terminating (checks :camera-mode each tick)
;; when leaving it.

(def ^:private fly-speed 12.0) ;; world units / second

(def ^:private key->action
  {"w" :forward "s" :backward "a" :left "d" :right
   " " :up "shift" :down})

(def ^:private profile-shortcuts
  {:twinmotion {"m" :toggle-camera "k" :record-keyframe "1" :overcast "2" :clear
                "u" :export-usd "g" :export-gltf "r" :export-png "p" :toggle-playback}
   :lumion {"f" :toggle-camera "p" :record-keyframe "1" :overcast "2" :clear
            "u" :export-usd "g" :export-gltf "r" :export-png "k" :toggle-playback}
   :d5-render {"f" :toggle-camera "k" :record-keyframe "1" :overcast "2" :clear
               "u" :export-usd "g" :export-gltf "r" :export-png "p" :toggle-playback}
   :enscape {"f" :toggle-camera "k" :record-keyframe "1" :overcast "2" :clear
             "u" :export-usd "g" :export-gltf "r" :export-png "p" :toggle-playback}})

(declare toggle-camera-mode! record-keyframe! download-usda! download-glb! download-png! toggle-video-recording!
         download-blob! play-timeline! pause-timeline!)

(defn- editable-target? [e]
  (let [target (.-target e)
        tag (some-> target .-tagName .toLowerCase)]
    (or (= tag "input") (= tag "textarea") (= tag "select")
        (true? (.-isContentEditable target)))))

(defn- execute-profile-command! [command]
  (case command
    :toggle-camera (toggle-camera-mode!)
    :record-keyframe (record-keyframe!)
    :overcast (do (swap! state assoc :weather "overcast") (recompute-scene!) (mark-changed!))
    :clear (do (swap! state assoc :weather "clear") (recompute-scene!) (mark-changed!))
    :export-usd (download-usda!)
    :export-gltf (download-glb!)
    :export-png (download-png!)
    :toggle-playback (if (:playing? @state) (pause-timeline!) (play-timeline!))
    nil))

(defn- on-keydown [e]
  (when-not (editable-target? e)
    (let [key (.toLowerCase (.-key e))
          command (get-in profile-shortcuts [(:interaction-profile @state) key])]
      (if command
        (do (.preventDefault e) (execute-profile-command! command))
        (when (= (:camera-mode @state) :fly)
          (when-let [action (key->action key)]
            (.preventDefault e)
            (swap! state update :fly-keys conj action)))))))

(defn- on-keyup [e]
  (when-let [action (key->action (.toLowerCase (.-key e)))]
    (swap! state update :fly-keys disj action)))

(defn- move-fly! [dt]
  (let [{:keys [fly fly-keys]} @state
        {:keys [yaw]} fly
        held (fn [a] (contains? fly-keys a))
        forward [(js/Math.sin yaw) 0.0 (js/Math.cos yaw)]
        right [(js/Math.cos yaw) 0.0 (- (js/Math.sin yaw))]
        move-x (+ (if (held :right) 1.0 0.0) (if (held :left) -1.0 0.0))
        move-z (+ (if (held :forward) 1.0 0.0) (if (held :backward) -1.0 0.0))
        move-y (+ (if (held :up) 1.0 0.0) (if (held :down) -1.0 0.0))
        step (* fly-speed dt)]
    (when-not (zero? (+ (js/Math.abs move-x) (js/Math.abs move-z) (js/Math.abs move-y)))
      (swap! state update-in [:fly :pos]
             (fn [[px py pz]]
               [(+ px (* step (+ (* move-x (nth right 0)) (* move-z (nth forward 0)))))
                (+ py (* step move-y))
                (+ pz (* step (+ (* move-x (nth right 2)) (* move-z (nth forward 2)))))]))
      (apply-camera!))))

(defonce fly-loop-running? (atom false))

(defn- fly-loop! [last-t]
  (when (= (:camera-mode @state) :fly)
    (let [now (js/performance.now)]
      (move-fly! (/ (- now last-t) 1000.0))
      (js/requestAnimationFrame (fn [] (fly-loop! now)))))
  (when-not (= (:camera-mode @state) :fly) (reset! fly-loop-running? false)))

(defn- start-fly-loop! []
  (when-not @fly-loop-running?
    (reset! fly-loop-running? true)
    (js/requestAnimationFrame (fn [] (fly-loop! (js/performance.now))))))

(defn- toggle-camera-mode! []
  (let [{:keys [camera-mode render-ir]} @state]
    (if (= camera-mode :fly)
      (swap! state assoc :camera-mode :orbit)
      (do
        ;; hand off smoothly: seed fly's pos/yaw from wherever orbit's
        ;; eye/target currently are, so toggling mid-orbit doesn't jump.
        (let [eye (get-in render-ir [:globals :eye] [0.0 5.0 0.0])
              target (get-in render-ir [:globals :target] [0.0 0.0 0.0])
              [ex ey ez] eye [tx ty tz] target
              dx (- tx ex) dz (- tz ez)
              yaw (js/Math.atan2 dx dz)
              horiz (js/Math.sqrt (+ (* dx dx) (* dz dz)))
              pitch (js/Math.atan2 (- ty ey) (max 1e-6 horiz))]
          (swap! state assoc :camera-mode :fly
                 :fly {:pos eye :yaw yaw :pitch (-> pitch (max -1.5) (min 1.5))}))
        (start-fly-loop!)))
    (apply-camera!)))

;; ── omni.timeline parity: minimal keyframe/camera-path (M9) ──
;;
;; apply-timeline-at! pushes eye/target straight into the render-IR the
;; same way apply-camera! does — it deliberately does NOT go through
;; apply-camera!/camera-mode, since playback is a transient override of
;; whatever the camera is currently doing, not a mode switch (orbit/fly
;; state is left untouched underneath and resumes as-is once playback/
;; scrubbing stops).

(defn- apply-timeline-at! [t]
  (let [{:keys [render-ir timeline webgpu-ctx]} @state]
    (when-let [{:keys [eye target]} (tl/eval-at timeline t)]
      (let [ir' (-> render-ir (assoc-in [:globals :eye] eye) (assoc-in [:globals :target] target))]
        (swap! state assoc :render-ir ir' :playhead t)
        (when webgpu-ctx (webgpu/draw! webgpu-ctx ir'))))))

(defn- record-keyframe! []
  (let [{:keys [render-ir]} @state
        eye (get-in render-ir [:globals :eye])
        target (get-in render-ir [:globals :target])]
    (when (and eye target)
      (swap! state update :timeline tl/add-keyframe eye target)
      (swap! state assoc :selected-keyframe (dec (count (:timeline @state))))
      (mark-changed!))))

(defn- clear-timeline! []
  (swap! state assoc :timeline [] :playhead 0.0 :playing? false :selected-keyframe nil)
  (mark-changed!))
(defn- delete-camera-key! [index]
  (swap! state update :timeline tl/delete-keyframe index)
  (swap! state assoc :selected-keyframe nil :playhead (min (:playhead @state) (tl/duration (:timeline @state))))
  (mark-changed!))
(defn- move-camera-key! [index time]
  (try (do (swap! state update :timeline tl/move-keyframe index time)
           (swap! state assoc :playhead time :selected-keyframe index)
           (apply-timeline-at! time) (mark-changed!))
       (catch :default _ nil)))
(defn- camera-path-csv []
  (str "time,eye_x,eye_y,eye_z,target_x,target_y,target_z\n"
       (string/join "\n" (map (fn [{:keys [t eye target]}]
                                  (string/join "," (concat [t] eye target))) (:timeline @state)))))
(defn- download-camera-path! []
  (download-blob! (js/Blob. #js [(camera-path-csv)] #js {:type "text/csv;charset=utf-8"}) "camera-path.csv"))

(defonce timeline-loop-running? (atom false))

(defn- timeline-loop! [last-t]
  (let [{:keys [playing? playhead timeline]} @state
        dur (tl/duration timeline)]
    (if (and playing? (< playhead dur))
      (let [now (js/performance.now)
            t' (min dur (+ playhead (/ (- now last-t) 1000.0)))]
        (apply-timeline-at! t')
        (js/requestAnimationFrame (fn [] (timeline-loop! now))))
      (do (swap! state assoc :playing? false)
          (reset! timeline-loop-running? false)
          (when (= :recording (:video-export-status @state)) (toggle-video-recording!))))))

(defn- play-timeline! []
  (let [{:keys [timeline playhead]} @state]
    (when (and (seq timeline) (not @timeline-loop-running?))
      ;; restart from the top if already at (or past) the end
      (when (>= playhead (tl/duration timeline)) (swap! state assoc :playhead 0.0))
      (swap! state assoc :playing? true)
      (reset! timeline-loop-running? true)
      (js/requestAnimationFrame (fn [] (timeline-loop! (js/performance.now)))))))

(defn- pause-timeline! [] (swap! state assoc :playing? false))

(defn- scrub-timeline! [t] (apply-timeline-at! t))

;; ── USD export (wires M1's kotoba.amenominaka.usd-export to a real
;; browser download, using the CURRENTLY selected presets) ──

(defn- download-blob! [blob filename]
  (let [url (js/URL.createObjectURL blob)
        a (js/document.createElement "a")]
    (set! (.-href a) url)
    (set! (.-download a) filename)
    (js/document.body.appendChild a)
    (.click a)
    (js/document.body.removeChild a)
    (js/URL.revokeObjectURL url)))

(defn- download-usda! []
  (download-blob! (js/Blob. #js [(usd-export/scene->usda (current-scene))] #js {:type "text/plain"})
                   "scene.usda"))

;; ── glTF export (M7: wires kotoba.amenominaka.gltf-export to a real
;; browser download, using the CURRENTLY selected presets — same pattern
;; as USD export above, just a binary Blob instead of a text one). ──

(defn- download-glb! []
  (let [bytes (gltf-export/scene->glb (current-scene))
        blob (js/Blob. #js [bytes] #js {:type "model/gltf-binary"})]
    (download-blob! blob "scene.glb")))
(defn- download-png! []
  (let [canvas (.getElementById js/document "viewport")
        filename (str (-> (:project-name @state) .toLowerCase (string/replace #"[^a-z0-9]+" "-") (string/replace #"(^-|-$)" "")) ".png")]
    (swap! state assoc :render-export-status :rendering)
    (apply-camera!)
    (js/requestAnimationFrame
     (fn [] (.toBlob canvas
                     (fn [blob]
                       (if blob
                         (do (download-blob! blob filename) (swap! state assoc :render-export-status :exported))
                         (swap! state assoc :render-export-status :failed)))
                     "image/png")))))

(defn- toggle-video-recording! []
  (if-let [recorder (:video-recorder @state)]
    (when (= "recording" (.-state recorder)) (.stop recorder))
    (let [canvas (.getElementById js/document "viewport")]
      (if (and canvas (.-captureStream canvas) (exists? js/MediaRecorder))
        (let [fps (:video-fps @state) bitrate (* 1000000 (:video-bitrate-mbps @state))
              stream (.captureStream canvas fps)
              mime (cond (.isTypeSupported js/MediaRecorder "video/webm;codecs=vp9") "video/webm;codecs=vp9"
                         (.isTypeSupported js/MediaRecorder "video/webm;codecs=vp8") "video/webm;codecs=vp8"
                         :else "video/webm")
              recorder (js/MediaRecorder. stream #js {:mimeType mime :videoBitsPerSecond bitrate})
              chunks (array)]
          (set! (.-ondataavailable recorder) #(when (pos? (.-size (.-data %))) (.push chunks (.-data %))))
          (set! (.-onstop recorder)
                (fn []
                  (doseq [track (array-seq (.getTracks stream))] (.stop track))
                  (download-blob! (js/Blob. chunks #js {:type mime})
                                  (str (-> (:project-name @state) .toLowerCase
                                           (string/replace #"[^a-z0-9]+" "-")
                                           (string/replace #"(^-|-$)" "")) ".webm"))
                  (swap! state assoc :video-export-status :exported :video-recorder nil)))
          (.start recorder 250)
          (swap! state assoc :video-export-status :recording :video-recorder recorder)
          (when (seq (:timeline @state))
            (swap! state assoc :playhead 0.0)
            (apply-timeline-at! 0.0)
            (play-timeline!)))
        (swap! state assoc :video-export-status :unsupported)))))

;; ── mount / view ──

(defn- on-canvas-ref [node]
  (when node
    (-> (webgpu/init! node)
        (.then (fn [ctx]
                 (swap! state assoc :webgpu-ctx ctx)
                 (recompute-scene!)))
        (.catch (fn [err] (js/console.error "webgpu/init! failed:" err))))))

(defn- viewport []
  [:canvas {:id "viewport" :width 1024 :height 768
            :ref on-canvas-ref
            :on-mouse-down on-mouse-down
            :on-mouse-move on-mouse-move
            :on-mouse-up on-mouse-up
            :on-mouse-leave on-mouse-up
            :on-wheel on-wheel}])

(defn- preset-field [label-text options value-key]
  (let [id (str "field-" (name value-key))]
    [:<>
     [:label {:html-for id} label-text]
     [preset-select options
      {:id id
       :value (if (= value-key :vegetation) (first (:vegetation @state)) (get @state value-key))
       :on-change (fn [e]
                    (let [v (.. e -target -value)]
                      (if (= value-key :vegetation)
                        (swap! state assoc :vegetation [v])
                        (swap! state assoc value-key v)))
                    (recompute-scene!)
                    (mark-changed!))}]]))

(defn- camera-mode-button []
  (let [mode (:camera-mode @state)]
    [btn (str "Camera: " (if (= mode :fly) "Fly (WASD)" "Orbit"))
     (fn [_e] (toggle-camera-mode!))
     "toggle-camera-mode"]))

(def tool-apps
  [["Modeler" "https://kotoba-lang.github.io/kami-app-modeler/"]
   ["Animator" "https://kotoba-lang.github.io/kami-app-animator/"]
   ["CAD" "https://kotoba-lang.github.io/kami-app-cad/"]
   ["BIM Editor" "https://kotoba-lang.github.io/kami-app-bim-editor/"]
   ["Sculpt" "https://kotoba-lang.github.io/kami-app-sculpt/"]])

(defn- tools-panel []
  [shape/panel
   [:div {:class "am-tools-panel"}
    [:h3 "Creative Tools"]
    (for [[label href] tool-apps]
      ^{:key label} [:a {:class (ui/class-name :button) :href href} label])]
   {:surface :thick :elevation :flat}])

(defn- scene-editor-panel []
  (let [selected (selected-element)
        length (get-in selected [:quantities :length-m] 0)
        height (get-in selected [:geometry :profile :height] 0)
        thickness (get-in selected [:geometry :profile :thickness] 0)]
    [shape/panel
     [:div {:class "am-scene-editor"}
      [:h3 "Scene Hierarchy"]
      [:div {:id "scene-tree"}
       (for [e (scene-elements)]
         ^{:key (:id e)}
         [:button {:type "button" :class (str (ui/class-name :button)
                                               (when (= (:id e) (:selected-element @state)) " selected"))
                   :on-click #(swap! state assoc :selected-element (:id e))}
          (str (name (:kind e)) " · " (:name e))])]
      [:div {:class "am-scene-actions"}
       [btn "Add Wall" (fn [_] (add-wall!)) "add-scene-wall"]
       [btn "Delete" (fn [_] (delete-selected!)) "delete-scene-element"]]
      (when selected
        [:div {:class "am-property-editor"}
         [:label "Name"]
         [:input {:id "element-name" :default-value (:name selected)
                  :key (str "name-" (:id selected) "-" (:name selected))
                  :on-blur #(update-selected! :name (.. % -target -value))}]
         [:label "Length (m)"]
         [:input {:id "element-length" :type "number" :step 0.1 :default-value length
                  :key (str "length-" (:id selected) "-" length)
                  :on-blur #(update-selected! :length (js/parseFloat (.. % -target -value)))}]
         [:label "Height (m)"]
         [:input {:id "element-height" :type "number" :step 0.1 :default-value height
                  :key (str "height-" (:id selected) "-" height)
                  :on-blur #(update-selected! :height (js/parseFloat (.. % -target -value)))}]
         [:label "Thickness (m)"]
         [:input {:id "element-thickness" :type "number" :step 0.05 :default-value thickness
                  :key (str "thickness-" (:id selected) "-" thickness)
                  :on-blur #(update-selected! :thickness (js/parseFloat (.. % -target -value)))}]])
      [:div {:class "am-scene-actions"}
       [btn "Undo" (fn [_] (undo-building!)) "undo-scene"]
       [btn "Redo" (fn [_] (redo-building!)) "redo-scene"]
       [btn "Save" (fn [_] (save-project!)) "save-project"]
       [btn "Load" (fn [_] (load-project!)) "load-project"]
       [btn "Export Project" (fn [_] (export-project!)) "export-project"]
       [btn "Import Project" (fn [_] (.click (.getElementById js/document "import-project-file"))) "import-project"]
       [:input {:id "import-project-file" :type "file" :accept ".edn,.amenominaka.edn" :style {:display "none"}
                :on-change import-project!}]]]
     {:surface :thick :elevation :flat}]))

(defn- env-panel []
  [shape/panel
   [:div {:class "am-env-panel"}
    [:h3 "Environment"]
    [:label {:for "interaction-profile"} "Interaction profile"]
    [:select {:id "interaction-profile"
              :value (name (:interaction-profile @state))
              :on-change #(swap! state assoc :interaction-profile
                                 (keyword (.. % -target -value)))}
     [:option {:value "twinmotion"} "Twinmotion"]
     [:option {:value "lumion"} "Lumion"]
     [:option {:value "d5-render"} "D5 Render"]
     [:option {:value "enscape"} "Enscape"]]
    [:p {:class "am-shortcut-hint"}
     (case (:interaction-profile @state)
       :twinmotion "M camera · K keyframe · P play · 1/2 weather · R PNG · U/G scene"
       :lumion "F camera · P keyframe · K play · 1/2 weather · R PNG · U/G scene"
       "F camera · K keyframe · P play · 1/2 weather · R PNG · U/G scene")]
    [preset-field "Weather" weather-options :weather]
    [preset-field "Terrain" terrain-options :terrain]
    [preset-field "Post FX" postfx-options :postfx]
    [preset-field "Vegetation" vegetation-options :vegetation]
    [:label {:for "video-fps"} "Video frame rate"]
    [:select {:id "video-fps" :value (:video-fps @state)
              :on-change #(swap! state assoc :video-fps (js/parseInt (.. % -target -value)))}
     [:option {:value 24} "24 fps"] [:option {:value 30} "30 fps"] [:option {:value 60} "60 fps"]]
    [:label {:for "video-bitrate"} "Video bitrate (Mbps)"]
    [:input {:id "video-bitrate" :type "number" :min 1 :max 50 :step 1 :value (:video-bitrate-mbps @state)
             :on-change #(swap! state assoc :video-bitrate-mbps (-> (js/parseInt (.. % -target -value)) (max 1) (min 50)))}]
    [:div {:class "am-export-row"}
     [camera-mode-button]
     [btn "Export PNG" (fn [_e] (download-png!)) "export-png"]
     [btn (if (= :recording (:video-export-status @state)) "Stop Video" "Record WebM")
      (fn [_e] (toggle-video-recording!)) "export-video"]
     [btn "Export USD" (fn [_e] (download-usda!)) "export-usd"]
     [btn "Export glTF" (fn [_e] (download-glb!)) "export-gltf"]]]
   {:surface :thick :elevation :flat}])

;; ── omni.timeline UI (M9) — record the current camera view as a
;; keyframe, scrub/play back the resulting path. ──

(defn- timeline-panel []
  (let [{:keys [timeline playhead playing?]} @state
        dur (tl/duration timeline)]
    [shape/panel
     [:div {:class "am-timeline-panel"}
      [:h3 "Camera Path"]
      [:div {:class "am-timeline-info"}
       (str (count timeline) " keyframe" (when (not= 1 (count timeline)) "s")
            ", " (.toFixed dur 1) "s")]
      [:input {:id "timeline-scrub" :type "range" :min 0 :max (max dur 0.001) :step 0.01
               :value playhead :disabled (zero? dur)
               :on-change (fn [e] (scrub-timeline! (js/parseFloat (.. e -target -value))))}]
      [:div {:class "am-timeline-row"}
       [btn "Record Keyframe" (fn [_e] (record-keyframe!)) "record-keyframe"]
       [btn (if playing? "Pause" "Play")
        (fn [_e] (if playing? (pause-timeline!) (play-timeline!)))
        "toggle-play"]
       [btn "Export CSV" (fn [_e] (download-camera-path!)) "export-camera-path"]
       [btn "Clear" (fn [_e] (clear-timeline!)) "clear-timeline"]]
      [:div {:id "camera-key-list" :class "am-camera-key-list"}
       (for [[index keyframe] (map-indexed vector timeline)]
         ^{:key (str "camera-key-" index)}
         [:div {:class (str "am-camera-key" (when (= index (:selected-keyframe @state)) " selected"))}
          [:button {:on-click #(do (swap! state assoc :selected-keyframe index) (scrub-timeline! (:t keyframe)))}
           (str "Shot " (inc index))]
          [:input {:type "number" :step 0.1 :min 0 :value (:t keyframe)
                   :aria-label (str "Shot " (inc index) " time")
                   :on-change #(move-camera-key! index (js/parseFloat (.. % -target -value)))}]
          [:button {:aria-label (str "Delete shot " (inc index)) :on-click #(delete-camera-key! index)} "×"]])]]
     {:surface :thick :elevation :flat}]))

(defn- debug-state []
  ;; Same idiom as M2/M4's `#out` div: a plain DOM text node a real-browser
  ;; nbb/Playwright script can poll, since WebGPU canvas pixel readback was
  ;; found unreliable in this Chromium build (see test/render/verify_m2_render
  ;; docstring) and there is no other externally-observable signal that a
  ;; preset change actually reached (state) and re-rendered.
  (let [{:keys [weather terrain postfx vegetation interaction-profile camera-mode fly timeline selected-keyframe playhead playing? render-ir webgpu-ctx selected-element building project-revision save-status render-export-status video-export-status video-fps video-bitrate-mbps]} @state]
    [:span {:id "debug-state" :style {:display "none"}}
     (js/JSON.stringify (clj->js {:weather weather :terrain terrain :postfx postfx :vegetation vegetation
                                   :interactionProfile (name interaction-profile)
                                   :cameraMode (name camera-mode) :flyPos (:pos fly)
                                   :elementCount (count (:elements (bim/find-storey building 3)))
                                   :selectedElement selected-element
                                   :projectVersion project/current-version :projectRevision project-revision :saveStatus (name save-status)
                                   :renderExportStatus (name render-export-status)
                                   :videoExportStatus (name video-export-status)
                                   :videoFps video-fps :videoBitrateMbps video-bitrate-mbps
                                   :rendererBackend (name (:backend webgpu-ctx :webgpu))
                                   :keyframeCount (count timeline) :selectedKeyframe selected-keyframe :keyframeTimes (mapv :t timeline) :playhead playhead :playing playing?
                                   :renderEye (get-in render-ir [:globals :eye])}))]))

(defn- root []
  [:div {:class "am-shell"}
   [ui/nav-bar "kami-app-amenominaka" {:trailing nil}]
   [:div {:class "am-body"}
    [:div {:class "am-sidebar"}
     [tools-panel]
     [scene-editor-panel]
     [env-panel]
     [timeline-panel]]
    [viewport]]
   [debug-state]])

(defonce root-node (atom nil))
(defonce keyboard-listeners-installed? (atom false))

(defn init! []
  (let [el (js/document.getElementById "app")]
    (when-not @root-node (reset! root-node (rdomc/create-root el)))
    (rdomc/render @root-node [root])
    (when-not @keyboard-listeners-installed?
      (reset! keyboard-listeners-installed? true)
      (js/window.addEventListener "keydown" on-keydown)
      (js/window.addEventListener "keyup" on-keyup))))
