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
  (:require [datalevin.core :as d]))

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
