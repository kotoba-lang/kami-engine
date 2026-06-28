(ns sip.research
  "P3-B foundation — research data model + ingest/query for the n=1000 study
  (ADR-0022 P3). The Datomic/datalevin schema and the write path that the old
  Svelte/TS researcher (Participant / Session / WordStatistics / EmotionVector)
  needs, on the clj+Kotoba substrate.

  Local datalevin here mirrors `sip.store`'s world; the *shared* cross-researcher
  store is Kotoba's Datomic XRPC (`datomic.transact` for ingest — operator JWT;
  `datomic.q` for read), see docs/p2-shared-index.md and docs/p3b-data-collection.md.

  PRIVACY: PII (email / medical history) is stored access-controlled and is NEVER
  surfaced by the read-only dashboard (`sip.researcher` lists `isPublic`
  participants by demographics only). Real collection requires consent + the
  write-auth/Datomic-Cloud decisions in the P3-B design doc — this ns is the
  data foundation, not a consent-bearing collector."
  (:require [datalevin.core :as d]
            [clojure.edn :as edn]
            [clojure.data.json :as json])
  (:import [java.net URI]
           [java.net.http HttpClient HttpRequest HttpRequest$Builder
                          HttpRequest$BodyPublishers HttpResponse$BodyHandlers]))

(def emotion-axes
  [:joy :sadness :anger :fear :surprise :disgust :calm :focus :excitement :confusion])

(defn schema-map
  "datalevin schema for participants, sessions, and per-(session,word) emotion
  vectors. Emotion-sum attrs are schemaless longs (no entry needed); only ref /
  unique / typed attrs are declared."
  []
  {:research.participant/id          {:db/valueType :db.type/string  :db/unique :db.unique/identity}
   :research.participant/public?     {:db/valueType :db.type/boolean}
   :research.session/id              {:db/valueType :db.type/string  :db/unique :db.unique/identity}
   :research.session/participant     {:db/valueType :db.type/ref}
   :research.emotion/session         {:db/valueType :db.type/ref}})

(defn connect [dir] (d/get-conn dir (schema-map)))
(defn db [conn] (d/db conn))

(defn- participant-tx [{:keys [id age-group gender ethnicity income-range email public? created-at]}]
  (cond-> {:research.participant/id id}
    age-group    (assoc :research.participant/age-group age-group)
    gender       (assoc :research.participant/gender gender)
    ethnicity    (assoc :research.participant/ethnicity ethnicity)
    income-range (assoc :research.participant/income-range income-range)
    email        (assoc :research.participant/email email)        ; PII — access-controlled
    (some? public?) (assoc :research.participant/public? public?)
    created-at   (assoc :research.participant/created-at created-at)))

(defn ingest-session!
  "Upsert a participant, its session, and the session's per-word emotion vectors.
  `m` = {:participant {:id ..} :session {:id :word-count :created-at}
         :emotions [{:word .. :joy .. ... :entry-count ..} ...]}.
  Returns the session id. (The shared variant POSTs the same datoms to Kotoba's
  `datomic.transact` XRPC with the operator JWT — see the P3-B design doc.)"
  [conn {:keys [participant session emotions]}]
  (let [pid (:id participant)
        sid (:id session)]
    ;; participant + session first, so emotion datoms can resolve the session by
    ;; its unique id as a lookup ref.
    (d/transact! conn
      [(participant-tx participant)
       (cond-> {:research.session/id sid
                :research.session/participant [:research.participant/id pid]}
         (:word-count session) (assoc :research.session/word-count (:word-count session))
         (:created-at session) (assoc :research.session/created-at (:created-at session)))])
    (when (seq emotions)
      (d/transact! conn
        (for [e emotions]
          (into {:research.emotion/session [:research.session/id sid]
                 :research.emotion/word (:word e)
                 :research.emotion/entry-count (long (:entry-count e 0))}
                (for [ax emotion-axes] [(keyword "research.emotion" (name ax)) (long (get e ax 0))])))))
    sid))

(defn participants
  "All participants (demographics + public flag). PII attrs are intentionally
  not pulled here — this is the dashboard-facing read."
  [db]
  (->> (d/q '[:find (pull ?p [:research.participant/id :research.participant/age-group
                              :research.participant/gender :research.participant/public?])
              :where [?p :research.participant/id]]
            db)
       (map first)))

(defn sessions-for [db pid]
  (->> (d/q '[:find (pull ?s [:research.session/id :research.session/word-count :research.session/created-at])
              :in $ ?pid
              :where
              [?p :research.participant/id ?pid]
              [?s :research.session/participant ?p]]
            db pid)
       (map first)))

