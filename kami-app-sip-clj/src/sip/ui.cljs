(ns sip.ui
  "Playable web HUD for Spirit in Physics (P1 web parity).

  Drives the pure `sip.session` FSM from DOM input: phase-appropriate controls,
  the 心音(kokoro) / 寄り添い(grace) meters, surfaced insights, and the gentle
  outcome screen. There is no fail state — every input helps or holds steady.

  Deliberately dependency-free (plain DOM, no React/reagent) to match the rest
  of the app. Works standalone without WebGPU; when the 3D scene boots it sits
  behind this overlay. `^:export mount` is called from index.html."
  (:require [sip.session :as ses]
            [sip.kotoba :as kotoba]
            [clojure.string :as str]
            [cljs.core.async :refer [<!]])
  (:require-macros [cljs.core.async.macros :refer [go]]))

;; --- state -----------------------------------------------------------------

(defonce ^:private !state   (atom nil))   ; current session map (sip.session)
(defonce ^:private !root    (atom nil))   ; the overlay root element
(defonce ^:private !raf     (atom nil))   ; resonate breathing rAF handle
(defonce ^:private !t0      (atom 0))     ; breathing clock origin
(defonce ^:private !letters (atom nil))   ; loaded 瓶詞 inbox (vector) or nil
(defonce ^:private !lidx    (atom 0))     ; which letter is shown
(defonce ^:private !posted  (atom nil))   ; cid of the 瓶詞 we just floated

;; Story-bible would supply the client + emotion via a session picker; P1 seeds
;; one representative Ghost-space so the loop is playable end-to-end today.
(def ^:private seed-client "tamaki")
(def ^:private seed-emotion {:turbulence 0.6 :loneliness 0.72 :fear 0.5 :hope 0.2})

(def ^:private breath-ms 3200.0)

(def ^:private phase-label
  {:observe "観察 · observe"
   :resonate "共鳴 · resonate"
   :accompany "寄り添い · accompany"
   :name "名づけ · name"
   :complete "完了 · complete"})

(def ^:private how-label
  {:stay "寄り添う" :ask "問いかける" :silence "沈黙する"})

(def ^:private insight-label
  {:togetherness "共にいる" :the-question "問い" :held-space "ひらかれた間"
   :unspoken "語られぬもの" :loneliness "孤独" :fear "おそれ"
   :turbulence "ゆらぎ" :hope "希望"})

(defn- ins-name [k] (or (insight-label k) (name k)))

;; --- tiny DOM helper -------------------------------------------------------

(defn- el [tag props & children]
  (let [e (js/document.createElement (name tag))]
    (doseq [[k v] props]
      (case k
        :text  (set! (.-textContent e) v)
        :class (set! (.-className e) v)
        :style (set! (.. e -style -cssText) v)
        :on    (doseq [[ev f] v] (.addEventListener e (name ev) f))
        (.setAttribute e (name k) (str v))))
    (letfn [(add [c]
              (cond
                (nil? c)        nil
                (sequential? c) (doseq [x c] (add x))   ; flatten (for …) children
                (or (string? c) (number? c)) (.appendChild e (js/document.createTextNode (str c)))
                :else           (.appendChild e c)))]
      (doseq [c children] (add c)))
    e))

(defn- pct [x] (str (js/Math.round (* 100 (double x))) "%"))

(defn- meter [label value color]
  (el :div {:style "margin:6px 0"}
      (el :div {:style "display:flex;justify-content:space-between;font-size:11px;color:#9a8fb5;letter-spacing:.06em"}
          (el :span {:text label}) (el :span {:text (pct value)}))
      (el :div {:style "height:8px;border-radius:6px;background:#e7e0f4;overflow:hidden;margin-top:3px"}
          (el :div {:style (str "height:100%;width:" (pct value)
                                ";background:" color ";transition:width .5s ease")}))))

(defn- btn [label f & [disabled?]]
  (el :button
      {:text label
       :style (str "appearance:none;border:0;border-radius:999px;padding:9px 16px;margin:4px;"
                   "font:inherit;font-size:14px;cursor:" (if disabled? "default" "pointer") ";"
                   "color:#fff;letter-spacing:.04em;transition:transform .1s,opacity .2s;"
                   "background:" (if disabled? "#c9bbe6" "#7d6bb0") ";"
                   "opacity:" (if disabled? "0.55" "1"))
       :on (when-not disabled? {:click (fn [_] (f))})}))

