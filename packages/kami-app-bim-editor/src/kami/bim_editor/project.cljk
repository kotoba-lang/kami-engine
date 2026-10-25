(ns kami.bim-editor.project)
(def current-version 8)
(defn document [{:keys [id name building-model family-catalog drawings drawing-set print-setting
                        structural-model mep-systems review-topics cloud-state cloud-workspace
                        cloud-checkpoint stream-settings
                        editor camera interaction]}]
  {:kami/document :bim-editor-project :kami/version current-version
   :project/id (or id "untitled-bim") :project/name (or name "Untitled BIM")
   :project/building-model building-model :project/family-catalog (or family-catalog {})
   :project/drawings (or drawings {})
   :project/drawing-set (or drawing-set {})
   :project/print-setting (or print-setting {})
   :project/structural-model structural-model
   :project/mep-systems (vec mep-systems)
   :project/review-topics (vec review-topics)
   :project/cloud-state cloud-state
   :project/cloud-workspace cloud-workspace
   :project/cloud-checkpoint cloud-checkpoint
   :project/stream-settings (or stream-settings {})
   :project/editor editor :project/camera camera :project/interaction interaction})
(defn migrate [v]
  (cond
    (= :bim-editor-project (:kami/document v))
    (case (:kami/version v) 8 v
      7 (assoc v :kami/version 8 :project/cloud-workspace nil :project/cloud-checkpoint nil)
      6 (assoc v :kami/version 8 :project/structural-model nil :project/mep-systems []
               :project/review-topics [] :project/cloud-state nil :project/cloud-workspace nil
               :project/cloud-checkpoint nil :project/stream-settings {})
      5 (assoc v :kami/version 8 :project/drawing-set {} :project/structural-model nil
               :project/mep-systems [] :project/review-topics [] :project/cloud-state nil
               :project/cloud-workspace nil :project/cloud-checkpoint nil
               :project/stream-settings {})
      4 (assoc v :kami/version 8 :project/print-setting {} :project/drawing-set {}
               :project/structural-model nil :project/mep-systems []
               :project/review-topics [] :project/cloud-state nil :project/cloud-workspace nil
               :project/cloud-checkpoint nil :project/stream-settings {})
      3 (assoc v :kami/version 8 :project/drawings {} :project/print-setting {}
               :project/drawing-set {} :project/structural-model nil :project/mep-systems []
               :project/review-topics [] :project/cloud-state nil :project/cloud-workspace nil
               :project/cloud-checkpoint nil :project/stream-settings {})
      2 (-> v (assoc :kami/version 8 :project/family-catalog {} :project/drawings {}
                     :project/drawing-set {}
                     :project/print-setting {} :project/structural-model nil
                     :project/mep-systems [] :project/review-topics []
                     :project/cloud-state nil :project/cloud-workspace nil
                     :project/cloud-checkpoint nil :project/stream-settings {}))
      1 (-> v (assoc :kami/version 8 :project/interaction {:profile :revit}
                     :project/family-catalog {} :project/drawings {}
                     :project/drawing-set {}
                     :project/print-setting {} :project/structural-model nil
                     :project/mep-systems [] :project/review-topics []
                     :project/cloud-state nil :project/cloud-workspace nil
                     :project/cloud-checkpoint nil :project/stream-settings {})
            (dissoc :project/version))
      (throw (ex-info "Unsupported BIM project version" {:version (:kami/version v)})))
    (and (map? v) (vector? (:sites v)))
    (let [storey (some-> v :sites first :buildings first :storeys first)]
      (document {:building-model (dissoc v :structural/model :mep/systems)
                 :structural-model (:structural/model v)
                 :mep-systems (:mep/systems v)
                 :editor {:active-storey (:id storey) :selected (some-> storey :elements first :id)}
                 :camera {:azimuth 0.75 :elevation 0.5} :interaction {:profile :revit}}))
    :else (throw (ex-info "Not a BIM Editor project" {:value v}))))
(defn valid? [p]
  (let [model (:project/building-model p) storeys (mapcat :storeys (mapcat :buildings (:sites model)))]
    (and (= :bim-editor-project (:kami/document p)) (= current-version (:kami/version p))
         (string? (:project/id p)) (string? (:project/name p)) (seq (:sites model)) (seq storeys)
         (every? #(and (some? (:id %)) (vector? (:elements %))) storeys)
         (map? (:project/family-catalog p))
         (map? (:project/drawings p))
         (map? (:project/drawing-set p))
         (map? (:project/print-setting p))
         (vector? (:project/mep-systems p))
         (vector? (:project/review-topics p))
         (or (nil? (:project/cloud-workspace p)) (map? (:project/cloud-workspace p)))
         (or (nil? (:project/cloud-checkpoint p)) (map? (:project/cloud-checkpoint p)))
         (map? (:project/stream-settings p))
         (map? (:project/editor p)) (map? (:project/camera p)) (map? (:project/interaction p)))))
(defn open [v] (let [p (migrate v)] (when-not (valid? p) (throw (ex-info "Invalid BIM Editor project" {:project p}))) p))