(defn session-emotion-summary
  "Aggregate a session's emotion sums across all its words → {axis total}."
  [db sid]
  (let [rows (d/q '[:find ?ax ?v
                    :in $ ?sid [?ax ...]
                    :where
                    [?s :research.session/id ?sid]
                    [?e :research.emotion/session ?s]
                    [?e ?ax ?v]]
                  db sid (map #(keyword "research.emotion" (name %)) emotion-axes))]
    (reduce (fn [acc [ax v]]
              (update acc (keyword (name ax)) (fnil + 0) (or v 0)))
            {} rows)))

;; ---------------------------------------------------------------------------
;; Shared backend — Kotoba Datomic XRPC (datomic.transact / datomic.q)
;;   The cross-researcher store. ingest requires the operator JWT (Bearer);
;;   reads use datomic.q. `graph` is the shared research graph (CID/name).
;;   See docs/p2-shared-index.md / docs/p3b-data-collection.md.
;; ---------------------------------------------------------------------------

(defn- session-tx-data
  "Full datom vector for a session (participant upsert + session + emotions) as a
  single Datomic transaction — lookup refs resolve in-tx on a real Datomic."
  [{:keys [participant session emotions]}]
  (let [pid (:id participant) sid (:id session)]
    (into [(participant-tx participant)
           (cond-> {:research.session/id sid
                    :research.session/participant [:research.participant/id pid]}
             (:word-count session) (assoc :research.session/word-count (:word-count session))
             (:created-at session) (assoc :research.session/created-at (:created-at session)))]
          (for [e emotions]
            (into {:research.emotion/session [:research.session/id sid]
                   :research.emotion/word (:word e)
                   :research.emotion/entry-count (long (:entry-count e 0))}
                  (for [ax emotion-axes] [(keyword "research.emotion" (name ax)) (long (get e ax 0))]))))))

(defn- with-auth [^HttpRequest$Builder b token]
  (cond-> b (and token (seq token)) (.header "authorization" (str "Bearer " token))))

(defn- xrpc-post [^HttpClient client base nsid body token]
  (let [req (-> (HttpRequest/newBuilder (URI/create (str base "/xrpc/" nsid)))
                (.header "content-type" "application/json")
                (with-auth token)
                (.POST (HttpRequest$BodyPublishers/ofString (json/write-str body)))
                (.build))
        resp (.send client req (HttpResponse$BodyHandlers/ofString))]
    (when (<= 200 (.statusCode resp) 299)
      (json/read-str (.body resp) :key-fn keyword))))

(defrecord KotobaGraph [^HttpClient client base graph token])

(defn kotoba-graph
  "Shared research store over Kotoba Datomic XRPC. `graph` is the shared research
  graph (CID/name). `token` (operator JWT / CACAO) authorises ingest and reads.
  Default base is the hosted Kotoba CF Worker at https://kotobase.net — its
  kotoba-datomic gives `as_of`/`history` natively, so the study's provenance
  needs NO separate Datomic Cloud. Override with $KOTOBA_URL / $KOTOBA_TOKEN."
  ([graph] (kotoba-graph graph (or (System/getenv "KOTOBA_URL") "https://kotobase.net")
                         (System/getenv "KOTOBA_TOKEN")))
  ([graph base token] (->KotobaGraph (HttpClient/newHttpClient) base graph token)))

(defn kotoba-ingest-session!
  "Ingest a session into the shared graph via datomic.transact (tx_edn = the
  session datoms as EDN). Requires the operator JWT. Returns the response map."
  [{:keys [client base graph token]} m]
  (xrpc-post client base "com.etzhayyim.apps.kotoba.datomic.transact"
             {:graph graph :tx_edn (pr-str (session-tx-data m))} token))

(defn- q-rows
  "Run a Datalog query over the shared graph; rows of EDN-parsed cells.
  `opts` {:as-of <basis-t>} reads the graph at that point (Kotoba-native
  time-travel via datomic.q `as_of`)."
  ([kg query inputs] (q-rows kg query inputs nil))
  ([{:keys [client base graph token]} query inputs {:keys [as-of]}]
   (->> (xrpc-post client base "com.etzhayyim.apps.kotoba.datomic.q"
                   (cond-> {:graph graph :query_edn (pr-str query) :inputs_edn (mapv pr-str inputs)}
                     as-of (assoc :as_of (str as-of)))
                   token)
        :rows_edn
        (mapv (fn [row] (mapv edn/read-string row))))))

(defn kotoba-participants
  "Read public participants (demographics only, no PII) from the shared graph.
  Pass `as-of` (a basis-t string) to read the cohort as it was at that point —
  the provenance/reproducibility the n=1000 study needs, served by Kotoba's own
  datomic.q (kotobase.net), not Datomic Cloud."
  ([kg] (kotoba-participants kg nil))
  ([kg as-of]
   (->> (q-rows kg '[:find ?id ?ag ?g
                     :where
                     [?p :research.participant/id ?id]
                     [?p :research.participant/public? true]
                     [(get-else $ ?p :research.participant/age-group "") ?ag]
                     [(get-else $ ?p :research.participant/gender "") ?g]]
                [] {:as-of as-of})
        (mapv (fn [[id ag g]] {:research.participant/id id
                               :research.participant/age-group ag
                               :research.participant/gender g})))))
