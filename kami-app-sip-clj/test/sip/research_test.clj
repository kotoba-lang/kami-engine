(ns sip.research-test
  "P3-B foundation tests — ingest a research session and read it back (datalevin).
  Run with `clojure -M:datomic:test`."
  (:require [clojure.test :refer [deftest is testing]]
            [clojure.data.json :as json]
            [clojure.edn :as edn]
            [sip.research :as r])
  (:import [com.sun.net.httpserver HttpServer HttpHandler]
           [java.net InetSocketAddress]))

(defn- tmp-conn []
  (r/connect (str (System/getProperty "java.io.tmpdir") "/sip-research-"
                  (System/currentTimeMillis) "-" (rand-int 1000000))))

(deftest ingest-and-read
  (testing "ingest a session → participants / sessions / emotion summary read back"
    (let [conn (tmp-conn)
          _   (r/ingest-session! conn
                {:participant {:id "p-0007" :age-group "20-29" :gender "F"
                               :email "secret@example.com" :public? true}
                 :session     {:id "s1" :word-count 142}
                 :emotions    [{:word "海"  :sadness 20 :calm 5 :entry-count 3}
                               {:word "灯" :calm 7 :joy 4 :entry-count 2}]})
          db  (r/db conn)
          ps  (r/participants db)
          ss  (r/sessions-for db "p-0007")
          sum (r/session-emotion-summary db "s1")]
      (is (= 1 (count ps)))
      (is (= "20-29" (:research.participant/age-group (first ps))))
      (is (true? (:research.participant/public? (first ps))))
      (is (nil? (:research.participant/email (first ps)))
          "dashboard read must not surface PII (email)")
      (is (= 1 (count ss)))
      (is (= "s1" (:research.session/id (first ss))))
      (is (= 142 (:research.session/word-count (first ss))))
      (is (= 20 (:sadness sum 0)) "sadness summed across words")  ; 20 + 0
      (is (= 12 (:calm sum 0))    "calm summed across words")     ; 5 + 7
      (is (= 4  (:joy sum 0))     "joy summed across words"))))

(deftest upsert-participant
  (testing "ingesting a second session for the same participant upserts (no dup)"
    (let [conn (tmp-conn)]
      (r/ingest-session! conn {:participant {:id "p-1" :public? true} :session {:id "a"} :emotions []})
      (r/ingest-session! conn {:participant {:id "p-1" :public? true} :session {:id "b"} :emotions []})
      (let [db (r/db conn)]
        (is (= 1 (count (r/participants db))) "participant upserted by id, not duplicated")
        (is (= 2 (count (r/sessions-for db "p-1"))) "both sessions linked")))))

;; --- shared backend: Kotoba Datomic XRPC (in-process mock) ------------------

(defn- start-mock-graph! [rows]
  (let [tx (atom nil) auth (atom nil) qreq (atom nil)
        srv (HttpServer/create (InetSocketAddress. "127.0.0.1" 0) 0)
        respond (fn [exch ^String s] (let [b (.getBytes s "UTF-8")]
                                       (.sendResponseHeaders exch 200 (count b))
                                       (with-open [o (.getResponseBody exch)] (.write o b))))]
    (.createContext srv "/xrpc/com.etzhayyim.apps.kotoba.datomic.transact"
      (reify HttpHandler
        (handle [_ e]
          (reset! auth (.getFirst (.getRequestHeaders e) "Authorization"))
          (reset! tx (:tx_edn (json/read-str (slurp (.getRequestBody e)) :key-fn keyword)))
          (respond e (json/write-str {:graph "g" :basis_t "1"})))))
    (.createContext srv "/xrpc/com.etzhayyim.apps.kotoba.datomic.q"
      (reify HttpHandler
        (handle [_ e]
          (reset! qreq (json/read-str (slurp (.getRequestBody e)) :key-fn keyword))
          (respond e (json/write-str {:graph "g" :rows_edn rows})))))
    (.start srv)
    {:base (str "http://127.0.0.1:" (.getPort (.getAddress srv)))
     :tx tx :auth auth :qreq qreq :stop #(.stop srv 0)}))

(deftest kotoba-shared-backend
  (testing "ingest → datomic.transact (operator JWT + tx_edn) ; participants ← datomic.q"
    (let [{:keys [base tx auth stop]} (start-mock-graph! [["\"p-0007\"" "\"20-29\"" "\"F\""]])]
      (try
        (let [kg (r/kotoba-graph "research-graph" base "op-jwt")]
          (r/kotoba-ingest-session! kg {:participant {:id "p-0007" :public? true}
                                        :session {:id "s1"}
                                        :emotions [{:word "海" :sadness 3 :entry-count 1}]})
          (is (= "Bearer op-jwt" @auth) "ingest carries the operator JWT")
          (let [data (edn/read-string @tx)]   ; tx_edn parsed back to datom maps
            (is (some #(= "s1" (:research.session/id %)) data) "tx_edn contains the session datom")
            (is (some #(= 3 (:research.emotion/sadness %)) data) "tx_edn contains emotion datoms"))
          (is (= [{:research.participant/id "p-0007"
                   :research.participant/age-group "20-29"
                   :research.participant/gender "F"}]
                 (r/kotoba-participants kg))
              "datomic.q rows_edn parsed into participant maps (no PII)"))
        (finally (stop))))))

(deftest kotoba-as-of-time-travel
  (testing "as-of read forwards datomic.q :as_of (Kotoba-native provenance; no Datomic Cloud)"
    (let [{:keys [base qreq stop]} (start-mock-graph! [["\"p-1\"" "\"30-39\"" "\"M\""]])]
      (try
        (let [kg (r/kotoba-graph "research-graph" base "op-jwt")]
          (r/kotoba-participants kg "t-42")
          (is (= "t-42" (:as_of @qreq)) "datomic.q carries the as_of basis-t")
          (r/kotoba-participants kg)
          (is (nil? (:as_of @qreq)) "no as-of → current view (no :as_of)"))
        (finally (stop))))))
