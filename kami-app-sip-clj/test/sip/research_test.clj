(ns sip.research-test
  "P3-B foundation tests — ingest a research session and read it back (datalevin).
  Run with `clojure -M:datomic:test`."
  (:require [clojure.test :refer [deftest is testing]]
            [sip.research :as r]))

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
