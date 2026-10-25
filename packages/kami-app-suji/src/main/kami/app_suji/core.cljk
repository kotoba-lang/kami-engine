(ns kami.app-suji.core
  "kami-app-suji — the views, as pure functions of state.

  Every function here takes a state map and returns hiccup. No atom, no re-frame,
  no `js/`: the mount lives in `ui.cljs` and this namespace can be rendered and
  asserted on the JVM, which is how the views are tested without a browser.

  NON-DIAGNOSTIC (G1, 医師法 §17). Every figure shown is mechanical — a moment in
  N·m, a force in N, a fraction of maximum voluntary contraction, a compressive
  load in N or kgf. There is no field in this app that can hold a diagnosis, a
  condition or a recommendation to treat, and adding one would be a charter
  violation in `suji` before it was a UI change here. SELF-REFERENCED (G3): the
  comparison is one body across setups, never one person against another."
  (:require [kotoba.lang.text :as str]
            [jp-go-dds.core :as dds]
            [kami.app-suji.coverage :as coverage]
            [kami.app-suji.route :as route]
            [kami.app-suji.scene :as scene]
            [suji.methods.math :as math]
            [suji.methods.muscle :as muscle]
            [suji.methods.pose :as pose]
            [suji.methods.posture :as posture]
            [suji.methods.spine :as spine]
            [suji.methods.segment :as segment]
            [suji.methods.attachment :as attachment]
            [suji.methods.strain :as strain]))

;; --- state -------------------------------------------------------------------

