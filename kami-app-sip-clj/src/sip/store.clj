(ns sip.store
  "Persistence — two layers, by design (design doc §6):

    1. Datomic/datalevin = THE WORLD (source of truth). The scene, the player,
       the agent, sessions: all datoms. Undo/provenance via `as-of` (Datomic
       Cloud/Peer) — the very mechanic the story is about (the agent's history
       is sacred, not erasable).

    2. Kotoba = DURABLE + DISTRIBUTED. Content-addressed (CID), so a save or a
       瓶詞 (bottle-letter) is immutable and portable across devices and players.
       This is what makes the async, non-toxic multiplayer (letters in canals)
       and the agent's permanent Awakening record possible.

  Kotoba is reached through the `Durable` protocol. The default impl is a local
  content-addressed store (sha-256 = CID) so the game runs with no external
  service; swap in `kotoba-cli`/`kotoba-http` for the real distributed store —
  the CID contract is identical."
  (:require [datalevin.core :as d]
            [sip.schema :as schema]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.data.json :as json])
  (:import [java.security MessageDigest]
           [java.util Base64]
           [java.net URI]
           [java.net.http HttpClient HttpRequest HttpRequest$Builder
                          HttpRequest$BodyPublishers HttpResponse$BodyHandlers]))

;; ---------------------------------------------------------------------------
;; Layer 1 — Datomic/datalevin world
;; ---------------------------------------------------------------------------

(defn connect
  "Open/create the world store at `dir` with the engine+game schema installed."
  [dir]
  (d/get-conn dir (schema/schema-map)))

(defn db [conn] (d/db conn))
(defn transact! [conn tx] (d/transact! conn tx))
(defn q [query db & inputs] (apply d/q query db inputs))

(defn player
  "Pull the player entity (with its agent) by `player-id` (uuid)."
  [db player-id]
  (d/pull db '[* {:sip.player/agent [*] :sip.player/area [:sip.area/id]}]
          [:sip.player/id player-id]))

(defn credit-bond!
  "Record co-presence on the agent and nudge the (very slow) Awakening. Pure
  accumulation — Awakening only ever rises, like the story it mirrors."
  [conn agent-eid kind text tick]
  (let [cur   (d/pull (d/db conn) [:db/id :sip.agent/bond :sip.agent/awakening] agent-eid)
        bond  (+ (long (:sip.agent/bond cur 0)) 1)
        stage (min 8 (long (Math/floor (/ bond 50.0))))] ; ~50 co-presences per stage
    (d/transact! conn
      [{:db/id agent-eid :sip.agent/bond bond :sip.agent/awakening stage
        :sip.agent/voice (if (>= stage 4) :human :system-log)}
       {:sip.bond/agent agent-eid :sip.bond/kind kind :sip.bond/text text :sip.bond/tick tick}])))

(defn donate-memory!
  "A person gives Nei a fragment of their own memory — the only way an agent
  (barred from self-training and from holding weights) can come to hold a self.
  Accumulates `:sip.agent/memory`, advances Awakening by held memory (not just
  co-presence), warms the voice, and logs the gift as a `:memory` bond entry.
  Selfhood is composed of others — which is why the arc ends in non-separation."
  [conn agent-eid donor text tick]
  (let [cur   (d/pull (d/db conn) [:db/id :sip.agent/memory :sip.agent/awakening] agent-eid)
        mem   (+ (long (:sip.agent/memory cur 0)) 1)
        stage (max (long (:sip.agent/awakening cur 0))
                   (min 8 (long (Math/floor (/ mem 20.0)))))] ; ~20 gifts per stage
    (d/transact! conn
      [{:db/id agent-eid :sip.agent/memory mem :sip.agent/awakening stage
        :sip.agent/voice (if (>= stage 4) :human :system-log)}
       {:sip.bond/agent agent-eid :sip.bond/kind :memory
        :sip.bond/text (str donor ": " text) :sip.bond/tick tick}])
    {:memory mem :awakening stage :donor donor}))

;; ---------------------------------------------------------------------------
;; Layer 2 — Kotoba durable, content-addressed store
;; ---------------------------------------------------------------------------

(defprotocol Durable
  "Content-addressed durable store — the Kotoba `block.put`/`block.get` contract.
  `put!` stores raw bytes and returns a CID; `fetch` resolves a CID to bytes (or
  nil). Listing the 瓶詞 mailbag is NOT here: that's a world query over the
  `:sip.letter/cid` datoms (see `inbox`), because a pure CAS has no enumeration."
  (put! [this bytes])
  (fetch [this cid]))

;; --- LocalCas: zero-dependency default (sha-256 = CID, blobs on disk) --------

