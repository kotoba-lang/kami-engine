(ns sip.researcher
  "P3-A — read-only researcher dashboard (ADR-0022 P3). A privacy-respecting view
  of the n=1000 study: participants (demographics only, no PII) and their session
  emotion summaries (Hume-style 10-axis vectors).

  Data source is pluggable:
    • `window.SIP_KOTOBA_URL` set → query the shared world over Kotoba's Datomic
      XRPC (read). The exact Datalog/schema is the P2 shared-index follow-up
      (see docs/p2-shared-index.md); until that lands this path best-efforts and
      falls back to seed on any error.
    • otherwise → bundled seed data, so the dashboard renders with no server
      (mirrors how the HUD plays without WebGPU).

  READ-ONLY by design: no participant PII (email / medical history) is shown, and
  only `isPublic` participants are listed. Writes/collection are out of scope
  (ADR-0022 P3-B)."
  (:require [clojure.string :as str]
            [cljs.core.async :as a :refer [<!]])
  (:require-macros [cljs.core.async.macros :refer [go]]))

;; --- emotion axes (Hume-style, from researcher EmotionVector) ---------------

(def ^:private emotions
  [[:joy "喜"] [:sadness "哀"] [:anger "怒"] [:fear "恐"] [:surprise "驚"]
   [:disgust "嫌"] [:calm "穏"] [:focus "集"] [:excitement "昂"] [:confusion "惑"]])

(def ^:private emo-color
  {:joy "#e5b84f" :sadness "#6f8fe5" :anger "#e56f6f" :fear "#9a6fe5" :surprise "#5fc8d8"
   :disgust "#7faf6f" :calm "#8fb0e5" :focus "#6b9bd1" :excitement "#e58fb0" :confusion "#b3a9c9"})

;; --- seed (privacy-safe sample so the dashboard renders serverless) ----------

(def ^:private seed
  {:source :seed
   :participants
   [{:id "p-0007" :age-group "20-29" :gender "F" :public? true
     :sessions [{:session-id "s1" :words 142 :emotions {:joy 8 :sadness 31 :calm 12 :fear 19 :focus 7}}
                {:session-id "s2" :words 168 :emotions {:joy 21 :sadness 14 :calm 28 :focus 16 :excitement 9}}]}
    {:id "p-0042" :age-group "30-39" :gender "M" :public? true
     :sessions [{:session-id "s1" :words 96 :emotions {:fear 24 :confusion 18 :sadness 11 :calm 6}}]}
    {:id "p-0108" :age-group "40-49" :gender "X" :public? true
     :sessions [{:session-id "s1" :words 203 :emotions {:calm 34 :joy 17 :focus 22 :surprise 6}}
                {:session-id "s2" :words 151 :emotions {:joy 26 :calm 19 :excitement 14 :focus 11}}]}]})

;; --- kotoba Datomic XRPC source (best-effort; falls back to seed) -----------

(defn- base-url []
  (let [u (when (exists? js/window) (aget js/window "SIP_KOTOBA_URL"))]
    (when (and (string? u) (not (str/blank? u))) u)))

(defn- load-from-kotoba [base]
  ;; The shared-index Datalog (participants/sessions/emotions over datomic.q)
  ;; is the P2 follow-up; until the schema is fixed we only probe reachability
  ;; and otherwise return nil so the caller falls back to seed.
  (let [out (a/chan 1)]
    (-> (js/fetch (str base "/xrpc/com.etzhayyim.apps.kotoba.datomic.q")
                  (clj->js {:method "POST" :headers {"content-type" "application/json"}
                            :body (js/JSON.stringify (clj->js {:query "[:find ?p :where [?p :sip.participant/id]]"}))}))
        (.then (fn [r] (if (.-ok r) (.json r) (throw (js/Error. "q failed")))))
        (.then (fn [_] (a/close! out)))   ; schema TBD → defer to seed for now
        (.catch (fn [_] (a/close! out))))
    out))

(defn- load-data []
  (go (or (when-let [b (base-url)] (<! (load-from-kotoba b)))
          seed)))

;; --- state ------------------------------------------------------------------

(defonce ^:private !data (atom nil))
(defonce ^:private !sel  (atom nil))   ; selected participant id
(defonce ^:private !root (atom nil))

;; --- tiny DOM helper --------------------------------------------------------

