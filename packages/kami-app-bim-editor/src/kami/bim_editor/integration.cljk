(ns kami.bim-editor.integration
  "Application boundary for publishing one coordinated design revision."
  (:require [kotoba.lang.text :as string]
            [bim.integration :as bim-integration]
            [bim.interchange :as interchange]
            [bim.spatial :as spatial]))

(defn reconcile-mep-systems
  "Replace embedded MEP member snapshots with the current authored elements and
  remove deleted members before persistence or exchange."
  [project systems]
  (let [elements (mapcat :elements
                         (mapcat :storeys (mapcat :buildings (:sites project))))
        by-id (into {} (map (juxt :id identity)) elements)
        current (fn [members] (vec (keep #(get by-id (:id %)) members)))]
    (mapv #(-> %
               (update :mep/segments current)
               (update :mep/fittings current)
               (update :mep/equipment current))
          systems)))

(defn coordinated-model
  "Attach editor-owned analytical/system state to the authored building before
  IFC, CDE, drawing, or cloud publication."
  [project structural-model mep-systems]
  (cond-> (assoc project :mep/systems (reconcile-mep-systems project mep-systems))
    structural-model (assoc :structural/model structural-model)))

(defn coordinated-revision
  [{:keys [project project-id project-name revision events structural-model mep-systems]}]
  (let [project (coordinated-model project structural-model mep-systems)]
    {:kami/document :coordinated-bim-revision
   :kami/version 1
   :project/id project-id
   :project/name project-name
   :project/revision revision
   :project/model project
   :project/federation (bim-integration/federated-design
                        {:architectural project :structural [] :mep []})
   :project/drawings (bim-integration/generate-drawing-set project)
   :project/ifc (bim-integration/export-ifc project)
   :project/collaboration {:events (vec events)}
     :project/cloud-itonami (bim-integration/cloud-itonami-payload project revision)}))

(defn model-bounds
  "Return the tight world-space AABB for renderable or metadata-only elements."
  [elements]
  (when-let [bounds (seq (keep spatial/element-bounds elements))]
    {:min (apply mapv min (map :min bounds))
     :max (apply mapv max (map :max bounds))}))

(defn large-model-view
  "Build a metadata-first visible-set plan suitable for WebGPU workers."
  [elements view-bounds camera-position focal-length-px
   {:keys [cell-size batch-size max-resident max-loads]
    :or {cell-size 10.0 batch-size 512 max-resident 10000 max-loads 1000}}]
  (let [index (spatial/build-index elements {:cell-size cell-size})
        plan (spatial/stream-plan index view-bounds camera-position focal-length-px
                                  batch-size {:include-elements? false})
        delta (spatial/stream-delta (spatial/stream-session) plan
                                    {:max-resident max-resident :max-loads max-loads})]
    {:workspace/spatial-index index :workspace/query (spatial/query-report index view-bounds)
     :workspace/stream-plan plan :workspace/stream-delta delta}))

(defn capability-status
  "Summarize whether each integrated workspace discipline has publishable
  state, so UI and automation share the same readiness rules."
  [{:keys [project family-catalog structural-model mep-systems drawing-set
           review-topics cloud-state electrical-distribution]}]
  {:architecture (boolean project)
   :families (boolean (seq (:family-catalog/families family-catalog)))
   :structure (boolean structural-model)
   :mep (boolean (seq mep-systems))
   :electrical (boolean electrical-distribution)
   :drawings (boolean (seq (:drawing/views drawing-set)))
   :coordination (boolean (seq review-topics))
   :opencde (boolean cloud-state)
   :ifc (boolean project)})

(defn apply-remote-events [project events]
  (bim-integration/merge-events project events))

(defn coordinated-ifc [project]
  (bim-integration/export-ifc project))

(defn import-ifc-spf [text]
  (bim-integration/import-ifc-spf text))

(defn advance-cloud-workspace
  "Create or advance the durable collaboration history to `project`.
  A whole top-level semantic field is represented by one deterministic event;
  unchanged retries return the same workspace and revision id."
  [workspace project actor]
  (let [workspace (or workspace (bim-integration/collaboration-workspace project))
        branch (:collab/default-branch workspace)
        head (get-in workspace [:collab/branches branch])
        previous (get-in workspace [:collab/revisions head :revision/document])]
    (if (= previous project)
      workspace
      (let [clock (inc (reduce max 0 (keep :revision/clock
                                           (vals (:collab/revisions workspace)))))
            revision-id (str "editor-" clock)
            keys* (sort-by str (into (set (keys previous)) (keys project)))
            events (->> keys*
                        (keep-indexed
                         (fn [index key]
                           (when (not= (get previous key ::missing)
                                       (get project key ::missing))
                             (bim-integration/change-event
                              {:id (str revision-id "-" index) :actor actor :clock clock
                               :operation (if (contains? project key) :assoc :dissoc)
                               :path [key] :value (get project key)}))))
                        vec)]
        (bim-integration/commit-design-revision
         workspace {:id revision-id :branch branch :base-revision head
                    :actor actor :clock clock :message "BIM editor publish"
                    :events events})))))

(defn cloud-sync-package
  "Return the advanced workspace and exact replay delta sent to cloud-itonami."
  [workspace checkpoint project actor]
  (let [workspace (advance-cloud-workspace workspace project actor)
        checkpoint (or checkpoint {:sync/revisions #{}})]
    {:workspace workspace
     :envelope (bim-integration/cloud-itonami-sync-payload workspace checkpoint)}))

(defn cloud-sync-request
  "Build the browser HTTP request without exposing fetch to shared code."
  ([config envelope] (cloud-sync-request config envelope nil))
  ([{:keys [base-url org repo cacao]} envelope publication]
   (when-not (and (string? base-url) (seq base-url)
                  (string? org) (seq org) (string? repo) (seq repo)
                  (string? cacao) (seq cacao))
     (throw (ex-info "cloud-itonami endpoint and CACAO are required" {})))
   {:url (str (string/replace base-url #"/+$" "")
              "/api/" org "/" repo "/design/sync")
    :method "POST"
    :headers {"content-type" "application/json"
              "authorization" (str "CACAO " cacao)}
    :body (cond-> {:envelopeEdn (pr-str envelope)}
            publication (assoc :publicationEdn (pr-str publication)))}))

(defn cloud-opencde-publication
  "Build the exact wire package consumed by cloud-itonami.design-opencde for
  any document kind (IFC, or an analysis-result document such as structural,
  MEP, or electrical distribution). The document is bound to the
  collaboration envelope's durable head; topic revisions and idempotency
  keys remain stable across retries."
  [sync-payload {:keys [document-id name content content-ref content-hash base-version
                        timestamp topic-revisions topics media-type]
                 :or {media-type "application/ifc"}}]
  (when-not (= :design/collaboration-synchronized (:itonami/event sync-payload))
    (throw (ex-info "OpenCDE publication requires collaboration sync payload" {})))
  (let [project-id (:design/project-id sync-payload)
        head (:design/head-revision sync-payload)]
    (when-not (and project-id head document-id name (string? content) (seq content)
                   content-ref content-hash
                   (integer? base-version) (<= 0 base-version))
      (throw (ex-info "incomplete cloud OpenCDE publication metadata"
                      {:project-id project-id :head head :document-id document-id})))
    {:document {:project-id project-id :design-revision head
                :document-id document-id :name name :media-type media-type
                :content content
                :content-ref content-ref :content-hash content-hash
                :base-version base-version
                :idempotency-key (str project-id ":" head ":" document-id ":" content-hash)
                :metadata {:design/revision head} :timestamp timestamp}
     :topics
     (mapv (fn [topic]
             (let [guid (:bcf.topic/guid topic)
                   revision (get topic-revisions guid 0)]
               (when-not guid
                 (throw (ex-info "OpenCDE topic requires BCF guid" {:topic topic})))
               {:guid guid :topic topic :expected-revision revision
                :idempotency-key (str project-id ":" head ":bcf:" guid ":" revision)}))
           topics)}))

(defn export-drawing
  "Export an active floor plan or the full storey set through shared formats."
  ([project active-storey-id format]
   (export-drawing project active-storey-id format {} nil))
  ([project active-storey-id format drawing-views]
   (export-drawing project active-storey-id format drawing-views nil))
  ([project active-storey-id format drawing-views print-setting]
   (export-drawing project active-storey-id format drawing-views print-setting nil))
  ([project active-storey-id format drawing-views print-setting drawing-set]
   (let [storeys (mapcat :storeys (mapcat :buildings (:sites project)))
         storey (first (filter #(= active-storey-id (:id %)) storeys))
         annotations-by-storey
         (into {} (map (fn [[storey-id view]] [storey-id (:view/annotations view)]))
               drawing-views)]
     (case format
       :dxf {:filename (str "floor-plan-" active-storey-id ".dxf")
             :media-type "application/dxf"
             :content (interchange/floor-plan-dxf
                       storey {:annotations (get annotations-by-storey active-storey-id)})}
       :pdf {:filename "building-drawing-set.pdf" :media-type "application/pdf"
             :content (interchange/drawing-set-pdf
                       (vec storeys) {:annotations-by-storey annotations-by-storey
                                      :print-setting print-setting
                                      :drawing-set drawing-set})}
       (throw (ex-info "unsupported BIM drawing export format" {:format format}))))))
