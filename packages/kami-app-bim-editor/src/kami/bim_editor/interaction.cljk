(ns kami.bim-editor.interaction
  "Pure camera-ray picking and framing for the BIM viewport."
  (:require [bim :as bim]))

(def ^:private default-fov-radians (/ #?(:clj Math/PI :cljs js/Math.PI) 3.0))
(def ^:private model-color [0.55 0.7 0.95])
(def ^:private selection-color [1.0 0.58 0.12])
(defn- sqrt [value] (#?(:clj Math/sqrt :cljs js/Math.sqrt) value))
(defn- subtract [left right] (mapv - left right))
(defn- add [left right] (mapv + left right))
(defn- scale [value vector] (mapv #(* value %) vector))
(defn- dot [left right] (reduce + (map * left right)))
(defn- cross [[ax ay az] [bx by bz]]
  [(- (* ay bz) (* az by)) (- (* az bx) (* ax bz)) (- (* ax by) (* ay bx))])
(defn- magnitude [vector] (sqrt (dot vector vector)))
(defn- normalize [vector]
  (let [length (magnitude vector)]
    (when-not (pos? length)
      (throw (ex-info "camera vector must be non-zero" {:vector vector})))
    (scale (/ 1.0 length) vector)))

(defn camera-ray
  "Build a world-space ray from normalized viewport coordinates in [-1, 1].
  Positive screen-y points upward."
  [{:keys [eye target aspect fov-radians]
    :or {aspect 1.0 fov-radians default-fov-radians}}
   screen-x screen-y]
  (when-not (and (= 3 (count eye)) (= 3 (count target))
                 (pos? aspect) (pos? fov-radians))
    (throw (ex-info "invalid viewport camera" {:eye eye :target target :aspect aspect})))
  (let [forward (normalize (subtract target eye))
        world-up (if (> (#?(:clj Math/abs :cljs js/Math.abs)
                           (dot forward [0.0 1.0 0.0])) 0.999)
                   [0.0 0.0 1.0] [0.0 1.0 0.0])
        right (normalize (cross forward world-up))
        up (cross right forward)
        half-height (#?(:clj Math/tan :cljs js/Math.tan) (/ fov-radians 2.0))
        direction (normalize
                   (-> forward
                       (add (scale (* screen-x aspect half-height) right))
                       (add (scale (* screen-y half-height) up))))]
    {:ray/origin (vec eye) :ray/direction direction}))

(defn ray-box-distance
  "Return the nearest non-negative ray distance intersecting an axis-aligned
  box, or nil when the ray misses."
  [{:keys [ray/origin ray/direction]} {:keys [min max]}]
  (loop [axis 0 near 0.0 far ##Inf]
    (if (= axis 3)
      (when (>= far near) near)
      (let [o (nth origin axis) d (nth direction axis)
            low (nth min axis) high (nth max axis)]
        (if (< (#?(:clj Math/abs :cljs js/Math.abs) d) 1.0e-12)
          (when (<= low o high) (recur (inc axis) near far))
          (let [a (/ (- low o) d) b (/ (- high o) d)
                entry (clojure.core/min a b) exit (clojure.core/max a b)
                next-near (clojure.core/max near entry)
                next-far (clojure.core/min far exit)]
            (when (>= next-far next-near)
              (recur (inc axis) next-near next-far))))))))

(defn ray-plane-point
  "Intersect a ray with a plane. Returns nil for parallel or behind-camera hits."
  [ray plane]
  (let [ray-origin (:ray/origin ray) direction (:ray/direction ray)
        plane-origin (:plane/origin plane) normal (:plane/normal plane)]
  (when-not (and (= 3 (count ray-origin)) (= 3 (count direction))
                 (= 3 (count plane-origin)) (= 3 (count normal)))
    (throw (ex-info "ray and plane must use 3D vectors" {})))
  (let [denominator (dot direction normal)]
    (when (> (#?(:clj Math/abs :cljs js/Math.abs) denominator) 1.0e-12)
      (let [distance (/ (dot (subtract plane-origin ray-origin) normal) denominator)]
        (when (not (neg? distance))
          (add ray-origin (scale distance direction))))))))

(defn drag-delta
  "Return the world-space displacement between two ray hits on one plane."
  [start-ray current-ray plane]
  (when-let [start (ray-plane-point start-ray plane)]
    (when-let [current (ray-plane-point current-ray plane)]
      {:drag/start start :drag/current current :drag/delta (subtract current start)})))

(defn commit-drag-state
  "Commit an already-previewed project as one undoable editor revision."
  [state original-project]
  (-> state
      (update :history conj original-project)
      (assoc :future [] :save-status :dirty)
      (update :revision (fnil inc 0))))

(defn cancel-drag-state
  "Restore the project snapshot captured when a drag began."
  [state original-project]
  (assoc state :project original-project :last-snap nil))

(defn element-bounds [element]
  (when-let [mesh (bim/element-mesh element)]
    (when (seq (:positions mesh))
      {:min (apply mapv min (:positions mesh))
       :max (apply mapv max (:positions mesh))})))

(defn pick-element
  "Return the nearest mesh-backed element hit by a ray."
  [elements ray]
  (->> elements
       (keep (fn [element]
               (when-let [bounds (element-bounds element)]
                 (when-let [distance (ray-box-distance ray bounds)]
                   {:element/id (:id element) :element element
                    :pick/distance distance :pick/bounds bounds}))))
       (sort-by (juxt :pick/distance (comp str :element/id)))
       first))

(defn element-render-items
  "Build stable per-element render items so selection can be highlighted in
  the shared depth pass. Elements without renderable geometry are omitted."
  [elements selected-id]
  (let [selected? (if (set? selected-id) #(contains? selected-id %)
                      #(= selected-id %))]
  (->> elements
       (keep (fn [element]
               (when-let [mesh (bim/element-mesh element)]
                 {:element/id (:id element)
                  :mesh mesh
                  :color (if (selected? (:id element))
                           selection-color model-color)})))
       vec)))

(defn selection-after-click
  "Apply replace/add/toggle semantics while retaining a deterministic primary id."
  [selection element-id mode]
  (let [selection (set selection)
        next-selection (case mode
                         :replace (if element-id #{element-id} #{})
                         :add (if element-id (conj selection element-id) selection)
                         :toggle (if element-id
                                   (if (contains? selection element-id)
                                     (disj selection element-id) (conj selection element-id))
                                   selection))
        primary (cond
                  (and element-id (contains? next-selection element-id)) element-id
                  (seq next-selection) (first (sort-by str next-selection))
                  :else nil)]
    {:selection next-selection :primary primary}))

(defn selection-after-box [selection element-ids mode]
  (let [selection (set selection) ids (set element-ids)
        next-selection (case mode
                         :replace ids
                         :add (into selection ids)
                         :toggle (reduce (fn [result id]
                                           (if (contains? result id)
                                             (disj result id) (conj result id)))
                                         selection ids))]
    {:selection next-selection
     :primary (first (sort-by str next-selection))}))

(defn project-point
  "Project a world point into viewport NDC using the same camera convention as camera-ray."
  [{:keys [eye target aspect fov-radians]
    :or {aspect 1.0 fov-radians default-fov-radians}}
   point]
  (let [forward (normalize (subtract target eye))
        world-up (if (> (#?(:clj Math/abs :cljs js/Math.abs)
                           (dot forward [0.0 1.0 0.0])) 0.999)
                   [0.0 0.0 1.0] [0.0 1.0 0.0])
        right (normalize (cross forward world-up)) up (cross right forward)
        delta (subtract point eye) depth (dot delta forward)
        half-height (#?(:clj Math/tan :cljs js/Math.tan) (/ fov-radians 2.0))]
    (when (> depth 1.0e-9)
      [(/ (dot delta right) (* depth aspect half-height))
       (/ (dot delta up) (* depth half-height))])))

(defn- bounds-corners [{:keys [min max]}]
  (for [x [(nth min 0) (nth max 0)]
        y [(nth min 1) (nth max 1)]
        z [(nth min 2) (nth max 2)]] [x y z]))

(defn elements-in-screen-rect
  "Select mesh-backed elements by an NDC rectangle. Window mode requires the
  projected bounds to be contained; crossing mode accepts any overlap."
  [elements camera {:keys [min max]} mode]
  (let [[left bottom] min [right top] max
        overlaps? (fn [[element-left element-bottom] [element-right element-top]]
                    (and (<= element-left right) (<= left element-right)
                         (<= element-bottom top) (<= bottom element-top)))
        contained? (fn [[element-left element-bottom] [element-right element-top]]
                     (and (<= left element-left element-right right)
                          (<= bottom element-bottom element-top top)))]
    (->> elements
         (keep (fn [element]
                 (when-let [bounds (element-bounds element)]
                   (let [points (vec (keep #(project-point camera %) (bounds-corners bounds)))]
                     (when (seq points)
                       (let [screen-bounds {:min (apply mapv clojure.core/min points)
                                            :max (apply mapv clojure.core/max points)}]
                         (when ((if (= :window mode) contained? overlaps?)
                                (:min screen-bounds) (:max screen-bounds))
                           (:id element))))))))
         (sort-by str)
         vec)))

(defn frame-bounds
  "Return an orbit target and distance that fit a box at the requested FOV."
  ([bounds] (frame-bounds bounds default-fov-radians))
  ([{:keys [min max]} fov-radians]
   (let [center (mapv #(/ (+ %1 %2) 2.0) min max)
         diagonal (magnitude (subtract max min))
         radius (clojure.core/max 0.25 (/ diagonal 2.0))
         distance (/ (* radius 1.25)
                     (#?(:clj Math/tan :cljs js/Math.tan) (/ fov-radians 2.0)))]
     {:camera/target center :camera/distance distance})))
