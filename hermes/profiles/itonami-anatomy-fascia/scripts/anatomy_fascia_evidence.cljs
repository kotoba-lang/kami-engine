;; itonami-anatomy-fascia evidence.cljs - no_agent measurement script.
;; Decision/calculation live in this script. The agent only reads output JSON
;; and reports 1 finding per tick. No credentials read.
;; Run: nbb scripts/anatomy_fascia_evidence.cljs
;;
;; 3 measurements:
;;   1. app-hyakka current shoseki frontier (:qids) measured from origin/main
;;   2. fascia/anatomy book candidate QID upchecked via Special:EntityData:
;;      P31 in wikidata-book book-classes (Q7725634 Q571 Q47461344 Q3331189
;;      Q8261 Q49084 Q1279564 Q25379) AND disjoint from frontier, 1 entry
;;   3. kami-engine body/fascia sim backend vocabulary measured
;;
;; Candidates are verified-existing QID seeds, not hardcoded guesses
;; (a wrong QID answers 200). 429 respected: upchecks per tick =
;; length of candidate-qids.
;;
;; Output: ~/.hermes/profiles/itonami-anatomy-fascia/workspace/findings/*.json
;;        plus stdout (injected into agent prompt)

(require '[kotoba.lang.text :as str]
         '["node:child_process" :as cp]
         '["node:fs" :as fs]
         '["node:path" :as path]
         '["node:os" :as os])

(def profile-dir (path/join (os/homedir) ".hermes" "profiles" "itonami-anatomy-fascia"))
(def find-dir (path/join profile-dir "workspace" "findings"))
(def hyakka-wt "~/.gftd/worktrees/itonami-anatomy-fascia-shoseki")
(def kami-root "~/github/com-junkawasaki/orgs/kotoba-lang/kami-engine")

(defn sh [cmd cwd]
  ;; maxBuffer: the default execSync buffer is 1MB but app-hyakka's
  ;; config/knowledge-ingest.edn is ~1.1MB, so `git show origin/main:...`
  ;; failed with spawnSync ENOBUFS; the catch folded that into an "ERR:"
  ;; string whose regex matched 0 QIDs, reporting an empty frontier and a
  ;; false ready-to-propose for QIDs already on main (the 2026-09-23
  ;; duplicate wikidata-books-6 entry came from exactly this).
  (try (str/trim (str (cp/execSync cmd #js {:cwd cwd :timeout 30000 :maxBuffer 10485760})))
       (catch :default e (str "ERR:" (.-message e)))))

(defn curl [url]
  (let [cmd (str "curl -s -m 25 -H 'User-Agent: itonami-anatomy-fascia/0.1' " url)]
    (try (str (cp/execSync cmd #js {:timeout 30000 :encoding "utf8" :maxBuffer 10485760}))
         (catch :default e (str "ERR:" (.-message e))))))

;; ---- 1. app-hyakka 現在の shoseki frontier + kami 語彙 ----

(defn current-shoseki-qids []
  (sh "git fetch origin" hyakka-wt)
  (let [cfg (sh "git show origin/main:config/knowledge-ingest.edn" hyakka-wt)]
    (->> (re-seq #"\"(Q[0-9]+)\"" cfg) (map second) (set) (vec) (sort))))

(defn kami-fascia-vocab []
  (let [read (fn [p] (try (fs/readFileSync p "utf8") (catch :default _ "")))
        claude (str/lower (read (path/join kami-root "CLAUDE.md")))
        adapter (str/lower (read (path/join kami-root "docs/adapter-registry.edn")))
        terms ["fascia" "myofascial" "muscle" "skeleton" "rigid-body" "soft-body"
               "spring" "cloth" "finite-element" "cae" "xpbd" "tension" "vrm"
               "spring-bone" "kinematics" "mesh-deform"]]
    (->> terms
         (map (fn [t] [t {:in-claude (boolean (str/includes? claude t))
                          :in-adapter (boolean (str/includes? adapter t))}]))
         (into {}))))

;; wikidata-book の book-classes (src/hyakka/wikidata_book.cljc) と同期させる。
(def book-classes
  #{"Q7725634" "Q571" "Q47461344" "Q3331189" "Q8261" "Q49084" "Q1279564" "Q25379"})

;; ballot-stuffing 防止: 候補は既に実在を確認した QID の seed だけ。当て推量を
;; 入れない。search での自動発見は 429 を避けるため実装しない (frontier は
;; 手で足した verified QID の列、と corpus 自身が coverage note で言っている)。
(def candidate-qids
  [["Q26971972" "Anatomy Trains (Myers)"]
   ["Q129350773" "Anatomy Trains paperback edition (Myers)"]])

(defn upcheck-book [qid]
  (let [raw (curl (str "https://www.wikidata.org/wiki/Special:EntityData/" qid ".json"))]
    (if (str/starts-with? raw "ERR")
      {:qid qid :error raw}
      (try
        (let [obj (js->clj (js/JSON.parse raw) :keywordize-keys false)
              e (get-in obj ["entities" qid])]
          (if (nil? e)
            {:qid qid :error "entity-missing"}
            (let [p31-ids (->> (get-in e ["claims" "P31"] #js [])
                               (keep #(get-in % ["mainsnak" "datavalue" "value" "id"]))
                               (filter #(re-matches #"Q\d+" %)))]
              {:qid qid
               :is-book (boolean (some book-classes p31-ids))
               :p31-ids (vec p31-ids)})))
        (catch :default e {:qid qid :error (str "parse:" (.-message e))})))))

(defn main []
  (let [ts (.toISOString (js/Date.))
        frontier (current-shoseki-qids)
        checked (mapv (fn [[qid label]]
                        (assoc (upcheck-book qid)
                               :label label
                               :already-in-frontier (some #(= % qid) frontier)))
                      candidate-qids)
        eligible (filter (fn [c]
                           (and (:is-book c)
                                (not (:already-in-frontier c))
                                (not (:error c))))
                         checked)
        chosen (:qid (first eligible))
        result {:at ts
                :kind "itonami-anatomy-fascia-evidence"
                :frontier (vec frontier)
                :frontier-count (count frontier)
                :candidates-checked checked
                :eligible (mapv :qid eligible)
                :eligible-labels (mapv :label eligible)
                :chosen-qid chosen
                :status (cond
                          (empty? checked) :no-candidates-checked
                          (seq eligible) :ready-to-propose
                          (some #(str/starts-with? (str (or (:error %) "")) "parse") checked)
                          :some-parse-error
                          :else :no-eligible-new)}
        outp (path/join find-dir (str "anatomy-fascia-" (.slice ts 0 10) ".json"))]
    (fs/mkdirSync find-dir #js {:recursive true})
    (fs/writeFileSync outp (js/JSON.stringify (clj->js result) nil 2))
    (println (js/JSON.stringify (clj->js result) nil 2))))

(main)
