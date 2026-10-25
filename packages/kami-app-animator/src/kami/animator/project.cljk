(ns kami.animator.project)

(def current-version 5)
(def default-rig
  {:skeleton/bones [{:bone/id :root :bone/name "Root" :bone/parent nil
                     :bone/rest {:translation [0 0 0] :rotation [0 0 0] :scale [1 1 1]}}
                    {:bone/id :spine :bone/name "Spine" :bone/parent :root
                     :bone/rest {:translation [0 1 0] :rotation [0 0 0] :scale [1 1 1]}}
                    {:bone/id :head :bone/name "Head" :bone/parent :spine
                     :bone/rest {:translation [0 1 0] :rotation [0 0 0] :scale [1 1 1]}}]})
(def default-pose {:pose/bones {}})
(def default-ik {:ik/target [1 1] :ik/elbow :positive})

(defn document [{:keys [id name timeline editor rig pose constraints ik]}]
  {:kami/document :animator-project :kami/version current-version
   :project/id (or id "untitled-animation") :project/name (or name "Untitled Animation")
   :project/timeline timeline :project/editor editor
   :project/rig (or rig default-rig) :project/pose (or pose default-pose)
   :project/constraints (vec constraints) :project/ik (or ik default-ik)})

(defn migrate [value]
  (cond
    (= :animator-project (:kami/document value))
    (case (:kami/version value)
      5 value
      4 (assoc value :kami/version 5 :project/ik default-ik)
      3 (assoc value :kami/version 5 :project/constraints [] :project/ik default-ik)
      2 (assoc value :kami/version 5 :project/rig default-rig :project/pose default-pose :project/constraints [] :project/ik default-ik)
      1 (-> value (assoc :kami/version 5
                         :project/editor {:time 0 :active-target :cube/x :selected nil
                                          :profile :maya}
                         :project/rig default-rig :project/pose default-pose :project/constraints [] :project/ik default-ik)
            (dissoc :project/version))
      (throw (ex-info "Unsupported Animator project version" {:version (:kami/version value)})))
    (:timeline/tracks value)
    (document {:timeline value :editor {:time 0 :active-target (some-> value :timeline/tracks first :track/target)
                                        :selected nil :profile :maya}})
    :else (throw (ex-info "Not an Animator project or timeline" {:value value}))))

(defn valid? [p]
  (and (= :animator-project (:kami/document p)) (= current-version (:kami/version p))
       (string? (:project/id p)) (string? (:project/name p))
       (number? (get-in p [:project/timeline :timeline/duration]))
       (seq (get-in p [:project/timeline :timeline/tracks]))
       (map? (:project/editor p))
       (seq (get-in p [:project/rig :skeleton/bones]))
       (map? (:project/pose p)) (vector? (:project/constraints p))
       (vector? (get-in p [:project/ik :ik/target]))))

(defn open [value]
  (let [p (migrate value)]
    (when-not (valid? p) (throw (ex-info "Invalid Animator project" {:project p}))) p))