(def controls
  "Every slider, as data — label, path, range and unit. The control panel is
  generated from this, so a control cannot be added to the state and forgotten in
  the UI, and the ranges live next to the thing they bound."
  [{:path [:body :total-mass-kg] :label "体重" :unit "kg" :min 35 :max 130 :step 1}
   {:path [:body :stature-m] :label "身長" :unit "m" :min 1.35 :max 2.05 :step 0.01}
   {:path [:posture :head-flexion-deg] :label "頭部前屈" :unit "°" :min 0 :max 60 :step 1}
   {:path [:posture :trunk-flexion-deg] :label "体幹前傾" :unit "°" :min 0 :max 60 :step 1}
   ;; THE PELVIS ROTATES, and until suji 3d494ba it could not. Anterior positive.
   ;; The range is not a taste: it spans the postures `posture/lumbar-lordosis` has
   ;; measurements for, which run from cross-legged (-7.4 deg of lordosis, so -8.0
   ;; of tilt from this model's straight neutral) to standing (+46.5), plus a
   ;; little either side. `the-pelvic-tilt-slider-reaches-every-measured-posture`
   ;; asserts that from the table rather than from this comment, so an entry added
   ;; to Cho's table outside this span fails instead of being quietly unreachable.
   {:path [:posture :pelvic-tilt-deg] :label "骨盤前傾（前弯）" :unit "°" :min -10 :max 50 :step 1}
   {:path [:posture :shoulder-flexion-deg] :label "肩屈曲" :unit "°" :min 0 :max 90 :step 1}
   {:path [:posture :elbow-flexion-deg] :label "肘屈曲" :unit "°" :min 0 :max 140 :step 1}
   {:path [:posture :wrist-extension-deg] :label "手首伸展" :unit "°" :min 0 :max 45 :step 1}
   {:path [:posture :shoulder-abduction-deg] :label "肩外転（前額面）" :unit "°" :min 0 :max 90 :step 1}
   {:path [:posture :trunk-lateral-bend-deg] :label "体幹側屈（前額面）" :unit "°" :min -40 :max 40 :step 1}
   {:path [:posture :head-rotation-deg] :label "頭部回旋" :unit "°" :min -70 :max 70 :step 1}
   {:path [:posture :hip-flexion-deg] :label "股関節屈曲" :unit "°" :min 0 :max 120 :step 1}
   {:path [:posture :knee-flexion-deg] :label "膝屈曲" :unit "°" :min 0 :max 130 :step 1}
   {:path [:posture :ankle-dorsiflexion-deg] :label "足関節背屈" :unit "°" :min -20 :max 30 :step 1}
   {:path [:session-minutes] :label "連続作業時間" :unit "分" :min 10 :max 480 :step 10}])

(def optional-angle-defaults
  "Every angle this app has a slider for that a reference posture may not state.

  `pose` treats them all as optional and defaults them to zero; the app has
  sliders for them, and a slider needs a value — so the defaults are made explicit
  HERE rather than left to `get-in` returning nil, which is how a missing control
  becomes a NullPointerException at render time instead of a zero.

  ⚠ IT WAS CALLED `out-of-plane-defaults`, AND `:pelvic-tilt-deg` IS NOT OUT OF
  PLANE. It is sagittal, and it is missing from the reference postures for a
  different reason: the frontal-plane angles are absent because
  `posture-from-workstation` describes a sagittal setup, and the pelvic tilt is
  absent because suji's reference postures have not been given one yet. Both need
  a default here; only the first is a modelling boundary, and `preset-lordosis`
  is where the second is stated rather than papered over."
  {:shoulder-abduction-deg 0.0
   :trunk-lateral-bend-deg 0.0
   :head-rotation-deg 0.0
   :pelvic-tilt-deg 0.0})

(defn workstation-posture
  "A reference workstation's posture, with every control this app exposes present."
  [w]
  (merge optional-angle-defaults
         (posture/posture-from-workstation w)
         {:preset (:name w)}))

(def presets
  "Every posture the panel offers, as one table.

  Two GROUPS, and the grouping is the point rather than tidiness. A workstation is
  a description of furniture that `posture/posture-from-workstation` turns into
  angles, and it is always `:seated`. A standing reference posture is already a
  posture and carries `:support :standing`.

  That difference is the largest single fact in the lower-limb model. Seated, the
  chair takes the trunk through the ischial tuberosities and the hip, knee and
  ankle carry only what is distal to them. Standing, the whole body's weight
  reaches the floor through both legs. Measured in `suji`: 333 N through a standing
  ankle against 10 N through a seated one, from the same joint angles. So the
  panel must never let a reader move a knee slider without being able to see which
  regime the answer came from."
  (concat
   (for [w posture/reference-workstations]
     {:name (:name w) :group :seated :posture (workstation-posture w)})
   (for [p posture/reference-standing-postures]
     {:name (:name p) :group :standing
      :posture (merge optional-angle-defaults p {:preset (:name p)})})))

(defn preset-posture
  "The posture for a preset name, or nil. Pure, so the button table and the click
  handler cannot disagree about what a preset means."
  [preset-name]
  (some #(when (= preset-name (:name %)) (:posture %)) presets))

(defn pelvic-tilt-range
  "The span of the pelvic-tilt slider, read off `controls` rather than restated.

  The claim below about which measured postures this page can reach is only true
  of the slider that is actually rendered, so it is derived from the same table
  the panel is generated from."
  []
  (let [c (first (filter #(= [:posture :pelvic-tilt-deg] (:path %)) controls))]
    [(double (:min c)) (double (:max c))]))

(defn measured-lordosis
  "Every lumbar lordosis `suji` has a measurement for, with what this page can do
  about it.

  DERIVED ON BOTH SIDES, which is the whole reason it is a function. The
  measurements are `posture/lumbar-lordosis` — Cho et al. 2015, suji's table and
  not a copy of it — and `:preset-carries` / `:slider-reaches` are computed from
  this app's own presets and its own slider bounds. The day suji gives a reference
  posture a lordosis, or the day somebody narrows the slider, this answers
  differently without a sentence being edited."
  ([] (measured-lordosis presets (pelvic-tilt-range)))
  ([ps [lo hi]]
   (let [carried (into (sorted-set)
                       (map #(pose/lumbar-lordosis-deg (:posture %))) ps)]
     (vec (for [[k v] (sort-by #(- (:deg (val %)))
                               (filter #(and (map? (val %)) (number? (:deg (val %))))
                                       posture/lumbar-lordosis))]
            {:posture k
             :deg (:deg v)
             :sd (:sd v)
             ;; inside the entry's OWN scatter, not inside a tolerance chosen here
             :preset-carries (vec (filter #(<= (math/abs* (- % (:deg v))) (:sd v)) carried))
             :slider-reaches (<= lo (posture/pelvic-tilt-for k) hi)})))))

(def lordosis-note-id
  "The label the panel and the method page both hang the preset-lordosis statement
  on. One string, so the coverage probe and the tests look for the same text the
  page prints."
  "参照姿勢の前弯")

(defn preset-lordosis
  "What lordosis the presets on this panel actually carry, and the gap that leaves.

  ⚠ NOTHING IN THE SENTENCE THIS FEEDS IS TYPED, and that is the point rather than
  a style. `suji`'s reference postures — the three workstations and the three
  standing postures — state no pelvic tilt, so every preset on this panel is
  solved at ZERO lordosis, including the standing ones, where Cho measures 47.1
  deg. This app must not fill that in: the tilt that would be right is a parameter
  the reference postures' own sources do not state, and choosing one here would
  move every lumbar number on the page on this app's authority.

  So the page says what is true instead, from numbers it computed. Give
  `reference-standing-postures` a `:pelvic-tilt-deg` upstream and
  `:standing-carries` stops being `[0.0]`, the gap closes, and the sentence
  changes by itself."
  ([] (preset-lordosis presets))
  ([ps]
   (let [lord (fn [xs] (into (sorted-set) (map #(pose/lumbar-lordosis-deg (:posture %))) xs))
         standing (filter #(= :standing (:group %)) ps)
         seated (filter #(= :seated (:group %)) ps)]
     {:carries (vec (lord ps))
      :standing-carries (vec (lord standing))
      :seated-carries (vec (lord seated))
      :standing-measured (:standing posture/lumbar-lordosis)
      :stool-measured (:stool posture/lumbar-lordosis)
      :standing-gap-deg (posture/pelvic-tilt-for :standing)
      :citation (:citation posture/lumbar-lordosis)
      :url (:url posture/lumbar-lordosis)
      ;; true only while every preset is at the model's straight neutral
      :every-preset-straight? (= [0.0] (vec (lord ps)))})))

(defn preset-lordosis-note
  "The preset-lordosis statement as hiccup. Both branches are written, because
  `every preset is straight` is a fact about today and not a property of the app —
  and a sentence that can only say one thing cannot be caught saying the wrong
  one."
  ([] (preset-lordosis-note (preset-lordosis)))
  ([{:keys [standing-carries standing-measured stool-measured standing-gap-deg
            every-preset-straight? carries citation url]}]
   (let [deg #(str (math/fmt-fixed % 1) "°")]
     [:p {:class "suji-note"}
      [:strong lordosis-note-id] " —— "
      (if every-preset-straight?
        [:span
         "このパネルのプリセットは" [:strong "すべて前弯 " (deg (first carries))]
         "（腰椎がまっすぐ）で解いている。"
         "立位プリセットも " [:strong (deg (first standing-carries))] " である。"]
        [:span
         "プリセットが持つ前弯は " [:strong (str/join "・" (map deg carries))]
         "（立位は " (str/join "・" (map deg standing-carries)) "）。"])
      "Cho らの実測では、丸椅子が " [:strong (deg (:deg stool-measured))]
      "（SD " (math/fmt-fixed (:sd stool-measured) 1) "）、"
      "立位が " [:strong (deg (:deg standing-measured))]
      "（SD " (math/fmt-fixed (:sd standing-measured) 1) "）で、"
      "この模型のまっすぐな中立からの差は " [:strong (deg standing-gap-deg)] "。"
      (when every-preset-straight?
        (str "つまり立位プリセットの腰椎は、立位ではなく丸椅子の前弯で計算されている —— "
             "これは上流（suji の参照姿勢が骨盤傾斜を持たない）の未対応であって、"
             "この面で埋めていない。埋めれば数字は動くが、その値の根拠はこの app に無い。"))
      "骨盤前傾のスライダーはこの差を手で入れるためにある。"
      "（" citation " " [:a {:href url} "本文"] "）"])))

(def initial-state
  (merge {:view :simulate
          :body {:total-mass-kg 70.0 :stature-m 1.70}
          :session-minutes 120.0
          :backend nil}
         {:posture (workstation-posture posture/laptop-on-lap)}))

(defn body-of [state]
  (segment/build-body (get-in state [:body :total-mass-kg])
                      (get-in state [:body :stature-m])))

(defn solved
  "The whole answer for the current state: loads, muscle tensions, session strain
  and the render scene. Pure — this is what the browser recomputes on every slider
  move, and what a test can call directly."
  [state]
  (let [body (body-of state)
        {:keys [loads tensions scene]} (scene/solve-and-scene body (:posture state))]
    {:body body
     :loads loads
     :tensions tensions
     :strains (strain/session-strain tensions (:session-minutes state))
     :scene scene}))

;; --- small view helpers ------------------------------------------------------

(defn coeff-label
  "A task coefficient with its unit. `recruit` returns a moment arm in metres for a
  moment equilibrium and a DIMENSIONLESS direction cosine for a suspended force —
  printing both as millimetres would put a cosine of 0.41 on the page as 410 mm.

  The task is read off the tension entry, which carries it. It used to be looked up
  as `(:task (get suji.methods.attachment/muscles (:name t)))`, and that map is keyed by
  GROUP (`upper_trapezius`) while a tension entry is named by INSTANCE
  (`upper_trapezius/left`) — every suspension muscle is `:paired?`, so the lookup
  returned nil for all of them and the guard was dead. The page shipped six rows of
  `343.3 mm` / `480.9 mm` / `176.9 mm` at the DEFAULT posture: a 34 cm moment arm at
  the shoulder, on the muscles this app's own prose calls the 肩こり muscles. The
  guard was written to prevent exactly that and could not fire once."
  [t]
  (let [c (:coeff t)
        suspension? (= :scapular-suspension (:task t))]
    (cond
      (nil? c) "—"
      suspension? (str (math/fmt-fixed c 2) " (cos)")
      :else (str (math/fmt-fixed (* 1000.0 c) 1) " mm"))))

(defn- rgb-css
  "A GPU colour rendered for a swatch. This is the one place a scene colour
  reaches CSS, and it is generated from the scene's own numbers rather than
  restated as a literal — a legend that can disagree with the picture is worse
  than no legend."
  [[r g b]]
  (str "rgb(" (int (* 255 r)) " " (int (* 255 g)) " " (int (* 255 b)) ")"))

(defn- figure [value unit label]
  [:div {:class "suji-readout-item"}
   [:div {:class "suji-figure"} value [:span {:class "suji-unit"} " " unit]]
   [:div {:class "suji-unit"} label]])

(def endurance-position-label
  "`strain/endurance-position` as a phrase, because the keyword is the whole
  caveat and this is the only place a reader can be told it.

  The dose layer is a POWER LAW with a floor bolted on and no published fit that
  matches its coefficients, and it says so about each row: below 8 %MVC it returns
  infinity by construction, between 8 and 10 it returns a number the reference has
  no data under, and above 100 it prices a holding time for a load the muscle
  cannot hold. A band alone — which is all this app showed until 2026-09-08 —
  cannot distinguish those from an answer inside the fitted range."
  {:below-endurance-floor "モデルの床より下（∞ は構造上の値）"
   :below-fitted-range "文献の当てはめ範囲より下"
   :within-fitted-range "当てはめ範囲内"
   :above-maximum-voluntary-contraction "最大随意収縮より上（保持時間は定義されない）"})

(defn endurance-label
  "How long this muscle can hold this load, with what qualifies the number.

  `∞` is not a missing value — below the model's floor the acute term is zero by
  construction — so it is printed as itself rather than as a dash, which would be
  the same character the refused rows use for `no answer`."
  [st]
  (let [m (:endurance-minutes st)
        ;; the caveat goes on the ∞ TOO. `:extrapolated?` is true below the
        ;; endurance floor — the infinity is `the model returns no acute failure
        ;; point here`, a modelling choice, and not a measurement that a muscle can
        ;; be held indefinitely. Marking only the finite numbers would put the one
        ;; value in the column that most invites relief on the page unqualified,
        ;; which is where the coverage probe found it missing.
        caveat (when (:endurance-extrapolated? st) "（外挿）")]
    (cond
      (nil? m) "—"
      (not (math/finite? m)) (str "∞" caveat)
      :else (str (math/fmt-fixed m 1) " 分" caveat))))

(defn t12l1-solved?
  "Does the model write an equilibrium at the joint the trunk split created?

  suji 3d494ba placed `:t12l1` and gave the thorax a frame of its own, and wrote
  no equilibrium there — so the thorax hangs from a joint no muscle acts about.
  That is a fact about today's model and not a property of it, so BOTH places the
  page states it read it off the joint-moment table the page itself prints, rather
  than asserting it. The day an equilibrium is written at T12/L1, the sentences
  flip on their own.

  `method-view`'s docstring lists five false claims this page has shipped, every
  one of them a sentence about the model that was true when it was typed. This is
  the shape of the fix."
  [loads]
  (boolean (some #(= "t12l1" (:joint %)) (:joints loads))))

(defn joint-name
  "A joint key as the page writes it. `(name :hip/left)` is `left`, which loses
  the joint; the qualifier is half the identity in a bilateral model, so the whole
  keyword is printed minus its colon."
  [j]
  (if (keyword? j) (subs (str j) 1) (str j)))

(defn arms-label
  "A muscle's moment arms about every joint its solve constrained, in millimetres.

  ONE ROW OF THE CONSTRAINT MATRIX, which is what `:coeffs` is. The muscle table's
  `モーメントアーム` column shows only the arm about the muscle's own joint, so a
  two-joint muscle's second column of the matrix — the reason the coupled solve
  exists — had nowhere to appear."
  [t]
  (if-let [cs (seq (sort-by (comp str key) (:coeffs t)))]
    (str/join " / " (for [[j c] cs]
                      (str (joint-name j) " " (math/fmt-fixed (* 1000.0 c) 1))))
    "—"))

;; --- views -------------------------------------------------------------------

(defn solve-card
  "What the solve says about itself.

  WHY THIS EXISTS AT ALL. `suji` moved to `recruit/solve` on 2026-09-08 — one
  optimisation over several equilibria instead of one per joint — and everything
  it says about its own answer arrived with it: which muscles were solved together,
  whether the Newton iteration converged, the equilibrium error left at each joint,
  the price that decides which muscles are switched off, and which second joints
  are still fed by nothing. A coverage census of this app measured every one of
  those as computed and not shown. The forces changed a great deal at the same
  moment — 22.1 to 39.2 %MVC at the cervical extensors — and the page had no way
  to say what had changed or whether the new numbers satisfied anything.

  THE RESIDUAL IS THE POINT OF IT. An unconverged group refuses outright, so every
  number on this page comes from a solve that claims to have satisfied its
  equilibria; `:coupled-residual-nm` is that claim in newton-metres, and printing
  it is the difference between a reader who can check and one who has to believe.
  It is reported in picoNewton-metres because it is zero to floating point — at
  1e-11 N·m every fixed-decimal rendering in newton-metres is `0.00`, and a column
  of zeros says `not measured` as loudly as it says `balanced`."
  [tensions loads]
  (let [s (muscle/tension-summary tensions loads)
        coupled (filterv :coupled-group tensions)
        groups (sort-by (comp str first)
                        (group-by (juxt :coupled-group :coupled-joints) coupled))
        two-joint (filterv :crosses-joint tensions)
        residual (or (:coupled-residual-nm s) {})
        unfed (or (:two-joint-unfed-nm s) {})
        joints (sort-by str (into (set (keys residual)) (keys unfed)))]
    (dds/card
     (dds/heading 3 "連立解 —— 解が自分について言っていること")
     [:p {:class "suji-note"}
      "2026-09-08 まで、この模型は関節を 1 つずつ解いていた。2 関節筋は片方の"
      "平衡で解かれ、もう片方に及ぼすモーメントは" [:strong "報告されるだけ"]
      "だった。いまは首の 2 関節と片脚の 3 関節がそれぞれ 1 つの最適化として同時に"
      "解かれる。下の残差はその平衡が実際に満たされているかで、"
      [:strong "読み手が確かめられる形にするためにここに出す"] "。"]
     [:div {:class "dds-ext-row"}
      (figure (str (:total s)) "" "筋・靭帯の行数")
      (figure (str (:refused s)) "" "拒否（計算していない）")
      (figure (str (:antagonists s)) "" "拮抗（反対側が担っている）")
      (figure (str (:inactive s)) "" "無活動（最適解が切った）")
      (figure (str (:over-mvc s)) "" "最大随意収縮を超過")
      (figure (str (:coupled-not-converged s)) "" "収束しなかった行")
      (figure (if (:complete? s) "すべて配置" "一部未配置") "" "荷重の配置")]
     (dds/table
      {:headers ["群" "同時に満たした関節" "収束" "構成する筋" "うち無活動"]
       :rows (mapv (fn [[[gid gjoints] members]]
                     [(name gid)
                      (str/join " · " (map joint-name gjoints))
                      (if (every? :coupled-converged? members) "収束した" "収束せず")
                      (str (count members))
                      (str (count (filter :inactive? members)))])
                   groups)})
     [:p {:class "suji-note"}
      "残差は Σ(モーメントアーム × 力) − 荷重 を、その関節について、"
      "元の荷重に対して受動張力込みで取ったもの。単位は "
      [:strong "pN·m（10⁻¹² N·m）"] " —— N·m で小数を並べると全部 0.00 になり、"
      "「釣り合っている」と「測っていない」が同じ顔をするため。"
      "「どの式にも入っていない」列は、その関節を跨ぐ筋が及ぼしているのに"
      "この模型がどの平衡にも渡していないモーメントで、"
      [:strong "連立で消えた分ではなく、まだ残っている分"] "である。"]
     (dds/table
      {:headers ["関節" "連立の残差" "どの式にも入っていないモーメント"]
       :rows (mapv (fn [j]
                     [(joint-name j)
                      (if (contains? residual j)
                        (str (math/fmt-fixed (* 1.0e12 (get residual j)) 3) " pN·m")
                        "—")
                      (if (contains? unfed j)
                        (str (math/fmt-fixed (get unfed j) 3) " N·m")
                        "—")])
                   joints)})
     (when (seq two-joint)
       [:div
        [:p {:class "suji-note"}
         "2 関節筋の「もう一方の関節」。"
         [:strong "連立で解いた"] "ものは、その関節の平衡がこの筋の力を知っている。"
         [:strong "解いていない"] "ものは知らない —— そのモーメントは上の表の右列に"
         "積まれている。c2c3 に何も無いのは分節化の不足ではなく出典の不足である。"]
        (dds/table
         {:headers ["筋" "もう一方の関節" "モーメントアーム" "そこで出しているモーメント" "扱い"]
          :rows (mapv (fn [t]
                        [(str/replace (:name t) "_" " ")
                         (joint-name (:crosses-joint t))
                         (str (math/fmt-fixed (* 1000.0 (:secondary-arm-m t)) 1) " mm")
                         (str (math/fmt-fixed (or (:secondary-moment-nm t) 0.0) 3) " N·m")
                         (if (:secondary-fed? t) "連立で解いた" "解いていない")])
                      two-joint)})])
     (when (seq coupled)
       [:div
        [:p {:class "suji-note"}
         "各筋が連立のどこに入ったか。"
         [:strong "価格"] "（KKT 乗数と自分の係数の内積、10⁻⁶ 単位）が負の筋は"
         "最適解が切る —— 動かしてもどの関節の役にも立たず、コストだけ上がるため。"
         "これは「計算できなかった」ではなく「計算した結果 0」である。"
         "「自関節の荷重」はこの筋の関節に与えられた元のモーメント。"]
        (dds/table
         {:headers ["筋" "群" "自関節の荷重" "関節ごとのモーメントアーム (mm)" "価格 ×10⁻⁶" "状態"]
          :rows (mapv (fn [t]
                        [(str/replace (:name t) "_" " ")
                         (name (:coupled-group t))
                         (str (math/fmt-fixed (or (:task-load-nm t) 0.0) 3) " N·m")
                         (arms-label t)
                         (if (number? (:price t))
                           (math/fmt-fixed (* 1.0e6 (:price t)) 3)
                           "—")
                         (if (:inactive? t) "無活動" "活動")])
                      coupled)})]))))

(defn simulate-view [state]
  (let [{:keys [loads tensions strains scene]} (solved state)
        cerv (:cervical loads)
        sup (:support loads)]
    [:div {:class "suji-main"}
     [:div
      [:div {:class "suji-stage"}
       [:canvas {:id "suji-canvas" :aria-label "姿勢の 3D 表示"}]
       [:div {:class "suji-backend"} (or (some-> (:backend state) name) "…")]]
      [:p {:class "suji-note"}
       "色は各分節がぶら下がる関節の %MVC（最大随意収縮に対する割合）。"
       "力学量であって所見ではない。"
       [:strong "下の表の「帯」とは別の量である"]
       " —— 絵は" [:strong "その瞬間の"] "負荷を、表は作業時間ぶん積み上げた"
       [:strong "ドーズ"] "を帯分けする。120 分保持なら 9 %MVC の筋は"
       "「瞬間は低く、ドーズは高い」であり、両方とも正しい。"]
      (into [:div {:class "dds-ext-row"}]
            (concat
             (for [{:keys [band rgb max-mvc-pct]} (:bands (:legend scene))]
               [:span {:class "suji-note"}
                [:span {:class "suji-swatch" :style {:background (rgb-css rgb)}}]
                band
                (when (math/finite? max-mvc-pct)
                  (str " <" (math/fmt-fixed max-mvc-pct 0) "%"))])
             ;; Every non-load colour belongs in the key: a reader who sees a
             ;; colour with no entry for it has to guess, and both guesses (fine /
             ;; terrible) are claims the model did not make.
             ;;
             ;; ⚠ THE ANTAGONIST COLOUR WAS MISSING, and it is not rare. It is
             ;; used on MUSCLE lines rather than on bones, which is why it escaped
             ;; a key written while looking at the segments: measured over 108
             ;; postures, 1,308 muscle draws are drawn in it. A reader saw a
             ;; distinctly dimmed muscle constantly and had nothing to read it by.
             ;;
             ;; `no-load-colours` is derived so the next colour added to
             ;; `scene/muscle-draws` or `scene/bone-draws` cannot arrive without an
             ;; entry — `every-colour-a-draw-can-take-is-in-the-key` fails if it
             ;; does.
             (for [[rgb label] scene/no-load-colours]
               [:span {:class "suji-note"}
                [:span {:class "suji-swatch" :style {:background (rgb-css rgb)}}]
                label])))
      (let [f (:frontal loads)
            fl (:lumbosacral-nm f 0.0)
            fs (get-in f [:shoulder-per-side :left] 0.0)]
        ;; SIGNED, though the threshold that decides whether to say anything is
        ;; not. The magnitude answers `how much` and the sign answers `which way`,
        ;; and this line printed `(abs …)` until 2026-09-08 — so a trunk bent 35°
        ;; left and one bent 35° right put the identical sentence on the page. The
        ;; coverage probe is what found it: it looks for the model's own value and
        ;; the model's own value was −67.24 N·m where the page said 67.24.
        (when (or (> (math/abs* fl) 0.5) (> (math/abs* fs) 0.5))
          [:p {:class "suji-note"}
           "前額面のモーメント —— L5/S1 " [:strong (str (math/fmt-fixed fl 2) " N·m")]
           "、肩（左）" [:strong (str (math/fmt-fixed fs 2) " N·m")]
           "（符号は向き —— 正負が左右の別である）"
           "。腰方形筋・腹斜筋・中部三角筋・広背筋・斜角筋が担う。"
           "これらの筋は 2026-09-06 に足したもので、それ以前この荷重は"
           "「担う筋がモデルに無い」として報告されていた。"]))]
     [:div {:class "suji-readout"}
      (dds/card
       (dds/heading 3 "頸椎にかかる圧縮荷重")
       [:div {:class "dds-ext-row"}
        (figure (math/fmt-fixed (:compressive-load-kgf cerv) 1) "kgf" "圧縮荷重")
        (figure (math/fmt-fixed (:multiplier-vs-head cerv) 1) "×" "頭部重量比")
        (figure (math/fmt-fixed (:compressive-load-n cerv) 0) "N" "同 (N)")])
      (dds/card
       (dds/heading 3 "身体を支えているもの")
       [:p {:class "suji-note"}
        "下肢の関節が何を持つかは姿勢の角度から決まらない —— 決めるのは"
        [:strong "身体がどこに載っているか"] "である。座位では椅子が坐骨結節から体幹を受け、"
        "その荷重は股・膝・足関節を通らない。立位では骨盤の下に何も無いので、"
        "全体重が両脚を通って床へ届く。統計学的には差はちょうど 1 つの力（床反力）で、"
        "それが足部の重量より一桁大きい。"]
       [:div {:class "dds-ext-row"}
        (figure (if (= :standing (:mode sup)) "立位" "座位") "" "支持")
        (figure (math/fmt-fixed (:ground-reaction-per-foot-n sup) 0) "N" "床反力（片足）")
        (figure (math/fmt-fixed (:body-weight-n sup) 0) "N" "体重")]
       (when (= :standing (:mode sup))
         ;; BOTH branches in a `<strong>`, and not only the alarming one. The value
         ;; is `:cop-inside-base?` either way; emphasising one side made the page
         ;; carry the answer as a plain word in a sentence when it was `inside`,
         ;; which is indistinguishable from the sentence not stating it at all —
         ;; the coverage probe could not find it, and neither could a reader
         ;; scanning for the figure.
         [:p {:class "suji-note"}
          "圧力中心は"
          (if (:cop-inside-base? sup)
            [:strong "支持基底内"]
            [:strong "支持基底の外"])
          "。モデルはこれを拒否せず報告する —— 支持基底の外に重心がある姿勢は"
          "静止していられないが、それは力学の帰結であって入力の誤りではない。"]))
      ;; --- the degree of freedom the pelvis got, where a reader can see it -----
      (let [pst (:posture state)
            tilt (or (:pelvic-tilt-deg pst) 0.0)
            lord (pose/lumbar-lordosis-deg pst)
            chord (pose/lumbar-chord-tilt-deg (or (:trunk-flexion-deg pst) 0.0) tilt)
            trunk (or (:trunk-flexion-deg pst) 0.0)
            on-pelvis (sort (map key (filter #(= "pelvis" (:segment (:origin (val %))))
                                             attachment/muscles)))]
        (dds/card
         (dds/heading 3 "骨盤の傾きと腰椎の前弯")
         [:p {:class "suji-note"}
          "骨盤は suji 3d494ba まで回らなかった —— どの姿勢でも L5/S1 から真下に置かれて"
          "いたので、座位と立位は L5/S1 より下でしか違わず、腰椎は両者を区別できなかった。"
          "いま骨盤が回り、体幹は T12/L1 で 2 つに分かれているので、"
          [:strong "腰椎が胸郭とは別の向きを持つ"] "。"
          "前弯（Cobb L1–S1）は定義上その骨盤前傾そのものであり、"
          "腰椎の弦（1 本の剛体に与える 1 つの向き）は両端の中点 —— "
          [:strong "一定曲率という仮定"] "から出る値で、測定ではない。"]
         [:div {:class "dds-ext-row"}
          (figure (math/fmt-fixed lord 1) "°" "腰椎前弯（Cobb L1–S1）")
          (figure (math/fmt-fixed chord 1) "°" "腰椎の弦の傾き")
          (figure (math/fmt-fixed trunk 1) "°" "胸郭の傾き")]
         [:p {:class "suji-note"}
          "骨盤に起始を持つ筋・靭帯は " [:strong (str (count on-pelvis))] " 群 —— "
          (str/join "・" (map #(str/replace % "_" " ") on-pelvis))
          "。骨盤が回るとその起始が動き、モーメントアームが変わる。"
          [:strong "前弯は表示だけの量ではない"] "。"
          ;; DERIVED, both ways. The T12/L1 joint is placed and nothing is solved
          ;; at it — but that is a fact about today's model, so it is read off the
          ;; joint-moment table below rather than asserted here. The day an
          ;; equilibrium is written at T12/L1, this sentence flips on its own
          ;; instead of going quietly false, which is the failure this view has
          ;; recorded five times.
          (if (t12l1-solved? loads)
            [:strong "T12/L1 でも平衡を解いている。"]
            [:strong "T12/L1 の関節は置かれているが、そこで解く平衡はまだ無い"
             "（下の「関節モーメント」にその行が無いのがそれである）。"])]
         (preset-lordosis-note)))
      (dds/card
       (dds/heading 3 "関節モーメント")
       (dds/table
        {:headers ["関節" "モーメント" "内訳"]
         :rows (mapv (fn [j] [(:joint j) (str (math/fmt-fixed (:moment-nm j) 2) " N·m") (:note j)])
                     (:joints loads))}))
      (dds/card
       (dds/heading 3 "筋の緊張と強張り")
       (when-let [ls (seq (filter :ligament? tensions))]
         [:p {:class "suji-note"}
          "靭帯は筋ではない —— 収縮しないので %MVC を持たず、関節が既にそこまで運ばれて"
          "初めて張る。深い体幹前屈では脊柱起立筋が電気的に沈黙し、"
          [:strong "後方靭帯系が荷重を引き受ける"] "（屈曲弛緩）。"
          ;; STATED IN BOTH DIRECTIONS. `:at-limit?` used to reach the page only
          ;; when it was true, so `no warning` and `nobody asked` looked the same —
          ;; and the difference matters, because the second means the force beside
          ;; it may be an extrapolation nobody checked. Measured 2026-09-08: no
          ;; ligament reaches its calibration limit anywhere these sliders go
          ;; (posterior lumbar ligaments read 3,989 N at 60° of trunk flexion and
          ;; are still inside it), so the warning branch was unreachable and the
          ;; page said nothing about the check at all.
          (if (some :at-limit? ls)
            [:strong "⚠ 一部の靭帯は較正範囲を超えて伸ばされており、表示中の力は"
             "外挿ではなく打ち切った値である。"]
            [:strong "いま表示中の靭帯はすべて較正範囲内にある。"])])
       [:p {:class "suji-note"}
        "「出せる力」はその筋が" [:strong "この姿勢の長さで"]
        "出せる力（PCSA × 比張力 × Hill の力‑長さ係数）。%MVC の分母はこれであって"
        "ピーク値ではない —— 筋はどの長さでも最大を出せるわけではなく、"
        "長さを決めるのは姿勢だから。⚠ は最大随意収縮を超えていることを示す。"
        "張力の括弧内は（能動 / 受動）—— 受動は伸ばされた組織自身が出す力で、"
        "活動を要さず代謝コストも無い。"]
       (dds/table
        {:headers ["筋" "モーメントアーム" "張力（能動/受動）" "出せる力" "%MVC" "帯"
                   "保持できる時間" "ドーズ"
                   (str (int (:session-minutes state)) "分後") "当てはめ範囲"]
         :rows (mapv (fn [t st]
                       (if (:refused t)
                         ;; A refusal is shown AS a refusal. Rendering a dash in
                         ;; the %MVC column and nothing else would let a reader
                         ;; take it for a small number; the reason is the answer.
                         [(str/replace (:name t) "_" " ")
                          (coeff-label t) "—" "—" "適用範囲外" "—" "—" "—" "—" "—"]
                         [(str/replace (:name t) "_" " ")
                          (coeff-label t)
                          ;; active and passive shown apart: one is asked for and
                          ;; costs something, the other is the tissue and does not
                          (str (math/fmt-fixed (:force-n t) 0) " N"
                               (when (and (:passive-n t) (> (:passive-n t) 0.5))
                                 (str " (" (math/fmt-fixed (:active-n t) 0)
                                      " / " (math/fmt-fixed (:passive-n t) 0) ")")))
                          ;; the denominator of the %MVC beside it: what this
                          ;; muscle can produce AT THIS LENGTH, not its peak
                          (str (math/fmt-fixed (:f-max-n t) 0) " N")
                          ;; a LIGAMENT has no %MVC — it cannot contract, so
                          ;; there is no maximum voluntary contraction to be a
                          ;; fraction of. Asking `:refused` here would have missed
                          ;; it; asking whether the number is present does not.
                          (if (muscle/numeric-mvc? t)
                            (str (math/fmt-fixed (:mvc-pct t) 1) " %"
                                 (when (:over-mvc? t) " ⚠"))
                            (if (:ligament? t) "靭帯（収縮しない）" "—"))
                          ;; ⚠ A THIRD ANSWER, since the coupled solve. A muscle
                          ;; the optimum switches off comes back with a FORCE of
                          ;; zero and a %MVC of zero — not refused, because the
                          ;; model did compute it — and `band-for 0.0` is `low`.
                          ;; So eleven muscles at the default posture were being
                          ;; banded as lightly loaded when the answer is that they
                          ;; are not recruited at all. `low` and `off` are not the
                          ;; same claim and the difference is the whole content of
                          ;; `:inactive?`.
                          (cond
                            (:inactive? t) "無活動（最適解が切っている）"
                            (:ligament? t) "靭帯"
                            :else (or (:band (scene/band-for (:mvc-pct t))) "—"))
                          ;; HOW LONG IT CAN BE HELD, which is the ergonomic
                          ;; question and was computed and discarded. The band
                          ;; alone cannot answer it: a muscle at 9 %MVC has an
                          ;; unbounded holding time and a high 120-minute dose,
                          ;; and both are true.
                          (endurance-label st)
                          ;; the dose itself, of which the index is a saturating
                          ;; transform. `suji` kept it because the index runs out
                          ;; of range where the dose does not — above about 0.70
                          ;; the four bands stop distinguishing and this column
                          ;; goes on.
                          (if (number? (:dose st)) (math/fmt-fixed (:dose st) 3) "—")
                          (str (strain/stiffness-band (:stiffness-index st))
                               (when (:saturated? st) "（飽和）")
                               (when (:over-endurance st) "・保持時間超過"))
                          ;; the caveat that qualifies the holding time, per row.
                          ;; Without it an extrapolated number and a fitted one are
                          ;; the same shape, which is the defect `strain/endurance`
                          ;; exists to have fixed one layer down.
                          (get endurance-position-label (:endurance-position st) "—")]))
                     tensions strains)})
       ;; The reason comes from the data, not from a sentence written here. There
       ;; is more than one way this model declines — too little leverage for a
       ;; straight line (which a wrapping surface fixes) and a line that would act
       ;; the wrong way at this posture (which nothing here fixes) — and a fixed
       ;; explanation would keep naming the first after the second became the only
       ;; one that happens. Measured 2026-09-06: wrapping surfaces removed every
       ;; leverage-floor refusal, and the hardcoded sentence went on citing them.
       ;; ⚠ THIS BLOCK ASKED `:refused` AND SO DROPPED ELEVEN EXPLANATIONS. The
       ;; coupled solve writes a `:note` on every muscle it switches off — "the
       ;; coupled optimum switches this muscle off here: its price at the solved
       ;; multipliers is -0.000444, so activating it would raise the cost without
       ;; helping any of [:c7 :atlanto-occipital]" — and at the default posture
       ;; eleven rows carry one while only four are refused. Asking for the note
       ;; rather than for the reason is the same correction `numeric-mvc?` records
       ;; for the %MVC column: a site that branches on which reason it is has to be
       ;; revisited every time a new reason appears, and this one was not.
       (when-let [r (seq (filter #(and (:note %) (or (:refused %) (:inactive? %)))
                                 tensions))]
         (into [:div]
               (for [t r]
                 [:p {:class "suji-note"}
                  [:strong (str/replace (:name t) "_" " ")]
                  (if (:refused t)
                    (str " の力を計算していない（" (name (:refused t)) "）—— ")
                    " は最適解が切っている（力は 0 N、計算していないのではない）—— ")
                  (:note t)]))))
      (solve-card tensions loads)]]))

(defn compare-view [state]
  (let [body (body-of state)]
    [:div {:class "dds-ext-container"}
     (dds/section {}
      (dds/heading 2 "作業環境の比較")
      [:p {:class "suji-note"}
       "同じ身体（" (math/fmt-fixed (get-in state [:body :total-mass-kg]) 0) " kg / "
       (math/fmt-fixed (get-in state [:body :stature-m]) 2) " m）を 3 つの設定に置いた結果。"
       "同一人物の設定間比較であり、人と人を並べるものではない（G3）。"]
      (dds/table
       {:headers ["作業環境" "頭部前屈" "頸椎圧縮" "×頭部重量" "L5/S1" "最大 %MVC"]
        :rows (mapv (fn [w]
                      (let [p (posture/posture-from-workstation w)
                            {:keys [loads tensions]} (scene/solve-and-scene body p)
                            cerv (:cervical loads)
                            ls (first (filter #(= "lumbosacral" (:joint %)) (:joints loads)))]
                        [(:name w)
                         (str (math/fmt-fixed (:head-flexion-deg p) 0) "°")
                         (str (math/fmt-fixed (:compressive-load-kgf cerv) 1) " kgf")
                         (str (math/fmt-fixed (:multiplier-vs-head cerv) 1) "×")
                         (str (math/fmt-fixed (:moment-nm ls) 1) " N·m")
                         (if-let [mx (:max-mvc-pct (muscle/tension-summary tensions))]
                           (str (math/fmt-fixed mx 1) " %"
                                (when-not (:complete? (muscle/tension-summary tensions loads)) " ⚠"))
                           "適用範囲外")]))
                    posture/reference-workstations)}))]))

(defn crossing-label
  "Which muscles cross this level, as a readable list — or why none do.

  The 筋ぶん column is a sum, and a sum cannot be checked. Until 2026-09-07 the
  model decided crossing by a half-space test on height, so at the default posture
  the whole C3/C4 row was carried by the two WRIST EXTENSORS — a force that
  transmits to the forearm, credited to somebody's neck — and nothing on the page
  could have shown that, because the page only ever printed the total. The rule is
  a path test now, and this column is what makes the next such error visible
  without reading the source.

  An empty crossing set is stated rather than left blank: `C3/C4` genuinely has
  none in some configurations, and a blank cell reads as a rendering failure."
  [r]
  (let [names (mapv first (:muscle-crossing r))]
    (if (seq names)
      (str/join "、" (map #(str/replace % "_" " ") names))
      "（なし）")))

(defn spine-view [state]
  (let [{:keys [tensions loads]} (solved state)
        body (body-of state)
        rows (spine/profile body (:posture state) tensions)
        xcheck (spine/cervical-cross-check body (:posture state) tensions (:cervical loads))
        steps (spine/attachment-steps rows)]
    [:div {:class "dds-ext-container"}
     (dds/section {}
      (dds/heading 2 "椎間板レベルごとの圧縮")
      [:p {:class "suji-note"}
       "各レベルに載る重量と、そのレベルを跨ぐ筋張力の軸成分の和を、そのレベルの"
       "椎間板断面積で割ったもの。"
       [:strong "組織の項が支配的である"]
       " —— 伸筋は短いモーメントアームで働くので、小さな外部モーメントを保つのに"
       "大きな力が要り、その力は全部が関節を圧迫する。"
       "その担い手は姿勢で入れ替わるので、筋と靭帯を分けて示す。"]
      (dds/table
       {:headers ["レベル" "部位" "体重ぶん" "筋ぶん" "靭帯ぶん" "合計" "断面積" "圧縮応力" "跨いでいる筋"]
        :rows (mapv (fn [r] [(:name r) (name (:region r))
                             (str (math/fmt-fixed (:weight-n r) 0) " N")
                             (str (math/fmt-fixed (:muscle-n r) 0) " N")
                             (str (math/fmt-fixed (:ligament-n r) 0) " N")
                             (str (math/fmt-fixed (:force-n r) 0) " N")
                             (str (math/fmt-fixed (:disc-area-cm2 r) 1) " cm²")
                             (str (math/fmt-fixed (:stress-mpa r) 2) " MPa")
                             (crossing-label r)])
                    rows)})
      [:p {:class "suji-note"}
       "「跨いでいる筋」は" [:strong "経路"] "で決まる —— そのレベルを取り除いたとき"
       "身体が 2 つに分かれ、筋の 2 つの付着が別々の側に落ちるかどうか。"
       "2026-09-07 まではこれが" [:strong "高さの判定"] "だったので、既定の姿勢では"
       "C3/C4 の行を手関節伸筋 2 本だけが担っていた —— 前腕へ力を伝える筋が、"
       "誰かの首に計上されていた。合計だけを見ていると、この種の誤りは見えない。"]
      [:p {:class "suji-note"}
       "深い体幹前屈では「筋ぶん」が縮み「靭帯ぶん」が支配的になる（屈曲弛緩）。"
       "そのとき脊椎を圧迫しているのは筋ではなく後方靭帯系であり、"
       [:strong "何を変えれば減るのかが違う"] "。"]
      [:p {:class "suji-note"}
       [:strong "⚠ この表は検証されていない。"]
       "頸椎については、検証済みの集中定数モデル（Hansraj 2014）が "
       (math/fmt-fixed (:lumped-force-n xcheck) 0) " N とするところをこの表は "
       (math/fmt-fixed (:level-force-n xcheck) 0) " N とし、比は "
       (math/fmt-fixed (:ratio xcheck) 2) " 倍ある。"
       "集中定数側は公表値に合わせた実効レバーを使い、この表は筋の幾何モーメントアームを"
       "使うためである。"
       [:strong "検証を継承しているのは集中定数の側だけ"] "である。"]
      (when (seq steps)
        [:p {:class "suji-note"}
         "また、実際の筋は複数の椎骨にまたがって付着するが、このモデルは点で付着させる。"
         "そのため " (str/join "・" (map #(str (:after %) " → " (:at %)) steps))
         " で筋の寄与が段差状に 0 へ落ちる —— これはモデルの人工物であって身体ではない。"]))]))

(defn method-view
  "What this model does, and what it can and cannot answer for.

  ⚠ THE COUNTS AND THE LISTS ARE COMPUTED, NOT WRITTEN. This page's whole job is
  to say what is true, and on 2026-09-07 four of its claims were false:

    - it called the dose layer `Rohmert の等尺性持久モデル`. It is not Rohmert's
      equation — `strain/model-form` reports `:family :power` with
      `:provenance :could-not-obtain`, a plain power law with a bolted-on floor.
    - it listed `腱の巻き付き面` as unimplemented. Nineteen muscle groups declare
      one, and the middle deltoid's was the subject of a whole wave.
    - it listed `椎間板の個別モデル` as unimplemented. Ten levels exist.
    - it said the frontal plane had no muscles yet. There are five.

  Prose about a model goes stale the moment the model moves, and this model moves
  about once an hour. Anything derivable is derived, so the page cannot make a
  false claim about a count without the count being false."
  [state]
  (let [sol (solved state)
        {:keys [tensions loads]} sol
        body (body-of state)
        wrapped (count (filter :wrap (vals attachment/muscles)))
        frontal (sort (map :name (filter #(= :frontal (:axis %)) (vals attachment/muscles))))
        cerv (spine/cervical-cross-check body (:posture state) tensions (:cervical loads))
        lumbar (spine/lumbar-cross-check)
        sitstand (spine/sitting-standing-comparison)
        census (coverage/census body (:posture state) sol)
        counts (:counts census)
        summary (muscle/tension-summary tensions loads)
        dose strain/model-form]
    [:div {:class "dds-ext-container"}
     (dds/section {}
      (dds/heading 2 "計算の中身")
      [:p "姿勢（関節角）→ 前方運動学で 3 次元に配置 → 静的逆動力学（RNEA の重力項）→ "
          "関節モーメント → 起始・停止から幾何で出したモーメントアームで筋張力と %MVC → "
          "べき則の等尺性持久モデルで作業時間ぶんのドーズ。"]

      (dds/heading 3 "3 つの相互検証と、その向き")
      [:p {:class "suji-note"}
       "検証された量は 1 つだけで、残り 2 つは" [:strong "食い違いを報告する"] "。"
       "比が 1 に近づくことは検証ではない —— 近づいた原因が無関係な欠陥の修正である"
       "ことが、このリポジトリでは 2 回起きている。"]
      (dds/table
       {:headers ["対象" "文献" "モデル" "比" "どちらが検証済みか"]
        :rows [["頸椎の圧縮荷重（集中定数）" "Hansraj 2014"
                "0°→1 倍 … 60°→5 倍を 10% 以内で再現" "—"
                [:strong "モデル側が検証済み"]]
               ["頸椎のレベル別プロファイル" "上の集中定数モデル"
                (str (math/fmt-fixed (:level-force-n cerv) 0) " N")
                (math/fmt-fixed (:ratio cerv) 3)
                (str "検証済みは " (name (:validated cerv)))]
               ;; ⚠ THE SECOND CELL WAS THE LITERAL `Wilke 1999 in vivo 椎間板内圧`
               ;; until 2026-09-08, on a page whose entire job is to say what is
               ;; true. The model carries the citation, the URL, the posture the
               ;; reference was measured in, and the caveat the paper states about
               ;; itself; a hand-typed label cannot go stale loudly, and swapping
               ;; `default-lumbar-reference-id` for another entry would have left
               ;; this cell naming the wrong paper.
               [(str (:level lumbar) " の圧縮（" (:reference-label lumbar) "）")
                [:span (:citation lumbar) " "
                 [:a {:href (:url lumbar)} "本文"]]
                (str (math/fmt-fixed (:model-force-n lumbar) 0) " N vs "
                     (math/fmt-fixed (:reference-force-n lumbar) 0) " N")
                (math/fmt-fixed (:ratio lumbar) 3)
                (if (:within-reference-spread? lumbar)
                  "基準の幅の内"
                  [:strong "基準の幅の外（モデルが低い）"])]]})
      [:p {:class "suji-note"}
       "L4/L5 の基準について文献自身が言っていること —— "
       [:strong (:reference-caveat lumbar)]
       "。被験者は " (math/fmt-fixed (:mass-kg (:subject lumbar)) 0) " kg / "
       (math/fmt-fixed (:stature-m (:subject lumbar)) 2) " m、"
       "椎間板断面積はモデルが "
       [:strong (str (math/fmt-fixed (:model-disc-area-mm2 lumbar) 0) " mm²")]
       "、基準側が "
       [:strong (str (math/fmt-fixed (:reference-disc-area-mm2 lumbar) 0) " mm²")]
       "。圧力から力への換算は Nachemson の圧力指数 "
       [:strong (math/fmt-fixed (:mean (:pressure-index lumbar)) 2)]
       " による（"
       (math/fmt-fixed (:reference-pressure-mpa lumbar) 2)
       " MPa の実測値から）。姿勢の根拠は文献の記述そのもの —— "
       (:posture-basis lumbar)]

      ;; --- what the degree of freedom made comparable -------------------------
      (dds/heading 3 "立位と座位 —— 骨盤が回るまで比べられなかった比較")
      [:p {:class "suji-note"}
       "Wilke は安静立位と安静座位を" [:strong "別々に"] "測っている（0.50 MPa と 0.46 MPa）。"
       "この模型は 2 度この比較を拒否していた —— 最初は立てなかったから、次は"
       "座位と立位が L5/S1 より下でしか違わず L4/L5 に同じ力を返していたから。"
       "骨盤が回るようになって初めて、両者を分けるもの（前弯）を模型が持てた。"]
      (dds/table
       {:headers ["姿勢" "この模型" "Wilke" "比" "入れた前弯" "基準の幅"]
        :rows (mapv (fn [[label c]]
                      [label
                       (str (math/fmt-fixed (:model-force-n c) 1) " N")
                       (str (math/fmt-fixed (:reference-force-n c) 1) " N")
                       (math/fmt-fixed (:ratio c) 3)
                       (str (math/fmt-fixed (:lumbar-lordosis-deg c) 1) "°")
                       (if (:within-reference-spread? c)
                         "内"
                         (if (= :model-above-reference (:direction c))
                           "外（模型が高い）" "外（模型が低い）"))])
                    [["安静座位（丸椅子）" (:sitting sitstand)]
                     ["安静立位" (:standing sitstand)]])})
      (dds/table
       {:headers ["量" "値" "意味"]
        :rows [["立位 − 座位（この模型）"
                (str (math/fmt-fixed (:model-difference-n sitstand) 1) " N") ""]
               ["立位 − 座位（Wilke）"
                (str (math/fmt-fixed (:reference-difference-n sitstand) 1) " N") ""]
               ["比"
                (math/fmt-fixed (:difference-ratio sitstand) 3)
                "1 を大きく超えるほど、この模型は前弯に対して敏感すぎる"]
               ["向き"
                (if (:same-direction? sitstand) "向きは一致する" "向きは一致しない")
                "一致しても証拠にはならない（下）"]
               ["検証済みなのは"
                (name (:validated sitstand))
                (if (:model-validated? sitstand)
                  "この模型も検証済み"
                  "この模型は検証されていない")]]})
      ;; ⚠ THE MODEL'S OWN SENTENCE, not a paraphrase. `suji` states why the
      ;; agreement in direction is worthless here, and it states it more carefully
      ;; than a UI would: any lordosis of either sign raises compression in this
      ;; model, so the sign followed from which posture was given the smaller
      ;; lordosis and could not have come out the other way. Printed verbatim so
      ;; that the day the mechanism changes, the caveat on the page changes with
      ;; it — and so that `:from-value` can check that it is the model's words.
      [:p {:class "suji-note"}
       [:strong "向きの一致は証拠ではない"] " —— "
       (:direction-is-not-evidence sitstand)
       "。大きさの方は " [:strong (math/fmt-fixed (:difference-ratio sitstand) 3) " 倍"]
       " ずれており、そちらが所見である。"]
      ;; the input the reference's own source does not state, carried out of the
      ;; model rather than restated: its name, its provenance id, and its note.
      (when-let [pnis (:parameter-not-in-source lumbar)]
        [:p {:class "suji-note"}
         [:strong "基準側が述べていない入力"] " —— "
         [:strong (subs (str (:parameter pnis)) 1)] " ＝ "
         [:strong (str (math/fmt-fixed (:value pnis) 1) "°")]
         "（出所 " [:strong (subs (str (:from pnis)) 1)] "、実測 "
         (math/fmt-fixed (:measured-lordosis-deg pnis) 1) "°）。"
         (:note pnis)])

      (dds/heading 3 "実測された前弯と、この面が届く範囲")
      [:p {:class "suji-note"}
       "左 2 列は " [:strong "モデルが持っている実測値"] "（Cho ら 2015）で、"
       "右 2 列は" [:strong "この app が自分のプリセットとスライダーから数えたもの"]
       " —— どちらも書いた文ではない。"]
      (dds/table
       {:headers ["姿勢（Cho）" "前弯" "SD" "プリセットが持つ" "スライダーが届く"]
        :rows (mapv (fn [{:keys [posture deg sd preset-carries slider-reaches]}]
                      [(subs (str posture) 1)
                       (str (math/fmt-fixed deg 1) "°")
                       (str (math/fmt-fixed sd 1) "°")
                       (if (seq preset-carries)
                         (str/join "・" (map #(str (math/fmt-fixed % 1) "°") preset-carries))
                         "（無し）")
                       (if slider-reaches "届く" "届かない")])
                    (measured-lordosis))})
      (preset-lordosis-note)

      (dds/heading 3 "実装されているもの（数はモデルから数えている）")
      [:ul
       [:li "椎間板レベル：" [:strong (str (count spine/levels))]
            "（" (str/join "・" (map :name spine/levels)) "）"]
       [:li "巻き付き面を持つ筋群：" [:strong (str wrapped)]
            " —— 直線モデルには巻き付き面が無いので、作用線が関節を通る近傍で"
            "必要張力が発散する。面がある筋はそこで床を持つ。"]
       [:li "前額面（外転・側屈）の筋：" [:strong (str (count frontal))]
            "（" (str/join "・" frontal) "）"]
       [:li "モーメントアームは筋の起始・停止から幾何で計算する（定数表ではない）。"]
       ;; ⚠ THIS ITEM DID NOT EXIST ON 2026-09-07 AND WAS THE LARGEST THING THE
       ;; MODEL COULD NOT DO. The closed form solved one equality constraint, so a
       ;; two-joint muscle was solved at one joint and its moment at the other was
       ;; reported and not satisfied. Both numbers are counted rather than stated.
       [:li "同時に満たしている平衡："
            [:strong (str (count (distinct (keep :coupled-joints tensions))))]
            " 群（"
            (str/join "、"
                      (for [js (distinct (keep :coupled-joints tensions))]
                        (str/join "·" (map joint-name js))))
            "）。残差の最大は "
            [:strong (str (math/fmt-fixed
                           (* 1.0e12 (reduce max 0.0
                                             (map math/abs*
                                                  (vals (:coupled-residual-nm summary)))))
                           3)
                          " pN·m")]
            " —— 満たしていると言うだけでなく、どれだけ満たしているかを出す。"]
       ;; ⚠ COUNTED, and the count is the thing that was wrong for a day. suji
       ;; split `thorax_abdomen` at T12/L1 and three tables in `scene` were still
       ;; keyed by the name it lost, so both new segments drew unloaded. Nothing
       ;; here may say `two` as a word: it says however many `segment/trunk-bases`
       ;; names, and names them.
       [:li "体幹の分節："
            [:strong (str (count segment/trunk-bases))]
            "（" (str/join "・" segment/trunk-bases) "）—— T12/L1 で分かれており、"
            "腰椎は胸郭とは別の向きを持てる。"
            (if (t12l1-solved? loads)
              "T12/L1 でも平衡を解いている。"
              "ただし T12/L1 で解く平衡はまだ無い。")]
       [:li "骨盤の回転："
            [:strong (if (contains? (set (map (comp last :path) controls)) :pelvic-tilt-deg)
                       "入力できる" "入力できない")]
            "（前弯 ＝ Cobb L1–S1 はこの入力そのもの）。"
            "骨盤に起始を持つ筋・靭帯は "
            [:strong (str (count (filter #(= "pelvis" (:segment (:origin %)))
                                         (vals attachment/muscles))))]
            " 群あり、骨盤が回るとその起始とモーメントアームが動く。"]
       [:li "骨は解剖学的メッシュで描く（円柱ではない）。筋は線のまま —— "
            "モデルが断面を持たないので、太さを描けばそれは装飾である。"]]

      (dds/heading 3 "モデルが出した量のうち、何がこのページに出ているか")
      [:p {:class "suji-note"}
       "この 3 つの数も一覧も" [:strong "モデルを歩いて数えている"]
       " —— `suji` が量を 1 つ増やしてこのページが何もしなければ、"
       "「出していない」の数が 1 増える。"
       "既定の姿勢で 1 回解いた結果に対して数えたもので、"
       "姿勢によって存在しない量（拒否の理由、靭帯の較正状態）はその姿勢の数に入らない。"]
      (dds/table
       {:headers ["状態" "件数" "意味"]
        :rows [["出している" (str (:shown counts))
                "いずれかの view がこの値をページに書いている"]
               ["出していない" (str (:computed-not-shown counts))
                "この app の解が計算していて、どの view も読んでいない"]
               ["訊いていない" (str (:not-computed counts))
                (str "`suji` が答えを持っているが、この app が呼んでいない入口の分"
                     "（下限 —— 数えているのは "
                     (count coverage/unrequested-probes) " 個の入口だけ）")]]})
      [:p {:class "suji-note"}
       [:strong "出していない量："]
       (str/join "、" (map #(str (namespace %) "/" (name %))
                           (:computed-not-shown census)))]
      [:p {:class "suji-note"}
       [:strong "訊いていない入口："]
       (str/join "、" (sort (map name (keys coverage/unrequested-probes))))]

      (dds/heading 3 "持久モデルの出自")
      [:p {:class "suji-note"}
       "族は " [:strong (name (:family dose))] "、出自は "
       [:strong (name (:provenance dose))] "。"
       (when (= :could-not-obtain (:provenance dose))
         (str "係数と一致する公表された当てはめを見つけられなかった、という意味である —— "
              "「まだ探していない」ではなく、探して見つからなかった。"))]

      (dds/heading 3 "まだ無いもの")
      ;; ⚠ THE FIRST ITEM HERE WAS FALSE UNTIL 2026-09-08. It said the suboccipital
      ;; group was missing because the atlanto-occipital joint did not exist and the
      ;; head and neck were one rigid body. The joint exists, and three suboccipitals
      ;; are solved across it. That is the fifth false claim this section has made,
      ;; and the fix is the one the rest of this view already uses: derive it.
      ;;
      ;; What CANNOT be derived stays prose, and is prose about the shape of the
      ;; model rather than about its contents — an attachment being a point is not a
      ;; key anything can count.
      [:ul
       (when-not (contains? attachment/muscles "obliquus_capitis_inferior")
         [:li "後頭下筋のうち" [:strong "下頭斜筋"]
              " —— 両端が環軸間に乗るので、環椎後頭関節ではなく環軸関節が要る。"
              "他の 3 つ（大後頭直筋・小後頭直筋・上頭斜筋）は解かれている。"])
       (when-let [js (seq (sort-by str (keys (:two-joint-unfed-nm summary))))]
         [:li "どの平衡にも渡されていないモーメントが残る関節："
              [:strong (str/join "・" (map joint-name js))]
              " —— 連立解は首の 2 関節と片脚の 3 関節を同時に満たすが、"
              "この関節群にはそもそも解くべき方程式が無い。"
              "c2c3 を阻んでいるのは分節化ではなく出典で、"
              "集中定数 cervical_extensors の 12.0 cm² 自体に出典が無い。"])
       [:li "筋の付着は点であって、複数椎骨にまたがる面ではない。"]
       [:li "PCSA とモーメントアームは代表値であって個人の測定値ではない（G7）。"]]

      (dds/heading 3 "境界")
      [:p "力学量だけを返す。診断・処方・治療は表現できない（医師法 §17 / G1）。"
          "計測ハードウェアを持たず、入力は姿勢のパラメータである（薬機法 / G2）。"]
      [:p {:class "suji-note"}
       "物理は " [:a {:href "https://github.com/cloud-itonami/suji"} "cloud-itonami/suji"]
       "、描画は " [:a {:href "https://github.com/kotoba-lang/webgpu"} "kotoba-lang/webgpu"]
       "（WebGPU、WebGL 2.0 フォールバック）。"])]))

(defn support-note
  "Which regime the numbers on this page came from.

  Stated in the panel, not inferred from the preset name, because the hip, knee
  and ankle sliders mean two different things depending on it and nothing else on
  the page says which. Seated, the chair carries the trunk and those joints hold
  only the limb below them; standing, they carry the whole body — the same angles
  give 333 N through a standing ankle and 10 N through a seated one."
  [state]
  (if (posture/standing? (:posture state))
    [:p {:class "suji-note"}
     [:strong "立位"] " —— 体重は両脚を通って床へ届く。股・膝・足関節は"
     "その関節より上の全部を支える。"]
    [:p {:class "suji-note"}
     [:strong "座位"] " —— 椅子が坐骨結節から体幹を受ける。その荷重は股・膝・足関節を"
     [:strong "通らない"] "ので、下肢の各関節が持つのはその先にある肢節だけ。"]))

(defn control-panel [state]
  (into [:div {:class "dds-ext-stack"}
         (dds/heading 3 "入力")
         (into [:div {:class "dds-ext-row"}]
               (for [{:keys [name group]} presets]
                 (dds/button (str (if (= :standing group) "立 " "座 ") name)
                             {:type (if (= name (get-in state [:posture :preset]))
                                      :solid-fill :outline)
                              :size "sm"
                              :attrs {:data-preset name}})))
         (support-note state)]
        (for [{:keys [path label unit min max step]} controls]
          [:div {:class "suji-control"}
           [:label {:for (str/join "-" (map name path))}
            label " " [:span {:class "suji-unit"}
                       (math/fmt-fixed (get-in state path) (if (< step 1) 2 0)) " " unit]]
           [:input {:type "range" :id (str/join "-" (map name path))
                    :min min :max max :step step
                    :value (get-in state path)
                    :data-path (str/join "/" (map name path))}]])))

(defn app
  "The whole page body for a state. One shell; the view is a value."
  [state]
  (let [view (:view state)]
    [:div {:class "suji-shell"}
     [:header {:class "dds-ext-row"}
      (dds/heading 1 "筋 suji — 姿勢がつくる負荷")
      (route/nav view)]
     [:main
      (case view
        :simulate [:div (simulate-view state) (dds/container (control-panel state))]
        :compare (compare-view state)
        :spine (spine-view state)
        :method (method-view state)
        (simulate-view state))]]))
