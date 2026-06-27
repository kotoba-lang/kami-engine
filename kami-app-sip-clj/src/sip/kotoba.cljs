(ns sip.kotoba
  "Browser durable client for the 瓶詞 (bottle-letter) async, non-toxic
  multiplayer — the same content-addressed contract as `sip.store`'s `Durable`
  protocol, over Kotoba's block XRPC (`block.put` / `block.get`).

  Backend is chosen at runtime:
    • `window.SIP_KOTOBA_URL` set → real Kotoba server (block XRPC over fetch).
    • otherwise → a localStorage content-addressed store, so letters work with
      no server at all (mirrors how the HUD plays without WebGPU).

  The letter *index* (which CIDs exist per season) lives in localStorage here.
  The bodies are content-addressed (CAS), so they round-trip the same way on
  either backend. A server-shared cross-player index (so you read *other*
  players' bottles, not just your own + the seeds) is the follow-up that needs
  the shared world (sip.store on a deployed Kotoba) — see ADR-0022 P2.

  All ops return a core.async channel (uniform sync/async), yielding the result
  once, then closing."
  (:require [clojure.string :as str]
            [cljs.core.async :as a :refer [<!]])
  (:require-macros [cljs.core.async.macros :refer [go]]))

;; --- channel helper --------------------------------------------------------

(defn- chan-of
  "A channel that yields `v` once (unless nil) and closes."
  [v]
  (let [c (a/chan 1)]
    (when (some? v) (a/put! c v))
    (a/close! c)
    c))

;; --- backend selection -----------------------------------------------------

(defn- base-url []
  (let [u (when (exists? js/window) (aget js/window "SIP_KOTOBA_URL"))]
    (when (and (string? u) (not (str/blank? u))) u)))

;; --- UTF-8 <-> base64 (browser, UTF-8 safe) --------------------------------

(defn- utf8->b64 [s] (js/btoa (js/unescape (js/encodeURIComponent s))))
(defn- b64->utf8 [b] (js/decodeURIComponent (js/escape (js/atob b))))

;; --- localStorage CAS + index ----------------------------------------------

(defn- ls [] (when (exists? js/window) (.-localStorage js/window)))
(defn- ls-get [k] (when-let [s (ls)] (.getItem s k)))
(defn- ls-set [k v] (when-let [s (ls)] (.setItem s k v)))

(defn- local-cid
  "Stable local content address (real server returns its own sha-256 CID)."
  [s] (str "bl" (js/Math.abs (hash s))))

;; --- durable put! / fetch (server or local) --------------------------------

(defn- http-put! [base text]
  (let [out (a/chan 1)]
    (-> (js/fetch (str base "/xrpc/com.etzhayyim.apps.kotoba.block.put")
                  (clj->js {:method "POST"
                            :headers {"content-type" "application/json"}
                            :body (js/JSON.stringify (clj->js {:data_b64 (utf8->b64 text)}))}))
        (.then (fn [r] (.json r)))
        (.then (fn [j] (when-let [cid (.-cid j)] (a/put! out cid)) (a/close! out)))
        (.catch (fn [_] (a/close! out))))
    out))

(defn- http-fetch [base cid]
  (let [out (a/chan 1)]
    (-> (js/fetch (str base "/xrpc/com.etzhayyim.apps.kotoba.block.get?cid=" cid))
        (.then (fn [r] (.json r)))
        (.then (fn [j] (when-let [b (.-data_b64 j)] (a/put! out (b64->utf8 b))) (a/close! out)))
        (.catch (fn [_] (a/close! out))))
    out))

(defn cas-put!
  "Store letter `text` → CID (channel). Server when configured, else localStorage."
  [text]
  (if-let [b (base-url)]
    (http-put! b text)
    (let [cid (local-cid text)]
      (ls-set (str "sip.cas." cid) text)
      (chan-of cid))))

(defn cas-fetch
  "Resolve CID → text (channel; nil/closed if missing)."
  [cid]
  (if-let [b (base-url)]
    (http-fetch b cid)
    (chan-of (ls-get (str "sip.cas." cid)))))

;; --- letter index (localStorage) -------------------------------------------

(def ^:private idx-key "sip.letters")

(defn- read-index []
  (vec (js->clj (js/JSON.parse (or (ls-get idx-key) "[]")) :keywordize-keys true)))

(defn- write-index [v] (ls-set idx-key (js/JSON.stringify (clj->js v))))

(defn- index-add! [{:keys [cid] :as entry}]
  (let [cur (read-index)]
    (when-not (some #(= (:cid %) cid) cur)
      (write-index (conj cur (select-keys entry [:cid :from :text :season]))))))

;; --- 瓶詞 letters over the durable layer -----------------------------------

(defn post-letter!
  "Drop a 瓶詞 into the canal: durably store the body (→ CID) and record it in
  the index. Returns a channel yielding the CID. `m` = {:from :text :season}."
  [{:keys [from text season]}]
  (go (let [cid (<! (cas-put! text))]
        (when (and cid (not (str/blank? cid)))
          (index-add! {:cid cid :from from :text text :season (or season :spring)}))
        cid)))

(defn inbox
  "List 瓶詞 for `season`, hydrating each body from the durable store by CID
  (falling back to the indexed text). Returns a channel yielding a vector."
  [season]
  (go (let [season (keyword season)
            entries (filter #(= (keyword (:season %)) season) (read-index))]
        (loop [es entries, acc []]
          (if (empty? es)
            acc
            (let [e (first es)
                  body (<! (cas-fetch (:cid e)))
                  text (if (and body (not (str/blank? body))) body (:text e))]
              (recur (rest es) (conj acc (assoc e :season season :text text)))))))))

;; --- seed (so the canal isn't empty before a shared index lands) -----------

(def ^:private seeds
  [{:from "都の旅人"   :text "水面に映る灯り、今日もやさしい。" :season :spring}
   {:from "名もなき声" :text "うまく言えないけど、生きててよかった。" :season :spring}
   {:from "運河の猫"   :text "にゃー（また会えたね）。" :season :spring}])

(defn seed!
  "Once (when the canal is empty), float a few example 瓶詞 from other travellers
  so picking one up shows someone else's bottle even solo. Idempotent."
  []
  (when (empty? (read-index))
    (go (doseq [s seeds] (<! (post-letter! s))))))
