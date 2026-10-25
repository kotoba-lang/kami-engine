(ns kami.modeler.project)

(def current-version 2)
(def default-material {:material/base-color [0.35 0.58 1.0 1.0]
                       :material/metallic 0.0 :material/roughness 0.5})

(defn- normalize-materials [project]
  (update-in project [:project/scene :scene/objects]
             #(mapv (fn [object]
                      (update object :object/material
                              (fn [material] (merge default-material (or material {}))))) %)))

(defn document
  [{:keys [id name scene selection camera interaction]}]
  {:kami/document :modeler-project
   :kami/version current-version
   :project/id (or id "untitled")
   :project/name (or name "Untitled Model")
   :project/scene scene
   :project/selection selection
   :project/camera camera
   :project/interaction interaction})

(defn migrate
  "Normalizes supported project versions. A legacy bare scene is v1."
  [value]
  (cond
    (= :modeler-project (:kami/document value))
    (case (:kami/version value)
      2 (-> value
            (update :project/selection #(merge {:object-id nil :face-id 0 :mode :object} (or % {})))
            (update :project/camera #(merge {:azimuth 0.7 :elevation 0.45} (or % {})))
            (update :project/interaction #(merge {:profile :blender} (or % {})))
            normalize-materials)
      1 (-> value
            (assoc :kami/version 2
                   :project/selection {:object-id (some-> value :project/scene :scene/objects first :object/id)
                                       :face-id 0 :mode :object}
                   :project/camera {:azimuth 0.7 :elevation 0.45}
                   :project/interaction {:profile :blender})
            (dissoc :project/version)
            normalize-materials)
      (throw (ex-info "Unsupported Modeler project version" {:version (:kami/version value)})))

    (:scene/objects value)
    (normalize-materials (document {:scene value
               :selection {:object-id (some-> value :scene/objects first :object/id) :face-id 0 :mode :object}
               :camera {:azimuth 0.7 :elevation 0.45}
               :interaction {:profile :blender}}))

    :else (throw (ex-info "Not a Modeler project or scene" {:value value}))))

(defn valid? [project]
  (let [objects (get-in project [:project/scene :scene/objects])
        ids (map :object/id objects)
        selected-id (get-in project [:project/selection :object-id])]
    (and (= :modeler-project (:kami/document project))
       (= current-version (:kami/version project))
       (string? (:project/id project))
       (not-empty (:project/id project))
       (string? (:project/name project))
       (not-empty (:project/name project))
       (vector? objects)
       (seq objects)
       (every? #(and (integer? (:object/id %))
                     (string? (:object/name %))
                     (map? (:object/mesh %))) objects)
       (= (count ids) (count (distinct ids)))
       (or (nil? selected-id) (some #{selected-id} ids))
       (map? (:project/selection project))
       (map? (:project/camera project))
       (number? (get-in project [:project/camera :azimuth]))
       (number? (get-in project [:project/camera :elevation]))
       (map? (:project/interaction project)))))

(defn open [value]
  (let [project (migrate value)]
    (when-not (valid? project)
      (throw (ex-info "Invalid Modeler project" {:project project})))
    project))
