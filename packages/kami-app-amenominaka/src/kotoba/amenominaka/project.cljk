(ns kotoba.amenominaka.project
  "Versioned Amenominaka project documents and deterministic migrations."
  (:require [kotoba.amenominaka.scene :as scene]))

(def current-version 2)

(defn document
  [{:keys [id name building environment camera timeline updated-at]
    :or {id "untitled" name "Untitled" environment {} camera {} timeline [] updated-at 0}}]
  {:amenominaka/type :project
   :amenominaka/version current-version
   :project/id id :project/name name :project/updated-at updated-at
   :project/building building :project/environment environment
   :project/camera camera :project/timeline (vec timeline)})

(defn valid-document? [doc]
  (and (map? doc)
       (= :project (:amenominaka/type doc))
       (= current-version (:amenominaka/version doc))
       (string? (:project/id doc)) (string? (:project/name doc))
       (scene/bim-project? (:project/building doc))
       (map? (:project/environment doc)) (map? (:project/camera doc))
       (vector? (:project/timeline doc))))

(defn migrate
  "Migrate a legacy raw BIM project or an older Amenominaka document."
  [value]
  (cond
    (scene/bim-project? value)
    (document {:id "migrated-bim" :name (:name value "Migrated BIM") :building value})

    (and (= :project (:amenominaka/type value)) (= 1 (:amenominaka/version value)))
    (document {:id (:project/id value "migrated-v1") :name (:project/name value "Migrated Project")
               :building (:project/building value) :environment (:project/environment value {})
               :camera (:project/camera value {}) :timeline (:project/timeline value [])
               :updated-at (:project/updated-at value 0)})

    (valid-document? value) value
    :else (throw (ex-info "unsupported or invalid Amenominaka project"
                          {:amenominaka.project/error :invalid-project}))))

(defn revision-key [doc]
  (str (:project/id doc) ":" (:project/updated-at doc)))