(defn- el [tag props & children]
  (let [e (js/document.createElement (name tag))]
    (doseq [[k v] props]
      (case k
        :text  (set! (.-textContent e) v)
        :style (set! (.. e -style -cssText) v)
        :on    (doseq [[ev f] v] (.addEventListener e (name ev) f))
        (.setAttribute e (name k) (str v))))
    (letfn [(add [c] (cond (nil? c) nil
                           (sequential? c) (doseq [x c] (add x))
                           (or (string? c) (number? c)) (.appendChild e (js/document.createTextNode (str c)))
                           :else (.appendChild e c)))]
      (doseq [c children] (add c)))
    e))

;; --- rendering --------------------------------------------------------------

(declare render!)

(defn- emotion-bars [emap]
  (let [total (max 1 (reduce + (vals emap)))]
    (el :div {:style "display:flex;gap:3px;align-items:flex-end;height:46px;margin-top:6px"}
        (for [[k label] emotions]
          (let [v (get emap k 0)
                h (int (* 42 (/ v total)))]
            (el :div {:style "display:flex;flex-direction:column;align-items:center;width:26px"}
                (el :div {:style (str "width:16px;height:" (max 2 h) "px;border-radius:3px 3px 0 0;background:"
                                      (emo-color k "#ccc") ";opacity:" (if (pos? v) "1" "0.25"))})
                (el :div {:style "font-size:10px;color:#9a8fb5;margin-top:2px" :text label})))))))

(defn- sessions-view [p]
  (el :div {:style "margin-top:10px"}
      (el :div {:style "font-size:12px;color:#7d6bb0;font-weight:600;margin-bottom:4px"
                :text (str "participant " (:id p) " · " (:age-group p) " · " (:gender p)
                           " · " (count (:sessions p)) " sessions")})
      (for [s (:sessions p)]
        (el :div {:style "background:#f7f4fc;border-radius:12px;padding:10px 12px;margin:6px 0"}
            (el :div {:style "font-size:12px;color:#9a8fb5"
                      :text (str "session " (:session-id s) " · " (:words s) " words")})
            (emotion-bars (:emotions s))))))

(defn- participant-row [p selected?]
  (el :div {:style (str "display:flex;justify-content:space-between;align-items:center;padding:9px 12px;"
                        "border-radius:12px;cursor:pointer;margin:3px 0;"
                        "background:" (if selected? "#efe9fb" "transparent"))
            :on {:click (fn [_] (reset! !sel (:id p)) (render!))}}
      (el :span {:style "font-size:14px;color:#6b5b95" :text (:id p)})
      (el :span {:style "font-size:12px;color:#b3a9c9"
                 :text (str (:age-group p) " · " (:gender p) " · " (count (:sessions p)) "s")})))

(defn- panel []
  (let [{:keys [source participants]} @!data
        pub (filter :public? participants)
        sel (some #(when (= (:id %) @!sel) %) pub)]
    (el :div {:style (str "max-width:680px;margin:0 auto;padding:22px;font-family:ui-sans-serif,system-ui,"
                          "'Hiragino Sans',sans-serif;color:#6b5b95")}
        (el :div {:style "display:flex;justify-content:space-between;align-items:baseline"}
            (el :div {:style "font-size:20px;font-weight:700" :text "Spirit in Physics — Researcher"})
            (el :div {:style "font-size:11px;color:#b3a9c9"
                      :text (str "n=" (count pub) " · source: " (name source))}))
        (el :div {:style "font-size:12px;color:#9a8fb5;margin:4px 0 14px"
                  :text "read-only · 公開参加者のみ · PII（email/病歴）は非表示"})
        (el :div {:style "display:grid;grid-template-columns:1fr 1.4fr;gap:18px"}
            (el :div {}
                (el :div {:style "font-size:12px;color:#7d6bb0;font-weight:600;margin-bottom:4px" :text "participants"})
                (for [p pub] (participant-row p (= (:id p) @!sel))))
            (el :div {}
                (if sel (sessions-view sel)
                    (el :div {:style "font-size:13px;color:#b3a9c9;padding-top:8px"
                              :text "← 参加者を選ぶと session の感情サマリを表示"})))))))

(defn- render! []
  (when-let [root @!root]
    (set! (.-innerHTML root) "")
    (.appendChild root (panel))))

(defn ^:export mount []
  (when-not @!root
    (let [root (el :div {:id "sip-researcher" :style "min-height:100vh;background:#faf8fd"})]
      (.appendChild js/document.body root)
      (reset! !root root)))
  (go (reset! !data (<! (load-data)))
      (reset! !sel (:id (first (filter :public? (:participants @!data)))))
      (render!))
  @!root)