;; --- transitions (each re-renders) -----------------------------------------

(declare render!)

(defn- advance! [f] (swap! !state f) (render!))

(defn- do-resonate! []
  (let [frac (/ (mod (- (js/performance.now) @!t0) breath-ms) breath-ms)
        closeness (js/Math.sin (* js/Math.PI frac))]  ; 0→1→0 over the breath
    (advance! #(ses/resonate % closeness))))

(defn- restart! []
  (reset! !posted nil)
  (reset! !letters nil)
  (reset! !state (ses/begin seed-client seed-emotion))
  (render!))

;; --- 瓶詞 (bottle-letter) actions ------------------------------------------

(defn- float-letter! [textarea]
  (let [t (.-value textarea)]
    (when-not (str/blank? t)
      (go (let [cid (<! (kotoba/post-letter! {:from seed-client :text t :season :spring}))]
            (reset! !posted cid)
            (reset! !letters nil)        ; force re-load so our own bottle is included
            (render!))))))

(defn- load-inbox! []
  (go (let [ls (<! (kotoba/inbox :spring))]
        (reset! !letters (vec ls))
        (reset! !lidx 0)
        (render!))))

(defn- letter-section
  "瓶詞 compose + pick-up, shown on the completion screen — the non-toxic async
  multiplayer: float a one-line letter into the canal, pick up someone else's."
  []
  (let [ta (el :textarea {:placeholder "運河に流す一言…（いつか誰かが拾う）" :maxlength "80"
                          :style (str "font:inherit;font-size:14px;padding:8px 10px;border-radius:12px;"
                                      "border:1px solid #d8cdee;outline:none;width:100%;height:46px;"
                                      "resize:none;color:#6b5b95;box-sizing:border-box")})]
    (el :div {:style "margin-top:16px;border-top:1px solid #ece6f7;padding-top:12px"}
        (el :div {:style "font-size:12px;color:#9a8fb5;letter-spacing:.08em;margin-bottom:6px"
                  :text "瓶詞 — 運河の手紙"})
        ta
        (el :div {:style "margin-top:4px"}
            (btn "流す" #(float-letter! ta))
            (btn "運河の瓶詞を拾う" load-inbox!))
        (when @!posted
          (el :p {:style "font-size:11px;color:#8fa08f;margin-top:6px"
                  :text (str "瓶詞を流した（" (subs (str @!posted) 0 (min 12 (count (str @!posted)))) "…）")}))
        (when (seq @!letters)
          (let [l (nth @!letters (mod @!lidx (count @!letters)))]
            (el :div {:style "margin-top:10px;background:#f3eefb;border-radius:14px;padding:12px 14px"}
                (el :div {:style "font-size:14px;color:#6b5b95;line-height:1.7" :text (:text l)})
                (el :div {:style "display:flex;justify-content:space-between;align-items:center;margin-top:8px"}
                    (el :span {:style "font-size:11px;color:#b3a9c9" :text (str "— " (:from l))})
                    (btn "次の瓶詞" #(do (swap! !lidx inc) (render!))))))))))

;; --- breathing animation (resonate only) -----------------------------------

(defn- stop-raf! []
  (when-let [h @!raf] (js/cancelAnimationFrame h) (reset! !raf nil)))

(defn- start-breath! [dot]
  (stop-raf!)
  (reset! !t0 (js/performance.now))
  (letfn [(tick []
            (let [frac (/ (mod (- (js/performance.now) @!t0) breath-ms) breath-ms)
                  s (+ 1.0 (* 0.7 (js/Math.sin (* js/Math.PI frac))))]
              (set! (.. dot -style -transform) (str "scale(" s ")"))
              (reset! !raf (js/requestAnimationFrame tick))))]
    (tick)))

;; --- per-phase body --------------------------------------------------------

(defn- phase-body [{:keys [phase] :as s}]
  (case phase
    :observe
    (el :div {}
        (el :p {:style "color:#6b5b95;font-size:14px;line-height:1.7;margin-bottom:8px"
                :text "風景はこの人の心。ただ、観る。"}
            )
        (btn "風景を観る" #(advance! ses/observe)))

    :resonate
    (let [dot (el :div {:style (str "width:18px;height:18px;border-radius:50%;background:#c9bbe6;"
                                    "margin:10px auto;will-change:transform")})]
      (start-breath! dot)
      (el :div {}
          (el :p {:style "color:#6b5b95;font-size:14px;line-height:1.7"
                  :text "息を合わせる。膨らんだ瞬間にそっと。"})
          dot
          (btn "呼吸を合わせる" do-resonate!)
          (btn "寄り添いへ進む" #(advance! ses/to-accompany)
               (not (ses/ready-to-accompany? s)))))

    :accompany
    (el :div {}
        (el :p {:style "color:#6b5b95;font-size:14px;line-height:1.7"
                :text "どう、そばにいる？"})
        (el :div {}
            (btn (how-label :stay)    #(advance! (fn [x] (ses/accompany x :stay))))
            (btn (how-label :ask)     #(advance! (fn [x] (ses/accompany x :ask))))
            (btn (how-label :silence) #(advance! (fn [x] (ses/accompany x :silence)))))
        (btn "名前を贈る" #(advance! ses/to-name) (not (ses/ready-to-name? s))))

    :name
    (let [input (el :input {:type "text" :placeholder "おそれに名前を…" :maxlength "24"
                            :style (str "font:inherit;font-size:15px;padding:9px 12px;border-radius:12px;"
                                        "border:1px solid #d8cdee;outline:none;width:60%;color:#6b5b95")})]
      (el :div {}
          (el :p {:style "color:#6b5b95;font-size:14px;line-height:1.7"
                  :text "心の奥のおそれに、名前を。名づけられた怖さは、やわらぐ。"})
          (el :div {:style "margin:8px 0"} input)
          (btn "名づける" #(let [nm (.-value input)]
                            (when-not (str/blank? nm) (advance! (fn [x] (ses/name-core x nm))))))))

    :complete
    (let [{:keys [grace bond insights named]} (ses/outcome s)]
      (el :div {}
          (el :p {:style "color:#6b5b95;font-size:16px;line-height:1.8"
                  :text (str "「" named "」と名づけた。景色が晴れていく。")})
          (el :p {:style "color:#9a8fb5;font-size:13px;margin-top:6px"
                  :text (str "寄り添い grace " (pct grace) " · 絆 bond " bond
                             " · 気づき " (count insights))})
          (btn "もう一度" restart!)
          (letter-section)))

    (el :div {})))

;; --- full render -----------------------------------------------------------

(defn- panel [{:keys [phase kokoro grace insights] :as s}]
  (el :div {:style (str "max-width:560px;margin:0 auto;background:rgba(251,247,255,.92);"
                        "backdrop-filter:blur(8px);border-radius:20px 20px 0 0;"
                        "box-shadow:0 -8px 40px rgba(107,91,149,.18);padding:18px 22px 26px;"
                        "font-family:ui-sans-serif,system-ui,'Hiragino Sans',sans-serif")}
      (el :div {:style "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:6px"}
          (el :div {:style "font-size:13px;color:#7d6bb0;letter-spacing:.1em;font-weight:600"
                    :text (phase-label phase)})
          (el :div {:style "font-size:11px;color:#b3a9c9"
                    :text (str "client · " seed-client)}))
      (meter "心音 kokoro" kokoro "#e58fb0")
      (meter "寄り添い grace" grace "#8fb0e5")
      (el :div {:style "margin-top:12px"} (phase-body s))
      (when (seq insights)
        (el :div {:style "margin-top:14px;display:flex;flex-wrap:wrap;gap:6px"}
            (for [i insights]
              (el :span {:style (str "font-size:11px;color:#7d6bb0;background:#efe9fb;"
                                     "border-radius:999px;padding:3px 10px")
                         :text (ins-name i)}))))))

(defn- render! []
  (stop-raf!)
  (when-let [root @!root]
    (set! (.-innerHTML root) "")
    (.appendChild root (panel @!state))))

;; --- mount -----------------------------------------------------------------

(defn ^:export mount
  "Create the HUD overlay, begin a seeded session, and render. Idempotent.
  The root only occupies the bottom panel area, so the rest of the canvas stays
  interactive for the 3D scene."
  []
  (when-not @!root
    (let [root (el :div {:id "sip-ui"
                         :style "position:fixed;left:0;right:0;bottom:0;z-index:40;padding:0 10px"})]
      (.appendChild js/document.body root)
      (reset! !root root)))
  (kotoba/seed!)        ; float a few example 瓶詞 once, so the canal isn't empty
  (restart!)
  @!root)
