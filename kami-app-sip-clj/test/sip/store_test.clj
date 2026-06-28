(ns sip.store-test
  "Durable-store tests: the Kotoba `block.put`/`block.get` CID contract and the
  瓶詞 (bottle-letter) flow. Run with `clojure -M:datomic:test`.

  `KotobaHttp` is exercised against an in-process JDK HttpServer that implements
  the exact block XRPC contract (request `{data_b64}` → `{cid}`, GET `?cid=` →
  `{cid,data_b64}`), so the real HTTP client is verified without standing up the
  whole kotoba-server. `LocalCas` + `inbox` run against a real datalevin store."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.data.json :as json]
            [clojure.java.io :as io]
            [sip.store :as store])
  (:import [com.sun.net.httpserver HttpServer HttpHandler]
           [java.net InetSocketAddress]
           [java.security MessageDigest]
           [java.util Base64]))

;; --- in-process mock of Kotoba's block.put / block.get ----------------------

(defn- sha [^bytes b]
  (->> (.digest (MessageDigest/getInstance "SHA-256") b)
       (map #(format "%02x" %)) (apply str)))

(defn- query->map [q]
  (into {} (for [pair (some-> q (.split "&")) :let [[k v] (.split pair "=")]] [k v])))

(defn- respond [exch ^String body]
  (let [bs (.getBytes body "UTF-8")]
    (.sendResponseHeaders exch 200 (count bs))
    (with-open [os (.getResponseBody exch)] (.write os bs))))

(defn start-mock-kotoba!
  "Start an in-process HttpServer speaking the block XRPC contract. Returns
  {:base url :stop (fn)}. CID = \"b\"+sha256 (CAS), blocks kept in an atom."
  []
  (let [blocks (atom {})
        auth   (atom nil)   ; records the Authorization header seen on block.put
        srv (HttpServer/create (InetSocketAddress. "127.0.0.1" 0) 0)
        b64 (Base64/getDecoder)
        enc (Base64/getEncoder)]
    (.createContext srv "/xrpc/com.etzhayyim.apps.kotoba.block.put"
      (reify HttpHandler
        (handle [_ exch]
          (reset! auth (.getFirst (.getRequestHeaders exch) "Authorization"))
          (let [in (json/read-str (slurp (.getRequestBody exch)) :key-fn keyword)
                bytes (.decode b64 ^String (:data_b64 in))
                cid (str "b" (sha bytes))]
            (swap! blocks assoc cid bytes)
            (respond exch (json/write-str {:cid cid}))))))
    (.createContext srv "/xrpc/com.etzhayyim.apps.kotoba.block.get"
      (reify HttpHandler
        (handle [_ exch]
          (let [cid (get (query->map (.getQuery (.getRequestURI exch))) "cid")
                bytes (get @blocks cid)]
            (respond exch (json/write-str {:cid cid
                                           :data_b64 (.encodeToString enc bytes)}))))))
    (.setExecutor srv nil)
    (.start srv)
    {:base (str "http://127.0.0.1:" (.getPort (.getAddress srv))) :auth auth :stop #(.stop srv 0)}))

;; --- tests ------------------------------------------------------------------

(deftest local-cas-round-trip
  (testing "LocalCas: bytes → CID → identical bytes"
    (let [dir (str (System/getProperty "java.io.tmpdir") "/sip-cas-" (hash (str (java.util.UUID/randomUUID))))
          cas (store/local-cas dir)
          payload (.getBytes "種をまく — planting a seed" "UTF-8")
          cid (store/put! cas payload)]
      (is (string? cid))
      (is (= (seq payload) (seq (store/fetch cas cid))) "fetched bytes equal stored bytes")
      (is (= cid (store/put! cas payload)) "same content → same CID (content-addressed)"))))

(deftest kotoba-http-round-trip
  (testing "KotobaHttp speaks the real block.put/block.get contract"
    (let [{:keys [base stop]} (start-mock-kotoba!)]
      (try
        (let [k (store/kotoba-http base)
              payload (.getBytes "瓶詞 / letter in a bottle" "UTF-8")
              cid (store/put! k payload)]
          (is (string? cid))
          (is (= (seq payload) (seq (store/fetch k cid))) "round-trips bytes over HTTP"))
        (finally (stop))))))

(deftest kotoba-http-sends-operator-token
  (testing "block.put carries the operator JWT as a Bearer token (server requires it)"
    (let [{:keys [base auth stop]} (start-mock-kotoba!)]
      (try
        (let [k (store/kotoba-http base "operator-jwt-xyz")]
          (store/put! k (.getBytes "瓶詞" "UTF-8"))
          (is (= "Bearer operator-jwt-xyz" @auth)
              "real kotoba block.put needs Authorization: Bearer <operator JWT>"))
        (finally (stop)))
      ;; and no token → no header (LocalCas / unauthenticated reads still fine)
      (let [{:keys [base auth stop]} (start-mock-kotoba!)]
        (try
          (store/put! (store/kotoba-http base nil) (.getBytes "x" "UTF-8"))
          (is (nil? @auth) "no token configured → no Authorization header")
          (finally (stop)))))))

(deftest letter-inbox-flow
  (testing "post-letter! durably stores body + records CID; inbox lists & hydrates"
    (let [{:keys [base stop]} (start-mock-kotoba!)
          dir (str (System/getProperty "java.io.tmpdir") "/sip-world-" (hash (str (java.util.UUID/randomUUID))))]
      (try
        (let [conn (store/connect dir)
              k (store/kotoba-http base)
              cid (store/post-letter! conn k {:from "見習い" :text "今日も運河が静かでした"
                                              :season :spring})]
          (is (string? cid))
          (let [letters (store/inbox (store/db conn) k :spring)]
            (is (= 1 (count letters)))
            (is (= "今日も運河が静かでした" (:text (first letters))) "body hydrated from Kotoba by CID")
            (is (= cid (:cid (first letters))))
            (is (empty? (store/inbox (store/db conn) k :winter)) "season filter works")))
        (finally (stop))))))
