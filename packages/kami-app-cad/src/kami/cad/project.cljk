(ns kami.cad.project)
(def current-version 5)
(defn- normalize-snap-units [p]
  (update-in p [:project/precision :snap]
             #(if (#{0.1 0.5 1} %) (/ % 1000.0) %)))
(defn document [{:keys [id name sections tessellation selection camera precision interaction feature-model solid view-mode sketch]}]
  {:kami/document :cad-project :kami/version current-version :project/id (or id "untitled-cad")
   :project/name (or name "Untitled CAD") :project/sections sections :project/tessellation tessellation
   :project/selection selection :project/camera camera :project/precision precision :project/interaction interaction
   :project/feature-model feature-model :project/solid solid :project/view-mode (or view-mode :loft)
   :project/sketch (or sketch {:width (or (:sketch-width precision) 4.0) :height 2.0})})
(defn migrate [v]
  (cond
    (= :cad-project (:kami/document v))
    (case (:kami/version v) 5 (normalize-snap-units v)
      4 (assoc (normalize-snap-units v) :kami/version 5
               :project/sketch {:width (or (get-in v [:project/precision :sketch-width]) 4.0) :height 2.0})
      3 (assoc (normalize-snap-units v) :kami/version 5 :project/solid nil :project/view-mode :loft :project/sketch {:width 4.0 :height 2.0})
      2 (assoc (normalize-snap-units v) :kami/version 5 :project/feature-model nil :project/solid nil :project/view-mode :loft :project/sketch {:width 4.0 :height 2.0})
      1 (-> v (assoc :kami/version 5 :project/precision {:snap 0.001 :sketch-width 4.0}
                     :project/sketch {:width 4.0 :height 2.0}
                     :project/interaction {:profile :rhino}) (dissoc :project/version))
      (throw (ex-info "Unsupported CAD project version" {:version (:kami/version v)})))
    (and (vector? v) (seq v) (:cad/control-points (first v)))
    (document {:sections v :tessellation 16 :selection {:section 0 :point 1}
               :camera {:azimuth 0.7 :elevation 0.45} :precision {:snap 0.001 :sketch-width 4.0}
               :interaction {:profile :rhino}})
    :else (throw (ex-info "Not a CAD project" {:value v}))))
(defn valid? [p]
  (and (= :cad-project (:kami/document p)) (= current-version (:kami/version p))
       (string? (:project/id p)) (string? (:project/name p))
       (every? #(and (seq (:cad/control-points %)) (= (count (:cad/control-points %)) (count (:cad/weights %)))) (:project/sections p))
       (pos-int? (:project/tessellation p)) (map? (:project/selection p)) (map? (:project/camera p))
       (map? (:project/precision p)) (map? (:project/interaction p))
       (pos? (get-in p [:project/sketch :width] 0)) (pos? (get-in p [:project/sketch :height] 0))))
(defn open [v] (let [p (migrate v)] (when-not (valid? p) (throw (ex-info "Invalid CAD project" {:project p}))) p))