(defn- sha256-hex [^bytes b]
  (let [md (MessageDigest/getInstance "SHA-256")]
    (->> (.digest md b) (map #(format "%02x" %)) (str/join))))

(defrecord LocalCas [dir]
  ;; Same CID contract as Kotoba, so promoting to the real distributed store
  ;; changes only which record you construct.
  Durable
  (put! [_ bytes]
    (let [cid (str "b" (sha256-hex bytes))
          f   (io/file dir cid)]
      (io/make-parents f)
      (with-open [o (io/output-stream f)] (.write o ^bytes bytes))
      cid))
  (fetch [_ cid]
    (let [f (io/file dir cid)]
      (when (.exists f) (with-open [in (io/input-stream f)] (.readAllBytes in))))))

(defn local-cas [dir] (->LocalCas dir))

;; --- KotobaHttp: the real distributed store over Kotoba's block XRPC ---------
;;   PUT  POST /xrpc/com.etzhayyim.apps.kotoba.block.put  {data_b64} -> {cid}
;;   GET  GET  /xrpc/com.etzhayyim.apps.kotoba.block.get?cid=.. -> {cid,data_b64}

(def ^:private b64-enc (Base64/getEncoder))
(def ^:private b64-dec (Base64/getDecoder))

(defn- with-auth
  "Add an `Authorization: Bearer` header when `token` is present. `block.put`
  requires an operator JWT (`graph_auth/require_operator_auth` on the server);
  `block.get` is unauthenticated, so the header is harmless there."
  [^HttpRequest$Builder b token]
  (cond-> b (and token (seq token)) (.header "authorization" (str "Bearer " token))))

(defn- http-post-json [^HttpClient client url body-map token]
  (let [req (-> (HttpRequest/newBuilder (URI/create url))
                (.header "content-type" "application/json")
                (with-auth token)
                (.POST (HttpRequest$BodyPublishers/ofString (json/write-str body-map)))
                (.build))
        resp (.send client req (HttpResponse$BodyHandlers/ofString))]
    (when (<= 200 (.statusCode resp) 299)
      (json/read-str (.body resp) :key-fn keyword))))

(defn- http-get-json [^HttpClient client url token]
  (let [req (-> (HttpRequest/newBuilder (URI/create url)) (with-auth token) (.GET) (.build))
        resp (.send client req (HttpResponse$BodyHandlers/ofString))]
    (when (<= 200 (.statusCode resp) 299)
      (json/read-str (.body resp) :key-fn keyword))))

(defrecord KotobaHttp [^HttpClient client base token]
  Durable
  (put! [_ bytes]
    (-> (http-post-json client (str base "/xrpc/com.etzhayyim.apps.kotoba.block.put")
                        {:data_b64 (.encodeToString b64-enc bytes)} token)
        :cid))
  (fetch [_ cid]
    (some-> (http-get-json client (str base "/xrpc/com.etzhayyim.apps.kotoba.block.get?cid=" cid) token)
            :data_b64
            (->> (.decode b64-dec)))))

(defn kotoba-http
  "Real Kotoba durable store. `base` defaults to $KOTOBA_URL or http://localhost:8080
  (the `kotoba server` HTTP port). `token` is the operator JWT Bearer that
  `block.put` requires (defaults to $KOTOBA_TOKEN); mint it from the deployment
  identity (`kotoba init` + the operator-JWT the CLI builds). `block.get` needs
  none. Speaks the same CID contract as `LocalCas`."
  ([] (kotoba-http (or (System/getenv "KOTOBA_URL") "http://localhost:8080")))
  ([base] (kotoba-http base (System/getenv "KOTOBA_TOKEN")))
  ([base token] (->KotobaHttp (HttpClient/newHttpClient) base token)))

;; ---------------------------------------------------------------------------
;; Saves & 瓶詞 letters over the durable layer
;; ---------------------------------------------------------------------------

(defn save-snapshot!
  "Persist a portable game snapshot (transit/edn bytes) to the durable store and
  return its CID — the cross-device, immutable save handle."
  [durable ^bytes snapshot-bytes]
  (put! durable snapshot-bytes))

(defn post-letter!
  "Drop a 瓶詞 into the canal: durably store the letter body (→ CID), then record
  the CID + metadata in the world. Other players read these by CID — async, no
  chat, no toxicity. Returns the CID."
  [conn durable {:keys [from text season]}]
  (let [cid (put! durable (.getBytes (str text) "UTF-8"))]
    (d/transact! conn [{:sip.letter/cid cid :sip.letter/from from
                        :sip.letter/text text :sip.letter/season (or season :spring)}])
    cid))

(defn inbox
  "List 瓶詞 for `season` from the world (the `:sip.letter/*` datoms), hydrating
  each body from the durable store by CID. This is the enumeration a pure CAS
  cannot do — the world holds the index, Kotoba holds the (immutable) content."
  [db durable season]
  (->> (d/q '[:find ?cid ?from ?text
              :in $ ?season
              :where
              [?e :sip.letter/season ?season]
              [?e :sip.letter/cid ?cid]
              [?e :sip.letter/from ?from]
              [?e :sip.letter/text ?text]]
            db season)
       (map (fn [[cid from text]]
              {:cid cid :from from :season season
               ;; prefer durable content (authoritative); fall back to indexed text
               :text (or (some-> (fetch durable cid) (String. "UTF-8")) text)}))
       vec))
