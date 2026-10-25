(ns kami.sculpt.project)
(def current-version 2)
(defn document [{:keys [id name sculpt-document brush symmetry interaction strokes]}]
  {:kami/document :sculpt-project :kami/version current-version :project/id (or id "untitled-sculpt")
   :project/name (or name "Untitled Sculpt") :project/sculpt sculpt-document :project/brush brush
   :project/symmetry symmetry :project/interaction interaction :project/strokes (or strokes 0)})
(defn migrate [v]
  (cond
    (= :sculpt-project (:kami/document v))
    (case (:kami/version v) 2 v
      1 (-> v (assoc :kami/version 2 :project/interaction {:profile :zbrush} :project/strokes 0) (dissoc :project/version))
      (throw (ex-info "Unsupported Sculpt project version" {:version (:kami/version v)})))
    (and (map? v) (:sculpt/base v) (:sculpt/layers v))
    (document {:sculpt-document v :brush {:mode :inflate :radius 0.6 :strength 0.12 :spacing 0.12}
               :symmetry [:x] :interaction {:profile :zbrush}})
    :else (throw (ex-info "Not a Sculpt project" {:value v}))))
(defn valid? [p]
  (let [doc (:project/sculpt p) base (:sculpt/base doc)]
    (and (= :sculpt-project (:kami/document p)) (= current-version (:kami/version p))
         (string? (:project/id p)) (string? (:project/name p)) (seq (:positions base)) (seq (:indices base))
         (seq (:sculpt/layers doc)) (some? (:sculpt/active-layer doc)) (map? (:project/brush p))
         (vector? (:project/symmetry p)) (map? (:project/interaction p)) (nat-int? (:project/strokes p)))))
(defn open [v] (let [p (migrate v)] (when-not (valid? p) (throw (ex-info "Invalid Sculpt project" {:project p}))) p))
