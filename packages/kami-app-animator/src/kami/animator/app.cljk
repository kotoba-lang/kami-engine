(ns kami.animator.app
  (:require [cljs.reader :as reader] [kotoba.lang.text :as string] [kami.animation :as animation]
            [kami.animator.project :as project] [kami.webgpu.mesh :as gpu-mesh]))

(def cube-geo
  {:positions [[-1 -1 -1] [1 -1 -1] [1 1 -1] [-1 1 -1] [-1 -1 1] [1 -1 1] [1 1 1] [-1 1 1]
               [-1 -1 -1] [1 -1 -1] [1 -1 1] [-1 -1 1] [-1 1 -1] [-1 1 1] [1 1 1] [1 1 -1]
               [-1 -1 -1] [-1 -1 1] [-1 1 1] [-1 1 -1] [1 -1 -1] [1 1 -1] [1 1 1] [1 -1 1]]
   :normals (vec (mapcat #(repeat 4 %) [[0 0 -1] [0 -1 0] [0 1 0] [-1 0 0] [1 0 0] [0 0 1]]))
   :joints (mapv (fn [[_ y _]] (if (pos? y) [2 0 0 0] [0 0 0 0]))
                 [[-1 -1 -1] [1 -1 -1] [1 1 -1] [-1 1 -1] [-1 -1 1] [1 -1 1] [1 1 1] [-1 1 1]
                  [-1 -1 -1] [1 -1 -1] [1 -1 1] [-1 -1 1] [-1 1 -1] [-1 1 1] [1 1 1] [1 1 -1]
                  [-1 -1 -1] [-1 -1 1] [-1 1 1] [-1 1 -1] [1 -1 -1] [1 1 -1] [1 1 1] [1 -1 1]])
   :weights (vec (repeat 24 [1 0 0 0]))
   :indices (vec (mapcat (fn [i] [i (+ i 1) (+ i 2) i (+ i 2) (+ i 3)]) (range 0 24 4)))})
(def channel-defs
  [{:target :cube/x :label "Location X" :default 0} {:target :cube/y :label "Location Y" :default 0} {:target :cube/z :label "Location Z" :default 0}
   {:target :cube/rotation-x :label "Rotation X" :default 0} {:target :cube/rotation-y :label "Rotation Y" :default 0} {:target :cube/rotation-z :label "Rotation Z" :default 0}
   {:target :cube/scale-x :label "Scale X" :default 1} {:target :cube/scale-y :label "Scale Y" :default 1} {:target :cube/scale-z :label "Scale Z" :default 1}])
(def channels (mapv :target channel-defs))
(def initial
  (animation/timeline 2
    (mapv (fn [{:keys [target default]}]
            (animation/track target
              (case target
                :cube/x [(animation/keyframe 0 -2) (animation/keyframe 2 2 :smooth)]
                :cube/y [(animation/keyframe 0 0) (animation/keyframe 1 1 :smooth) (animation/keyframe 2 0 :smooth)]
                :cube/rotation-z [(animation/keyframe 0 0) (animation/keyframe 2 (* 2 js/Math.PI) :smooth)]
                [(animation/keyframe 0 default) (animation/keyframe 2 default)]))) channel-defs)))
(defonce state (atom {:timeline initial :time 0 :playing? false :active-target :cube/x :selected nil
                      :selected-keys #{}
                      :profile :maya :history [initial] :future []
                      :project-id "untitled-animation" :project-name "Untitled Animation"
                      :fps 24 :frame-snap? true :clipboard nil :revision 0 :save-status :clean
                      :rig project/default-rig :pose project/default-pose :selected-bone :root
                      :pose-history [project/default-pose] :pose-future [] :constraints [] :ik project/default-ik}))
(defonce viewport (atom nil))
(declare update-ui! set-time! commit! snap-time)

(defn- frames
  ([] (frames (:active-target @state)))
  ([target] (:track/keyframes (first (filter #(= target (:track/target %)) (:timeline/tracks (:timeline @state)))))))
(defn- active-track [] (first (filter #(= (:active-target @state) (:track/target %)) (:timeline/tracks (:timeline @state)))))
(defn- select-key! [event keyframe]
  (let [id (:keyframe/id keyframe) additive (or (.-shiftKey event) (.-ctrlKey event) (.-metaKey event))]
    (swap! state (fn [s]
                   (if additive
                     (let [selected (if ((:selected-keys s) id) (disj (:selected-keys s) id) (conj (:selected-keys s) id))]
                       (assoc s :selected-keys selected :selected (when (selected id) id)))
                     (assoc s :selected-keys #{id} :selected id))))
  (update-ui!)))
(defn- render-graph! []
  (let [svg (.getElementById js/document "curve-graph") track (active-track) duration (:timeline/duration (:timeline @state))
        sample-count 65 times (mapv #(* duration (/ % (dec sample-count))) (range sample-count))
        values (mapv #(animation/sample track %) times) key-values (mapv :keyframe/value (:track/keyframes track))
        min-value (reduce min (concat values key-values)) max-value (reduce max (concat values key-values))
        span (max 1.0e-6 (- max-value min-value)) padding (* span 0.1) low (- min-value padding) high (+ max-value padding)
        x #(-> % (/ duration) (* 800)) y #(-> (- high %) (/ (- high low)) (* 160))
        svg-ns "http://www.w3.org/2000/svg"]
    (set! (.-innerHTML svg) "")
    (doseq [gy [40 80 120]]
      (let [line (.createElementNS js/document svg-ns "line")]
        (doseq [[attr value] [["x1" 0] ["x2" 800] ["y1" gy] ["y2" gy] ["class" "graph-grid"]]] (.setAttribute line attr value))
        (.appendChild svg line)))
    (let [path (.createElementNS js/document svg-ns "polyline")]
      (.setAttribute path "class" "curve-path")
      (.setAttribute path "points" (string/join " " (map (fn [t value] (str (x t) "," (y value))) times values)))
      (.appendChild svg path))
    (doseq [k (:track/keyframes track)]
      (let [circle (.createElementNS js/document svg-ns "circle")]
        (doseq [[attr value] [["cx" (x (:keyframe/time k))] ["cy" (y (:keyframe/value k))] ["r" 6]
                              ["class" (str "graph-key" (when ((:selected-keys @state) (:keyframe/id k)) " selected"))]]]
          (.setAttribute circle attr value))
        (.addEventListener circle "click" #(do (swap! state assoc :time (:keyframe/time k)) (select-key! % k)))
        (.appendChild svg circle)))))
(defn- render-keys! []
  (let [lane (.getElementById js/document "lane")]
    (doseq [n (array-seq (.querySelectorAll lane ".key"))] (.remove n))
    (doseq [k (frames)]
      (let [b (.createElement js/document "button")]
        (set! (.-className b) (str "key" (when ((:selected-keys @state) (:keyframe/id k)) " selected")))
        (.setAttribute b "aria-label" (str "Keyframe at " (:keyframe/time k) " seconds, value " (:keyframe/value k)))
        (set! (.. b -style -left) (str (* 100 (/ (:keyframe/time k) (:timeline/duration (:timeline @state)))) "%"))
        (.addEventListener b "click" #(select-key! % k))
        (.appendChild lane b)))))
(defn- update-ui! []
  (let [{:keys [time timeline selected selected-keys history future active-target profile revision save-status fps frame-snap? clipboard]} @state
        k (first (filter #(= selected (:keyframe/id %)) (frames)))]
    (set! (.-value (.getElementById js/document "scrub")) time)
    (set! (.. (.getElementById js/document "playhead") -style -left) (str (* 100 (/ time (:timeline/duration timeline))) "%"))
    (set! (.-max (.getElementById js/document "scrub")) (:timeline/duration timeline))
    (set! (.-step (.getElementById js/document "scrub")) (/ 1 fps))
    (set! (.-textContent (.getElementById js/document "time"))
          (str "Frame " (js/Math.round (* time fps)) " · " (.toFixed time 3) " / " (.toFixed (:timeline/duration timeline) 3) " s"))
    (set! (.-disabled (.getElementById js/document "undo")) (and (= 1 (count history)) (= 1 (count (:pose-history @state)))))
    (set! (.-disabled (.getElementById js/document "redo")) (and (empty? future) (empty? (:pose-future @state))))
    (set! (.-textContent (.getElementById js/document "play")) (if (:playing? @state) "❚❚ Pause" "▶ Play"))
    (set! (.-textContent (.getElementById js/document "profile-hint"))
          (case profile :blender "I Insert Key · Space Play · X Delete"
                :max "K Set Key · / Play · Delete Key"
                :c4d "F9 Record · F8 Play · Delete Key"
                "S Set Key · Alt+V Play · Delete Key"))
    (doseq [target channels]
      (.toggle (.-classList (.getElementById js/document (str "channel-" (name target))))
               "primary" (= target active-target)))
    (doseq [bone [:root :spine :head]]
      (.toggle (.-classList (.getElementById js/document (str "bone-" (name bone))))
               "primary" (= bone (:selected-bone @state))))
    (let [bone-id (:selected-bone @state)
          bone (first (filter #(= bone-id (:bone/id %)) (get-in @state [:rig :skeleton/bones])))
          trs (merge (:bone/rest bone) (get-in @state [:pose :pose/bones bone-id]))]
      (set! (.-textContent (.getElementById js/document "pose-bone-name")) (:bone/name bone))
      (doseq [[prefix values] [["pose-t" (:translation trs)] ["pose-r" (:rotation trs)]]
              [axis value] (map vector ["x" "y" "z"] values)]
        (set! (.-value (.getElementById js/document (str prefix axis))) value)))
    (let [container (.getElementById js/document "constraint-stack")]
      (set! (.-innerHTML container) "")
      (doseq [constraint (:constraints @state)]
        (let [row (.createElement js/document "div") toggle (.createElement js/document "button") remove (.createElement js/document "button")]
          (set! (.-textContent toggle) (str (if (:constraint/enabled? constraint) "◉ " "○ ")
                                             (name (:constraint/kind constraint)) " · " (name (:constraint/bone constraint))))
          (.addEventListener toggle "click" #(do (swap! state update :constraints
                                                         (fn [items] (mapv (fn [item] (if (= (:constraint/id item) (:constraint/id constraint))
                                                                                       (update item :constraint/enabled? not) item)) items)))
                                                   (swap! state assoc :save-status :dirty) (update-ui!)))
          (set! (.-textContent remove) "×") (set! (.-title remove) "Delete constraint")
          (.addEventListener remove "click" #(do (swap! state update :constraints
                                                         (fn [items] (vec (remove (fn [item] (= (:constraint/id item) (:constraint/id constraint))) items))))
                                                   (swap! state assoc :save-status :dirty) (update-ui!)))
          (.appendChild row toggle) (.appendChild row remove) (.appendChild container row))))
    (let [[x y] (get-in @state [:ik :ik/target])]
      (set! (.-value (.getElementById js/document "ik-target-x")) x)
      (set! (.-value (.getElementById js/document "ik-target-y")) y)
      (set! (.-value (.getElementById js/document "ik-elbow")) (name (get-in @state [:ik :ik/elbow])))
      (let [solution (animation/solve-two-bone-ik {:root [0 0] :length-a 1 :length-b 1
                                                    :target [y (- x)] :elbow (get-in @state [:ik :ik/elbow])})]
        (set! (.-textContent (.getElementById js/document "ik-status"))
              (if (:ik/clamped? solution) "Target outside reach · clamped" "Target reached"))))
    (set! (.-textContent (.getElementById js/document "active-channel"))
          (:label (first (filter #(= active-target (:target %)) channel-defs))))
    (when k
      (set! (.-value (.getElementById js/document "key-time")) (:keyframe/time k))
      (set! (.-value (.getElementById js/document "key-value")) (:keyframe/value k))
      (set! (.-value (.getElementById js/document "interpolation")) (name (:keyframe/interpolation k)))
      (set! (.-value (.getElementById js/document "tangent-in")) (:keyframe/tangent-in k 0))
      (set! (.-value (.getElementById js/document "tangent-out")) (:keyframe/tangent-out k 0)))
    (set! (.-checked (.getElementById js/document "loop")) (:timeline/loop? timeline))
    (set! (.-value (.getElementById js/document "loop-start")) (:timeline/loop-start timeline 0))
    (set! (.-value (.getElementById js/document "loop-end")) (:timeline/loop-end timeline (:timeline/duration timeline)))
    (set! (.-value (.getElementById js/document "playback-rate")) (:timeline/playback-rate timeline 1))
    (set! (.-value (.getElementById js/document "fps")) fps)
    (set! (.-checked (.getElementById js/document "frame-snap")) frame-snap?)
    (set! (.-value (.getElementById js/document "duration")) (:timeline/duration timeline))
    (set! (.-textContent (.getElementById js/document "debug-state"))
          (js/JSON.stringify (clj->js {:time time :keyCount (count (frames)) :trackCount (count channels)
                                       :activeTarget (str active-target)
                                       :profile (name profile) :playing (:playing? @state)
                                       :projectVersion project/current-version :revision revision :saveStatus (name save-status)
                                       :fps fps :frame (js/Math.round (* time fps)) :frameSnap frame-snap?
                                       :clipboard (boolean clipboard) :graphSamples 65 :selectedCount (count selected-keys)
                                       :selected (some-> selected str) :selectedBone (some-> (:selected-bone @state) name)
                                       :boneCount (count (get-in @state [:rig :skeleton/bones]))
                                       :poseMatrices (count (animation/bone-world-matrices (:rig @state) (:pose @state)))
                                       :boneTrackCount (count (filter #(and (vector? (:track/target %)) (= :bone (first (:track/target %)))) (:timeline/tracks timeline)))
                                       :constraintCount (count (:constraints @state))
                                       :translation (mapv #(get (animation/evaluate timeline time) % 0) (take 3 channels))
                                       :rotation (mapv #(get (animation/evaluate timeline time) % 0) (take 3 (drop 3 channels)))
                                       :scale (mapv #(get (animation/evaluate timeline time) % 1) (drop 6 channels))})))
    (render-keys!) (render-graph!)))
(defn- commit! [tl] (swap! state #(-> % (assoc :timeline tl :future [] :save-status :dirty)
                                        (update :history conj tl) (update :revision inc))) (update-ui!))
(defn- commit-pose! [pose]
  (swap! state #(-> % (assoc :pose pose :pose-future [] :save-status :dirty)
                           (update :pose-history conj pose) (update :revision inc)))
  (update-ui!))
(defn- key-selected-bone-pose! []
  (let [bone-id (:selected-bone @state)
        bone (first (filter #(= bone-id (:bone/id %)) (get-in @state [:rig :skeleton/bones])))
        trs (merge (:bone/rest bone) (get-in @state [:pose :pose/bones bone-id]))
        time (snap-time (:time @state))
        next-timeline
        (reduce (fn [tl [channel values]]
                  (reduce (fn [tl [axis value]]
                            (animation/add-keyframe tl (animation/bone-track-target bone-id channel axis)
                                                    (animation/keyframe time value)))
                          tl (map vector [:x :y :z] values)))
                (:timeline @state) (select-keys trs [:translation :rotation :scale]))]
    (commit! next-timeline)))
(defn- snap-time [time]
  (if (:frame-snap? @state) (/ (js/Math.round (* time (:fps @state))) (:fps @state)) time))
(defn- add-key! []
  (let [target (:active-target @state) time (snap-time (:time @state))
        value (get (animation/evaluate (:timeline @state) time) target 0)
        existing (first (filter #(< (js/Math.abs (- (:keyframe/time %) time)) 1.0e-6) (frames)))]
    (if existing
      (do (commit! (animation/update-keyframe (:timeline @state) target (:keyframe/id existing) assoc :keyframe/value value))
          (swap! state assoc :selected (:keyframe/id existing) :selected-keys #{(:keyframe/id existing)}) (update-ui!))
      (let [k (animation/keyframe time value)]
        (commit! (animation/add-keyframe (:timeline @state) target k))
        (swap! state assoc :selected (:keyframe/id k) :selected-keys #{(:keyframe/id k)}) (update-ui!)))))
(defn- delete-key! []
  (let [ids (if (seq (:selected-keys @state)) (:selected-keys @state) (some-> (:selected @state) hash-set))]
    (when (and (seq ids) (< (count ids) (count (frames))))
      (commit! (reduce #(animation/delete-keyframe %1 (:active-target @state) %2) (:timeline @state) ids))
      (swap! state assoc :selected nil :selected-keys #{}) (update-ui!))))
(defn- nudge-selected! [frames-delta]
  (let [ids (:selected-keys @state) delta (/ frames-delta (:fps @state)) target (:active-target @state)
        candidates (mapv #(if (ids (:keyframe/id %)) (update % :keyframe/time + delta) %) (:track/keyframes (active-track)))
        times (mapv :keyframe/time candidates)
        valid? (and (seq ids) (every? #(<= 0 % (:timeline/duration (:timeline @state))) times)
                    (= (count times) (count (distinct times))))]
    (when valid?
      (let [tracks (mapv #(if (= target (:track/target %)) (animation/track target candidates) %) (:timeline/tracks (:timeline @state)))]
        (commit! (assoc (:timeline @state) :timeline/tracks tracks))
        (when-let [primary (first (filter #(= (:selected @state) (:keyframe/id %)) candidates))]
          (swap! state assoc :time (:keyframe/time primary)))
        (update-ui!)))))
(defn- selected-key [] (first (filter #(= (:selected @state) (:keyframe/id %)) (frames))))
(defn- copy-key! []
  (when-let [k (selected-key)]
    (swap! state assoc :clipboard (select-keys k [:keyframe/value :keyframe/interpolation
                                                  :keyframe/tangent-in :keyframe/tangent-out :keyframe/broken?]))
    (update-ui!)))
(defn- paste-key! []
  (when-let [data (:clipboard @state)]
    (let [target (:active-target @state) time (snap-time (:time @state))
          existing (first (filter #(< (js/Math.abs (- (:keyframe/time %) time)) 1.0e-7) (frames)))
          frame (merge (animation/keyframe time (:keyframe/value data) (:keyframe/interpolation data)) data)]
      (if existing
        (do (commit! (animation/update-keyframe (:timeline @state) target (:keyframe/id existing) merge data))
            (swap! state assoc :selected (:keyframe/id existing) :selected-keys #{(:keyframe/id existing)}))
        (do (commit! (animation/add-keyframe (:timeline @state) target frame))
            (swap! state assoc :selected (:keyframe/id frame) :selected-keys #{(:keyframe/id frame)})))
      (update-ui!))))
(defn- cut-key! [] (when (selected-key) (copy-key!) (delete-key!)))
(defn- duplicate-key! []
  (when (selected-key)
    (copy-key!)
    (set-time! (min (:timeline/duration (:timeline @state)) (+ (:time @state) (/ 1 (:fps @state)))))
    (paste-key!)))
(defn- undo! []
  (if (> (count (:pose-history @state)) 1)
    (swap! state (fn [s] (let [h (:pose-history s) cur (peek h) h2 (pop h)]
                           (assoc s :pose-history h2 :pose (peek h2) :pose-future (conj (:pose-future s) cur)))))
   (when (> (count (:history @state)) 1)
    (swap! state (fn [s] (let [h (:history s) cur (peek h) h2 (pop h)]
                           (assoc s :history h2 :timeline (peek h2) :future (conj (:future s) cur)))))
    )) (update-ui!))
(defn- redo! []
  (if-let [pose (peek (:pose-future @state))]
    (swap! state (fn [s] (assoc s :pose pose :pose-history (conj (:pose-history s) pose) :pose-future (pop (:pose-future s)))))
   (when-let [tl (peek (:future @state))]
    (swap! state (fn [s] (assoc s :timeline tl :history (conj (:history s) tl) :future (pop (:future s)))))
    )) (update-ui!))
(declare tick!)
(defn- toggle-play! []
  (swap! state update :playing? not)
  (when (:playing? @state) (tick! (js/performance.now))) (update-ui!))
(defn- set-time! [time]
  (swap! state assoc :time (max 0 (min (:timeline/duration (:timeline @state)) time)) :playing? false) (update-ui!))
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
      (and ctrl (= key "c")) :copy-key
      (and ctrl (= key "x")) :cut-key
      (and ctrl (= key "v")) :paste-key
      (and (.-shiftKey event) (= key "d")) :duplicate-key
      (and alt (= key "arrowright")) :nudge-right (and alt (= key "arrowleft")) :nudge-left
      (= key "arrowright") :next-frame (= key "arrowleft") :previous-frame
      (= key "home") :start (= key "end") :end (= key "delete") :delete-key
      (= profile :maya) (cond (= key "s") :add-key (and alt (= key "v")) :play)
      (= profile :blender) ({"i" :add-key "x" :delete-key " " :play} key)
      (= profile :max) ({"k" :add-key "/" :play} key)
      (= profile :c4d) ({"f9" :add-key "f8" :play} key))))
(defn- execute-command! [command]
  (case command :add-key (add-key!) :delete-key (delete-key!) :play (toggle-play!) :undo (undo!) :redo (redo!)
        :copy-key (copy-key!) :cut-key (cut-key!) :paste-key (paste-key!) :duplicate-key (duplicate-key!)
        :nudge-left (nudge-selected! -1) :nudge-right (nudge-selected! 1)
        :next-frame (set-time! (+ (:time @state) (/ 1 (:fps @state)))) :previous-frame (set-time! (- (:time @state) (/ 1 (:fps @state))))
        :start (set-time! 0) :end (set-time! (:timeline/duration (:timeline @state))) nil))

(def ^:private storage-key "kami.animator.project.v2")
(def ^:private backup-key "kami.animator.project.backup")
(defn- project-document []
  (let [{:keys [project-id project-name timeline time active-target selected selected-keys profile fps frame-snap? rig pose selected-bone constraints]} @state]
    (project/document {:id project-id :name project-name :timeline timeline
                       :rig rig :pose pose :constraints constraints :ik (:ik @state)
                       :editor {:time time :active-target active-target :selected selected :selected-keys (vec selected-keys) :profile profile
                                :fps fps :frame-snap? frame-snap? :selected-bone selected-bone}})))
(defn- save-project! []
  (let [data (pr-str (project-document)) previous (.getItem js/localStorage storage-key)]
    (when previous (.setItem js/localStorage backup-key previous))
    (.setItem js/localStorage storage-key data) (swap! state assoc :save-status :saved) (update-ui!)))
(defn- apply-project! [value]
  (let [p (project/open value) tl (:project/timeline p) editor (:project/editor p)
        target (or (:active-target editor) (:track/target (first (:timeline/tracks tl))))]
    (swap! state assoc :project-id (:project/id p) :project-name (:project/name p)
           :timeline tl :time (min (:time editor 0) (:timeline/duration tl))
           :active-target target :selected (:selected editor) :profile (:profile editor :maya)
           :selected-keys (set (:selected-keys editor (if-let [id (:selected editor)] [id] [])))
           :fps (:fps editor 24) :frame-snap? (:frame-snap? editor true)
           :rig (:project/rig p) :pose (:project/pose p) :selected-bone (:selected-bone editor :root)
           :constraints (:project/constraints p) :ik (:project/ik p)
           :pose-history [(:project/pose p)] :pose-future []
           :playing? false :history [tl] :future [] :save-status :saved)
    (set! (.-value (.getElementById js/document "profile")) (name (:profile editor :maya)))
    (update-ui!)))
(defn- load-project! []
  (when-let [data (.getItem js/localStorage storage-key)]
    (try (apply-project! (reader/read-string data))
         (catch :default _ (when-let [backup (.getItem js/localStorage backup-key)]
                             (apply-project! (reader/read-string backup)))))))
(defn- download-project! []
  (let [a (.createElement js/document "a") url (.createObjectURL js/URL
        (js/Blob. #js [(pr-str (project-document))] #js {:type "application/edn"}))]
    (set! (.-href a) url) (set! (.-download a) "animation.kami-animator.edn") (.click a)
    (js/setTimeout #(.revokeObjectURL js/URL url) 0)))
(defn- import-project! [event]
  (when-let [file (aget (.. event -target -files) 0)]
    (-> (.text file) (.then #(apply-project! (reader/read-string %))))))
(defn- draw! []
  (when-let [{:keys [buffers] :as v} @viewport]
    (let [timeline (:timeline @state) time (:time @state) values (animation/evaluate timeline time)
          animated (animation/evaluate-skeleton-pose (:rig @state) timeline time)
          combined {:pose/bones (merge-with merge (:pose/bones (:pose @state)) (:pose/bones animated))}
          constrained (animation/apply-pose-constraints (:rig @state) combined (:constraints @state))
          palette (animation/bone-skinning-matrices (:rig @state) constrained)]
      (gpu-mesh/render-skinned-frame! v buffers [0 2.8 6] [0 0 0] [0.45 0.65 1.0] palette
                              {:translation (mapv #(get values % 0) (take 3 channels))
                               :rotation (mapv #(get values % 0) (take 3 (drop 3 channels)))
                               :scale (mapv #(get values % 1) (drop 6 channels))})))
  (js/requestAnimationFrame draw!))
(defn- tick! [last-ms]
  (when (:playing? @state)
    (let [now (js/performance.now) timeline (:timeline @state)
          dt (* (/ (- now last-ms) 1000) (:timeline/playback-rate timeline 1)) t (+ (:time @state) dt)
          end (if (:timeline/loop? timeline) (:timeline/loop-end timeline) (:timeline/duration timeline))
          next-time (if (> t end) (if (:timeline/loop? timeline) (:timeline/loop-start timeline) end) t)]
      (swap! state assoc :time next-time :playing? (or (:timeline/loop? timeline) (< next-time end)))
      (update-ui!) (when (:playing? @state) (js/requestAnimationFrame #(tick! now))))))
(defn ^:export init! []
  (let [canvas (.getElementById js/document "gpu-canvas")]
    (-> (gpu-mesh/init-canvas! canvas) (.then (fn [v] (let [b (gpu-mesh/upload-mesh! (:mesh-context v) cube-geo)] (reset! viewport (assoc v :buffers b)) (set! (.-textContent (.getElementById js/document "gpu-status")) "") (draw!)))))
    (.addEventListener (.getElementById js/document "play") "click" toggle-play!)
    (.addEventListener (.getElementById js/document "scrub") "input" #(do (swap! state assoc :time (js/parseFloat (.. % -target -value)) :playing? false) (update-ui!)))
    (.addEventListener (.getElementById js/document "fps") "change"
                       #(do (swap! state assoc :fps (js/parseInt (.. % -target -value)) :save-status :dirty) (update-ui!)))
    (.addEventListener (.getElementById js/document "frame-snap") "change"
                       #(do (swap! state assoc :frame-snap? (.. % -target -checked) :save-status :dirty) (update-ui!)))
    (.addEventListener (.getElementById js/document "duration") "change"
                       #(let [requested (max 0.1 (js/parseFloat (.. % -target -value)))
                              latest (reduce max 0 (map :keyframe/time (mapcat :track/keyframes (:timeline/tracks (:timeline @state)))))
                              duration (max requested latest)]
                          (commit! (-> (:timeline @state) (assoc :timeline/duration duration)
                                       (update :timeline/loop-end min duration)))
                          (set! (.-value (.-target %)) duration)))
    (doseq [target channels]
      (.addEventListener (.getElementById js/document (str "channel-" (name target))) "click"
                         #(do (swap! state assoc :active-target target :selected nil :selected-keys #{}) (update-ui!))))
    (doseq [bone [:root :spine :head]]
      (.addEventListener (.getElementById js/document (str "bone-" (name bone))) "click"
                         #(do (swap! state assoc :selected-bone bone :save-status :dirty) (update-ui!))))
    (doseq [[prefix key] [["pose-t" :translation] ["pose-r" :rotation]]
            [axis index] (map vector ["x" "y" "z"] (range 3))]
      (.addEventListener (.getElementById js/document (str prefix axis)) "change"
                         #(let [bone-id (:selected-bone @state)
                                bone (first (filter (fn [b] (= bone-id (:bone/id b))) (get-in @state [:rig :skeleton/bones])))
                                current (merge (:bone/rest bone) (get-in @state [:pose :pose/bones bone-id]))
                                next-trs (update current key assoc index (js/parseFloat (.. % -target -value)))]
                            (commit-pose! (assoc-in (:pose @state) [:pose/bones bone-id] next-trs)))))
    (.addEventListener (.getElementById js/document "key-bone-pose") "click" key-selected-bone-pose!)
    (.addEventListener (.getElementById js/document "solve-ik") "click"
      #(let [x (js/parseFloat (.-value (.getElementById js/document "ik-target-x")))
             y (js/parseFloat (.-value (.getElementById js/document "ik-target-y")))
             elbow (keyword (.-value (.getElementById js/document "ik-elbow")))
             solution (animation/solve-two-bone-ik {:root [0 0] :length-a 1 :length-b 1
                                                     :target [y (- x)] :elbow elbow})
             root-z (- (:ik/root-rotation solution))
             mid-z (- (:ik/mid-rotation solution))
             pose (-> (:pose @state)
                      (assoc-in [:pose/bones :root :rotation] [0 0 root-z])
                      (assoc-in [:pose/bones :spine :rotation] [0 0 mid-z]))]
         (swap! state assoc :ik {:ik/target [x y] :ik/elbow elbow})
         (commit-pose! pose)))
    (.addEventListener (.getElementById js/document "add-copy-constraint") "click"
                       #(let [bone (:selected-bone @state) target (keyword (.-value (.getElementById js/document "constraint-target")))]
                          (when (not= bone target)
                            (swap! state update :constraints conj (animation/pose-constraint (random-uuid) :copy-translation bone {:target target :influence 1.0}))
                            (swap! state assoc :save-status :dirty) (update-ui!))))
    (.addEventListener (.getElementById js/document "add-limit-constraint") "click"
                       #(do (swap! state update :constraints conj
                                   (animation/pose-constraint (random-uuid) :limit-rotation (:selected-bone @state)
                                                              {:min [-1 -1 -1] :max [1 1 1]}))
                            (swap! state assoc :save-status :dirty) (update-ui!)))
    (.addEventListener (.getElementById js/document "add-key") "click" add-key!)
    (.addEventListener (.getElementById js/document "delete-key") "click" delete-key!)
    (.addEventListener (.getElementById js/document "copy-key") "click" copy-key!)
    (.addEventListener (.getElementById js/document "paste-key") "click" paste-key!)
    (.addEventListener (.getElementById js/document "duplicate-key") "click" duplicate-key!)
    (.addEventListener (.getElementById js/document "nudge-left") "click" #(nudge-selected! -1))
    (.addEventListener (.getElementById js/document "nudge-right") "click" #(nudge-selected! 1))
    (.addEventListener (.getElementById js/document "profile") "change" #(do (swap! state assoc :profile (keyword (.. % -target -value))) (update-ui!)))
    (.addEventListener js/window "keydown" #(when-not (editable-target? %) (when-let [command (command-for-event %)] (.preventDefault %) (execute-command! command))))
    (.addEventListener (.getElementById js/document "key-time") "change"
                       #(when-let [id (:selected @state)]
                          (commit! (animation/move-keyframe (:timeline @state) (:active-target @state) id
                                                            (snap-time (js/parseFloat (.. % -target -value)))))))
    (.addEventListener (.getElementById js/document "key-value") "change"
                       #(when-let [id (:selected @state)]
                          (commit! (animation/update-keyframe (:timeline @state) (:active-target @state) id assoc
                                                              :keyframe/value (js/parseFloat (.. % -target -value))))))
    (.addEventListener (.getElementById js/document "interpolation") "change"
                       #(when-let [id (:selected @state)]
                          (commit! (animation/update-keyframe (:timeline @state) (:active-target @state) id assoc
                                                              :keyframe/interpolation (keyword (.. % -target -value))))))
    (doseq [[id field] [["tangent-in" :keyframe/tangent-in] ["tangent-out" :keyframe/tangent-out]]]
      (.addEventListener (.getElementById js/document id) "change"
                         #(when-let [frame-id (:selected @state)]
                            (commit! (animation/update-keyframe (:timeline @state) (:active-target @state) frame-id assoc
                                                                field (js/parseFloat (.. % -target -value)))))))
    (.addEventListener (.getElementById js/document "auto-tangents") "click"
                       #(let [target (:active-target @state)
                              timeline (:timeline @state)
                              tracks (mapv (fn [track] (if (= target (:track/target track)) (animation/auto-tangents track) track)) (:timeline/tracks timeline))]
                          (commit! (assoc timeline :timeline/tracks tracks))))
    (.addEventListener (.getElementById js/document "loop") "change"
                       #(commit! (assoc (:timeline @state) :timeline/loop? (.. % -target -checked))))
    (doseq [[id field] [["loop-start" :timeline/loop-start] ["loop-end" :timeline/loop-end] ["playback-rate" :timeline/playback-rate]]]
      (.addEventListener (.getElementById js/document id) "change"
                         #(let [timeline (:timeline @state) raw (js/parseFloat (.. % -target -value))
                                value (case field
                                        :timeline/loop-start (max 0 (min raw (- (:timeline/loop-end timeline) 0.01)))
                                        :timeline/loop-end (min (:timeline/duration timeline) (max raw (+ (:timeline/loop-start timeline) 0.01)))
                                        :timeline/playback-rate (max 0.1 (min 4 raw)))]
                            (commit! (assoc timeline field value)))))
    (.addEventListener (.getElementById js/document "undo") "click" undo!)
    (.addEventListener (.getElementById js/document "redo") "click" redo!)
    (.addEventListener (.getElementById js/document "save-project") "click" save-project!)
    (.addEventListener (.getElementById js/document "load-project") "click" load-project!)
    (.addEventListener (.getElementById js/document "import") "click" #(.click (.getElementById js/document "import-file")))
    (.addEventListener (.getElementById js/document "import-file") "change" import-project!)
    (.addEventListener (.getElementById js/document "export") "click" download-project!)
    (update-ui!)))
