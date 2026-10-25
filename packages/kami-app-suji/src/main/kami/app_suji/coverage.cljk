(ns kami.app-suji.coverage
  "How much of what `suji` computes reaches the person looking at the page.

  THE QUESTION, AND WHY IT NEEDS A MEASUREMENT RATHER THAN A SENTENCE. This app is
  the visible half of a model that produces several hundred numbers per posture.
  Nothing in the compiler, the tests or the browser check notices when the model
  gains a quantity and the page does not: the new key simply sits in a map that no
  view reads. The failure is silent by construction, and it has already happened
  here twice in the other direction — the README recorded five 'we do not have
  this' claims that were all false, and `method-view` four.

  So the census is COMPUTED, in three parts, and only the middle one is written
  by hand:

    `produced`     every quantity the app's own solve puts in a map, derived by
                   walking the actual result. Grows on its own when `suji` does.
    `registry`     for each produced quantity, whether a view renders it. This is
                   the claim, and `coverage-test` is what stops it being prose.
    `unrequested`  quantities `suji` will produce if asked, from entry points no
                   view calls. Also derived — by calling them.

  WHAT `:shown` HAS TO SURVIVE. A claim of `:shown` is checked by rendering every
  view and looking for the quantity's own VALUES inside a value-bearing element —
  a table cell, a readout figure, a `<strong>` — never in flattened page text. The
  distinction is not pedantry: three separate checks in this repo passed on prose
  that mentioned the thing beside the place the thing had been deleted from. A
  per-row quantity must be found for most of its rows, so one coincidental match
  in sixty cannot carry a column that is not there.

  NON-DIAGNOSTIC (G1): this namespace counts keys. It holds no clinical claim."
  (:require [kotoba.lang.text :as str]
            [suji.methods.girdle :as girdle]
            [suji.methods.load :as load]
            [suji.methods.math :as math]
            [suji.methods.muscle :as muscle]
            [suji.methods.pose :as pose]
            [suji.methods.spine :as spine]
            [suji.methods.strain :as strain]))

;; --- what the app's own solve produces ---------------------------------------

(defn- qid
  "A quantity's identity: the section it lives in and its key."
  [section k]
  (keyword section (name k)))

(defn- union-keys
  "Every key any member of a collection of maps carries. A key present on some
  rows and absent on others is still produced — `:at-limit?` exists only on
  ligaments and `:secondary-moment-nm` only on two-joint muscles, and a census
  that read the first row would miss both."
  [ms]
  (into #{} (mapcat keys) ms))

(defn- section-of
  "A section's quantities, as `{qid [values...]}` — the values are what the probe
  looks for on the page, so they are collected here rather than re-derived."
  [section ms]
  (into {}
        (for [k (union-keys ms)]
          [(qid section k) (vec (keep #(get % k) ms))])))

(defn produced
  "Every quantity this app's solve produces, as `{qid [values]}`.

  `solved` is `kami.app-suji.core/solved`'s value, passed in rather than computed
  here so that `core` can require this namespace to render the census without a
  cycle."
  [body posture solved]
  (let [{:keys [loads tensions strains]} solved
        rows (spine/profile body posture tensions)]
    (merge
     (section-of "load.cervical" [(:cervical loads)])
     (section-of "load.atlanto-occipital" [(:atlanto-occipital loads)])
     (section-of "load.joint" (:joints loads))
     (section-of "load.frontal" [(:frontal loads)])
     (section-of "load.support" [(:support loads)])
     (section-of "tension" tensions)
     (section-of "strain" strains)
     (section-of "summary" [(muscle/tension-summary tensions loads)])
     (section-of "spine.level" rows)
     (section-of "spine.cervical-check"
                 [(spine/cervical-cross-check body posture tensions (:cervical loads))])
     (section-of "spine.lumbar-check" [(spine/lumbar-cross-check)])
     ;; --- the pelvis's degree of freedom, as the page's own posture holds it ----
     ;; `lumbar-lordosis-deg` is a function of the CURRENT posture, so unlike the
     ;; cross-check's copy it moves when the slider moves — which is the only way
     ;; the differential probe can ask whether it is on the page at all. suji
     ;; reports it as its own function rather than as `:pelvic-tilt-deg` precisely
     ;; because the two stop being the same number the day the thorax stops holding
     ;; the lumbar spine's upper end, so this reads the function.
     (section-of "pose"
                 [{:lumbar-lordosis-deg (pose/lumbar-lordosis-deg posture)
                   :lumbar-chord-tilt-deg
                   (pose/lumbar-chord-tilt-deg (get posture :trunk-flexion-deg 0.0)
                                               (get posture :pelvic-tilt-deg 0.0))}])
     ;; --- what the degree of freedom made comparable ---------------------------
     ;; Wilke's relaxed standing entry was REFUSED for two different reasons on two
     ;; different days, and stopped being refused when the pelvis learned to rotate.
     ;; The app calls this now, so the census has to see it: a quantity the app
     ;; computes and the census does not walk is exactly the hole this namespace
     ;; exists to close.
     (section-of "spine.sitting-standing" [(spine/sitting-standing-comparison)]))))

;; --- what suji would produce if the page asked -------------------------------

(def unrequested-probes
  "Entry points `suji` exposes that no view in this app calls, each as a thunk of
  `[body posture solved]` returning a map.

  A FLOOR, NOT A CENSUS OF SUJI. These are the entry points whose absence from the
  page is a decision rather than an oversight, so the number the page reports is
  `at least` — `suji` has more. The list is written by hand and the KEYS are not:
  `every-unrequested-probe-returns-quantities` fails if one of these stops
  returning a map, which is what happens when it is renamed or removed upstream.

  ⚠ IT HAD AN EIGHTH ENTRY UNTIL 2026-09-08 and losing it is the census working.
  `load/atlanto-occipital-moment` took a third argument — the capitis forces — and
  returned the decomposition the uncoupled solve could not close. `recruit/solve`
  closed it, and the argument, `load/capitis-groups` and five reported quantities
  went with it. A hand-written list of gaps would still be listing them."
  {:load/cervical-load-sensitivity
   (fn [_ posture _]
     (load/cervical-load-sensitivity (get posture :head-flexion-deg 0.0) 50.0))

   :girdle/girdle-summary
   (fn [body posture _] (girdle/girdle-summary body posture))

   :spine/niosh-compression-comparison
   (fn [body posture solved]
     (spine/niosh-compression-comparison
      (:force-n (first (spine/profile body posture (:tensions solved))))))

   :strain/session-cross-check
   (fn [_ _ solved] (strain/session-cross-check (:tensions solved)))

   :strain/floor-discontinuity
   (fn [_ _ _] (strain/floor-discontinuity 120.0))

   :strain/band-resolution
   (fn [_ _ _] (strain/band-resolution 120.0))})

(defn unrequested
  "Quantities `suji` would produce from entry points no view calls, as
  `{qid [values]}`, minus anything the app's own solve already produces under the
  same name — `:residual-nm` reached by two different routes is one quantity."
  [body posture solved]
  (let [have (set (map name (keys (produced body posture solved))))]
    (into {}
          (for [[probe-id f] unrequested-probes
                :let [m (f body posture solved)]
                :when (map? m)
                [k v] m
                :when (not (contains? have (name k)))]
            [(qid (str "unrequested." (name probe-id)) k) [v]]))))

;; --- the claim ---------------------------------------------------------------

(def registry
  "For every quantity the app's solve produces, whether a view renders it.

    :shown               a view puts this value on the page
    :computed-not-shown  the solve produces it and nothing renders it

  `:token` is for a quantity whose rendering is not a number the probe can look
  for — a keyword, a boolean, a name, or a small integer count. It names the
  literal text the page must carry in a value-bearing element for the claim to
  hold, and `:from-value` means `use the model's own strings`, which is stronger
  because nobody has to keep it in step.

  A COUNT GETS ITS FIGURE'S LABEL, and that is a deliberately weaker check. There
  are four antagonists at the default posture and `4` occurs in `4.0`, `14` and
  `40` on any page with a hundred numbers on it, so the digits cannot answer. What
  can is that the figure is there at all — and the label is the text that
  disappears when it goes. The duplication is real: reword the label and this test
  fails until the registry is reworded too, which is the trade for being able to
  detect the figure's removal.

  The states are checked, not decorative: `every-produced-quantity-is-classified`
  fails when `suji` adds a key nobody has classified, and
  `every-shown-quantity-is-actually-on-the-page` fails when a `:shown` one stops
  being rendered."
  {;; --- the cervical lumped model: the one validated number this app has -----
   :load.cervical/compressive-load-kgf   {:state :shown}
   :load.cervical/compressive-load-n     {:state :shown}
   :load.cervical/multiplier-vs-head     {:state :shown}
   :load.cervical/extensor-moment-nm     {:state :shown} ; the cervicothoracic joint row
   :load.cervical/extensor-force-n       {:state :computed-not-shown}
   :load.cervical/head-tilt-deg          {:state :computed-not-shown}
   :load.cervical/head-weight-n          {:state :computed-not-shown}

   ;; --- the skull about the condyles ----------------------------------------
   :load.atlanto-occipital/joint         {:state :shown :token :from-value}
   :load.atlanto-occipital/moment-nm     {:state :shown}
   ;; NOT the same string as the joint table's note. `->joint-load` is given the
   ;; literal "the skull about the occipital condyles" and this map's own note adds
   ;; "; no fitted lever" — so what the page shows is a prefix of it, and a probe
   ;; asking for the model's own sentence rightly does not find it.
   :load.atlanto-occipital/note          {:state :computed-not-shown}
   :load.atlanto-occipital/skull-weight-n {:state :computed-not-shown}

   ;; --- the joint moment table ----------------------------------------------
   :load.joint/joint                     {:state :shown :token :from-value}
   :load.joint/moment-nm                 {:state :shown}
   :load.joint/note                      {:state :shown :token :from-value}
   :load.joint/per-side                  {:state :computed-not-shown}
   :load.joint/supported-weight-n        {:state :computed-not-shown}
   :load.joint/frontal-per-side          {:state :computed-not-shown}

   ;; --- the frontal plane ----------------------------------------------------
   :load.frontal/lumbosacral-nm          {:state :shown}
   :load.frontal/shoulder-per-side       {:state :shown}
   :load.frontal/cervical-nm             {:state :computed-not-shown}
   :load.frontal/shoulder-nm             {:state :computed-not-shown}

   ;; --- what holds the body up ----------------------------------------------
   :load.support/mode                    {:state :shown :token "座位"}
   :load.support/ground-reaction-per-foot-n {:state :shown}
   :load.support/body-weight-n           {:state :shown}
   :load.support/cop-inside-base?        {:state :shown :token "支持基底"}
   :load.support/base-of-support         {:state :computed-not-shown}
   :load.support/com-x                   {:state :computed-not-shown}
   :load.support/thigh-supported         {:state :computed-not-shown}

   ;; --- the muscle table ------------------------------------------------------
   :tension/name                         {:state :shown :token :from-value}
   :tension/coeff                        {:state :shown}
   :tension/force-n                      {:state :shown}
   :tension/active-n                     {:state :shown}
   :tension/passive-n                    {:state :shown}
   :tension/f-max-n                      {:state :shown}
   :tension/mvc-pct                      {:state :shown}
   :tension/over-mvc?                    {:state :shown :token "⚠"}
   :tension/refused                      {:state :shown :token "適用範囲外"}
   :tension/note                         {:state :shown :token :from-value}
   :tension/ligament?                    {:state :shown :token "靭帯（収縮しない）"}
   :tension/at-limit?                    {:state :shown :token "較正範囲内"}
   :tension/task                         {:state :shown :token "(cos)"}
   :tension/group                        {:state :computed-not-shown}
   :tension/side                         {:state :computed-not-shown}
   :tension/antagonist?                  {:state :computed-not-shown}
   ;; --- the coupled solve, and what it says about itself ----------------------
   ;; Every one of these arrived on 2026-09-08 with `recruit/solve`, and the seven
   ;; quantities the uncoupled form reported in their place went away the same day.
   ;; The census is what noticed; a written list of gaps would still be listing the
   ;; seven. All twelve were `:computed-not-shown` when they were first counted;
   ;; `solve-card` is what moved them.
   :tension/crosses-joint                {:state :shown :token :from-value}
   :tension/secondary-arm-m              {:state :shown}
   :tension/secondary-moment-nm          {:state :shown}
   :tension/secondary-fed?               {:state :shown :token "連立で解いた"}
   :tension/task-load-nm                 {:state :shown}
   :tension/coupled-group                {:state :shown :token :from-value}
   :tension/coupled-joints               {:state :shown :token :from-value}
   :tension/coupled-converged?           {:state :shown :token "収束した"}
   :tension/coupled-residual-nm          {:state :shown}
   :tension/inactive?                    {:state :shown :token "無活動"}
   :tension/price                        {:state :shown}
   :tension/coeffs                       {:state :shown}

   ;; --- the dose --------------------------------------------------------------
   ;; --- the dose ---------------------------------------------------------------
   ;; The band was the whole of this layer on the page until 2026-09-08, and it is
   ;; a saturating transform of a dose that keeps going after the band stops
   ;; distinguishing. The holding time, its caveat and the raw dose are all things
   ;; `strain` computes and says are worth carrying with the number they qualify.
   :strain/stiffness-index               {:state :shown :token "very-high"}
   :strain/saturated?                    {:state :shown :token "（飽和）"}
   :strain/endurance-minutes             {:state :shown}
   :strain/endurance-position            {:state :shown :token "当てはめ範囲内"}
   :strain/endurance-extrapolated?       {:state :shown :token "（外挿）"}
   :strain/unbounded-endurance?          {:state :shown :token "∞"}
   :strain/acute-dose                    {:state :computed-not-shown}
   :strain/chronic-dose                  {:state :computed-not-shown}
   :strain/dose                          {:state :shown}
   :strain/index-resolution              {:state :computed-not-shown}
   :strain/over-endurance                {:state :shown :token "・保持時間超過"}
   :strain/refused                       {:state :computed-not-shown}
   :strain/name                          {:state :computed-not-shown}
   :strain/group                         {:state :computed-not-shown}
   :strain/side                          {:state :computed-not-shown}
   :strain/task                          {:state :computed-not-shown}
   :strain/mvc-pct                       {:state :computed-not-shown}
   :strain/session-minutes               {:state :computed-not-shown}

   ;; --- the summary ------------------------------------------------------------
   :summary/complete?                    {:state :shown :token "すべて配置"}
   :summary/max-mvc-pct                  {:state :shown}
   :summary/two-joint-unfed-nm           {:state :shown}
   :summary/coupled-residual-nm          {:state :shown}
   :summary/coupled-not-converged        {:state :shown :token "収束しなかった行"}
   :summary/inactive                     {:state :shown :token "無活動（最適解が切った）"}
   :summary/total                        {:state :shown :token "筋・靭帯の行数"}
   :summary/refused                      {:state :shown :token "拒否（計算していない）"}
   :summary/antagonists                  {:state :shown :token "拮抗（反対側が担っている）"}
   :summary/over-mvc                     {:state :shown :token "最大随意収縮を超過"}
   ;; the frontal-plane moments reach the page through `:load.frontal/*`, which is
   ;; where they are produced; this is the same map carried along on the summary
   ;; for a consumer that has only the summary, and nothing renders THAT copy.
   :summary/frontal                      {:state :computed-not-shown}

   ;; --- the level table ---------------------------------------------------------
   :spine.level/name                     {:state :shown :token :from-value}
   :spine.level/region                   {:state :shown :token "cervical"}
   :spine.level/weight-n                 {:state :shown}
   :spine.level/muscle-n                 {:state :shown}
   :spine.level/ligament-n               {:state :shown}
   :spine.level/force-n                  {:state :shown}
   :spine.level/disc-area-cm2            {:state :shown}
   :spine.level/stress-mpa               {:state :shown}
   :spine.level/muscle-crossing          {:state :shown :token :from-value}

   ;; --- the two cross-checks ------------------------------------------------------
   :spine.cervical-check/level-force-n   {:state :shown}
   :spine.cervical-check/lumped-force-n  {:state :shown}
   :spine.cervical-check/ratio           {:state :shown}
   :spine.cervical-check/validated       {:state :shown :token "lumped"}
   :spine.cervical-check/note            {:state :computed-not-shown}

   :spine.lumbar-check/model-force-n     {:state :shown}
   :spine.lumbar-check/reference-force-n {:state :shown}
   :spine.lumbar-check/ratio             {:state :shown}
   :spine.lumbar-check/within-reference-spread? {:state :shown :token "基準の幅"}
   :spine.lumbar-check/citation          {:state :shown :token :from-value}
   ;; SHOWN since the standing/sitting table exists: that table's last column
   ;; branches on `:direction`, so `外（模型が低い）` is on the page only while the
   ;; model is below this reference. It is a weaker check than a number — the token
   ;; convention — but it is falsifiable: the day the model crosses the reference
   ;; the page says `高い` and this token stops being found.
   :spine.lumbar-check/direction         {:state :shown :token "外（模型が低い）"}
   :spine.lumbar-check/level             {:state :shown :token :from-value}
   :spine.lumbar-check/model-disc-area-mm2 {:state :shown}
   :spine.lumbar-check/model-ligament-n  {:state :computed-not-shown}
   :spine.lumbar-check/model-muscle-n    {:state :computed-not-shown}
   :spine.lumbar-check/model-stress-mpa  {:state :computed-not-shown}
   :spine.lumbar-check/model-validated?  {:state :computed-not-shown}
   :spine.lumbar-check/model-weight-n    {:state :computed-not-shown}
   :spine.lumbar-check/obtained          {:state :computed-not-shown}
   :spine.lumbar-check/posture           {:state :computed-not-shown}
   :spine.lumbar-check/posture-basis     {:state :shown :token :from-value}
   :spine.lumbar-check/pressure-index    {:state :shown}
   :spine.lumbar-check/reference-caveat  {:state :shown :token :from-value}
   :spine.lumbar-check/reference-disc-area-mm2 {:state :shown}
   :spine.lumbar-check/reference-force-range-n {:state :computed-not-shown}
   :spine.lumbar-check/reference-id      {:state :computed-not-shown}
   :spine.lumbar-check/reference-label   {:state :shown :token :from-value}
   :spine.lumbar-check/reference-pressure-mpa {:state :shown}
   :spine.lumbar-check/subject           {:state :shown}
   :spine.lumbar-check/url               {:state :shown :token :from-value}
   :spine.lumbar-check/validated         {:state :computed-not-shown}
   ;; --- the two keys suji 3d494ba added to the cross-check --------------------
   ;; They arrived because the pelvis learned to rotate: the reference posture now
   ;; carries a lordosis, and that lordosis is an input Wilke's paper does not
   ;; state. `every-quantity-the-model-produces-is-classified` failed on both the
   ;; moment the pin moved, which is the census doing its job.
   ;;
   ;; `lumbar-lordosis-deg` gets its COLUMN'S LABEL rather than its digits, and
   ;; deliberately: the default reference is Wilke's stool, whose lordosis is
   ;; exactly 0.0, and `significant?` rejects every rendering of zero on purpose —
   ;; `0.00` is on the page for a hundred unrelated reasons. What can be probed is
   ;; that the column is there, and its heading is the text that disappears when it
   ;; goes. The same quantity for the CURRENT posture is `:pose/lumbar-lordosis-deg`
   ;; below, which moves with the slider and is probed by difference.
   :spine.lumbar-check/lumbar-lordosis-deg {:state :shown :token "入れた前弯"}
   ;; the note, the parameter's own name and its provenance id, all as the model
   ;; wrote them — `string-values` and `keyword-strings` reach into the map
   :spine.lumbar-check/parameter-not-in-source {:state :shown :token :from-value}

   ;; --- the pelvis's degree of freedom at the posture on screen ---------------
   ;; ⚠ BY LABEL, AND THE FIRST VERSION WAS BY NUMBER — which passed with the two
   ;; figures DELETED. Measured 2026-09-09 by deleting them: the differential found
   ;; `:pose/lumbar-lordosis-deg`'s 25.0 as the `25` in a muscle row reading
   ;; `25 N (24 / 1)`, at the one posture where the quantity takes that value and
   ;; nowhere else — so the difference the probe is built on was satisfied by a
   ;; coincidence that happened to be posture-specific. The docstring on
   ;; `differential` says a coincidence cannot move when the posture does; a
   ;; coincidence in a table that DOES move when the posture does is the gap in
   ;; that argument, and this is an instance of it.
   ;;
   ;; So these take the figure's own label, the same convention a small integer
   ;; count takes and for the same reason: what can be probed is that the figure is
   ;; there, and the label is the text that disappears with it. That the NUMBER is
   ;; the model's is asserted exactly, by `the-lordosis-readout-is-the-models-own-
   ;; number` — where it can compare the two directly instead of hunting for digits
   ;; on a page that has three hundred of them.
   :pose/lumbar-lordosis-deg             {:state :shown :token "腰椎前弯（Cobb L1–S1）"}
   :pose/lumbar-chord-tilt-deg           {:state :shown :token "腰椎の弦の傾き"}

   ;; --- standing against sitting, comparable for the first time ---------------
   :spine.sitting-standing/model-difference-n     {:state :shown}
   ;; BY ITS ROW'S LABEL. Probed by digits it passed with the VALUE CELL deleted —
   ;; measured 2026-09-09 — because 48.0 renders as `48` and a page with three
   ;; hundred numbers on it has a standalone 48 somewhere. A label token cannot see
   ;; a value deleted from beside its label either; what it sees is the row going
   ;; away. That the number is the model's is asserted exactly by
   ;; `the-standing-comparison-is-the-models-numbers-and-its-caveat`, which does
   ;; fail on the value deletion. The two together are the claim.
   :spine.sitting-standing/reference-difference-n {:state :shown
                                                   :token "立位 − 座位（Wilke）"}
   :spine.sitting-standing/difference-ratio       {:state :shown}
   :spine.sitting-standing/same-direction?        {:state :shown :token "向きは一致する"}
   ;; the model's own sentence about why the agreement in direction proves nothing.
   ;; `:from-value` means the page has to carry it verbatim, so it goes stale the
   ;; day suji rewords it — which is the day it should.
   :spine.sitting-standing/direction-is-not-evidence {:state :shown :token :from-value}
   ;; ⚠ A TOKEN AND NOT `:from-value`, AND THE REASON IS THIS PAGE ITSELF. The
   ;; model's word here is `reference`, and `:from-value` searches the WHOLE page
   ;; text — which on the method view includes the census's own list of about
   ;; eighty qualified quantity names, several of them containing the substring
   ;; `reference` (`spine.lumbar-check/reference-force-range-n` among them). So the
   ;; probe was satisfied by the census reporting that something was hidden.
   ;; Measured 2026-09-09 by replacing the cell with a Japanese phrase: still
   ;; `:found`.
   ;;
   ;; A token is searched in VALUE-BEARING elements only, and the census's list is
   ;; a bare string beside a `<strong>` rather than inside one, so it is out of
   ;; reach. The general lesson is shorter: `:from-value` is only as strong as the
   ;; needle is unusual, and a single English word on a page that prints namespaced
   ;; keywords is not unusual.
   :spine.sitting-standing/validated              {:state :shown :token "reference"}
   :spine.sitting-standing/model-validated?       {:state :shown
                                                   :token "この模型は検証されていない"}
   ;; ⚠ THE TWO CROSS-CHECK MAPS THEMSELVES ARE NOT SHOWN, and saying so is more
   ;; honest than claiming them. The page reaches INTO each of them for five
   ;; numbers — model force, reference force, ratio, lordosis, which side of the
   ;; spread — and renders none of the other twenty-odd fields either map carries.
   ;; A `:shown` claim on the whole map would demand every number in it be findable,
   ;; and the constant probe would then be satisfied by a page showing a fraction
   ;; of it if the rest happened to collide. The same reading `:summary/frontal`
   ;; gets: a copy carried along, and nothing renders THAT copy.
   :spine.sitting-standing/sitting                {:state :computed-not-shown}
   :spine.sitting-standing/standing               {:state :computed-not-shown}})

;; --- reading the page ---------------------------------------------------------

(defn- attrs-of [node]
  (let [a (second node)] (when (map? a) a)))

(defn- value-bearing?
  "Is this hiccup element a place a VALUE is shown, as opposed to prose?

  Table cells, the readout figures, and `<strong>`. The distinction is the whole
  point of the probe: `body.innerText` contains the paragraph that explains the
  ground reaction as readily as the figure that states it, and this repo has three
  recorded cases of a check passing on the paragraph after the figure was gone."
  [node]
  (and (vector? node)
       (let [tag (first node)
             cls (str (:class (attrs-of node)))]
         (or (contains? #{:td :th :strong} tag)
             ;; `suji-readout-item` as well as `suji-figure`: the item is the
             ;; figure AND its label, and a small integer count — three refusals,
             ;; four antagonists — cannot be probed by its digits. `4` matches
             ;; `4.0`, `14` and `40` somewhere on any page. What can be probed is
             ;; that the figure exists, and the figure's own label is the only text
             ;; that disappears with it.
             (str/includes? cls "suji-figure")
             (str/includes? cls "suji-readout-item")))))

(defn- text-of
  "Every string under a node, joined — a cell's whole reading, plus the
  destination of a link.

  AN HREF IS SHOWN, even though it is not text. A citation rendered as
  `<a href=\"…pdf\">本文</a>` puts the paper's URL in front of the reader — it is
  what they follow, and what they see on hover — and the model is where it came
  from. Reading only the child strings reported `:spine.lumbar-check/url` as a
  quantity the page does not carry, which would have been answered either by
  reclassifying a link as absent or by printing a URL as body text to satisfy a
  probe. Both are the probe deciding the design."
  [node]
  (cond
    (string? node) node
    (number? node) (str node)
    (vector? node) (let [href (:href (attrs-of node))
                         inner (str/join " " (keep text-of (rest node)))]
                     (if href (str inner " " href) inner))
    (seq? node) (str/join " " (keep text-of node))
    :else nil))

(defn value-texts
  "The reading of every value-bearing element in a hiccup tree."
  [node]
  (cond
    (vector? node)
    (into (if (value-bearing? node)
            (let [t (text-of node)] (if (str/blank? t) #{} #{t}))
            #{})
          (mapcat value-texts)
          (rest node))

    (seq? node) (into #{} (mapcat value-texts) node)
    :else #{}))

;; --- does a value appear? -------------------------------------------------------

(def ^:private needle-forms
  "The ways this app prints a number, as `[scale decimals]`.

  Not every way a number COULD be printed — every way this app does, one entry per
  emit site. Widening this list weakens the probe rather than strengthening it:
  each extra form is another chance for an unrelated figure on the page to answer
  for the one being looked for.

    [1 0..3]     `fmt-fixed` at nought to three places — forces, moments, %MVC,
                 and the cross-check ratios, which are the three-place ones.
    [1000 1]     metres as millimetres: `coeff-label` and `arms-label`.
    [1e6 3]      the KKT price, which is of order 1e-4 and printed in millionths.
    [1e12 3]     the coupled residual, in picoNewton-metres. At 1e-11 N·m every
                 fixed-decimal rendering in newton-metres is `0.00`, so the page
                 scales it and so must anything looking for it."
  [[1.0 0] [1.0 1] [1.0 2] [1.0 3] [1000.0 1] [1.0e6 3] [1.0e12 3]])

(def ^:private digits
  "The ten digit characters, as a set.

  ⚠ NOT a code-point range test. On the JVM a character indexed out of a string
  is a Character and its int is a code point, so 48-to-57 identifies a digit. In
  ClojureScript `nth` on a string yields a one-character STRING and `int` is
  bitwise-or with zero, which coerces that string to the NUMBER it spells: the
  digit four becomes 4, not 52, and the range test is false for every digit there
  is. `contains-number?` then stopped rejecting a match
  inside a longer number, the probe started finding almost anything, and two
  quantities the page really renders came back absent while two it does not came
  back found — on one host and not the other, from source that reads as portable.

  This is the defect this app's README describes `suji` having had, arriving in
  this repo's own code: a `.cljc` file whose JVM tests are green and whose browser
  behaviour is different. It was found by running the suite on the second host on
  the day that host was added."
  #{\0 \1 \2 \3 \4 \5 \6 \7 \8 \9})

(defn- digit? [c] (contains? digits c))

(defn- contains-number?
  "Does `s` contain `needle` as a number rather than as part of a longer one?

  `44` inside `144.2` is not the number 44 on the page, and without this the probe
  would find nearly anything in a table of three-digit forces."
  [s needle]
  (loop [from 0]
    (let [i (str/index-of s needle from)]
      (cond
        (nil? i) false
        (let [before (when (pos? i) (nth s (dec i)))
              after (let [e (+ i (count needle))]
                      (when (< e (count s)) (nth s e)))]
          (and (not (digit? before))
               (not= \. before)
               (not (digit? after))))
        true
        :else (recur (inc i))))))

(defn- significant?
  "A needle worth looking for.

  Two disqualifications, both measured rather than guessed:

  NO DIGIT ABOVE ZERO. `0`, `0.00` and `-0.0` are on every table on the page for
  unrelated reasons, so a probe accepting them would report any quantity rounding
  to zero as shown — which is most of the two-joint moments in a seated posture,
  the exact quantities this census exists to find.

  A SINGLE DIGIT. `2.648 N-m` printed to nought places is `3`, and a lone `3`
  occurs somewhere on a page of three hundred numbers whatever the posture — so it
  is found at BOTH postures and the differential rejects a column that is really
  there. Measured 2026-09-08: four quantities the joint table plainly renders came
  back absent for exactly this reason."
  [s]
  (and (boolean (re-find #"[1-9]" s))
       (<= 2 (count (str/replace s #"[^0-9]" "")))))

(defn- renderings
  "Every way this app would print this number."
  [v]
  (when (number? v)
    (into #{}
          (comp (map (fn [[s d]] (math/fmt-fixed (* s v) d)))
                (filter significant?))
          needle-forms)))

(defn- found? [texts v]
  (boolean (some (fn [r] (some #(contains-number? % r) texts)) (renderings v))))

(defn- numbers-in
  "The numbers a value contains. A quantity is not always a number: `:coeffs` is a
  map of joint to moment arm, `:coupled-residual-nm` a map of joint to error, and
  `:shoulder-per-side` a map of side to moment. Each of those reaches the page as
  its numbers, so each is probed as its numbers."
  [v]
  (cond
    (number? v) [v]
    (map? v) (filter number? (vals v))
    (sequential? v) (filter number? v)
    :else nil))

(defn- probeable [values]
  (into [] (comp (mapcat numbers-in) (filter #(seq (renderings %)))) values))

(defn- normalise
  "Underscore to space, as `simulate-view` prints a muscle name."
  [s]
  (str/replace (str s) "_" " "))

(defn- string-values
  "The distinct strings a quantity takes. A `:muscle-crossing` entry is a vector
  of `[name force]` pairs, so the names are reached rather than the pair printed.

  ⚠ AND A MAP'S VALUES ARE REACHED TOO, for the same reason `numbers-in` reaches
  them: a quantity is not always a scalar. `:parameter-not-in-source` — the input a
  reference's own source does not state — is a map carrying the parameter's name,
  its provenance and the paragraph explaining where the value came from, and every
  one of those is something the page has to carry or it is showing a comparison
  without showing what was assumed to make it. Read shallowly rather than
  recursively: one level is what the model produces, and a deep walk would start
  demanding strings out of structures nobody renders.

  This STRENGTHENS the probe — every needle found here must appear on the page —
  which is why it is safe to widen. Widening `needle-forms` would be the opposite."
  [values]
  (let [strs (fn strs [v]
               (cond
                 (string? v) [v]
                 (map? v) (filter string? (vals v))
                 (sequential? v) (keep #(cond (string? %) %
                                              (sequential? %) (first %))
                                       v)
                 :else nil))]
    (into #{}
          (comp (mapcat strs)
                (filter string?)
                (map normalise)
                (filter #(<= 4 (count %))))
          values)))

(defn- keyword-strings
  "A keyword value as the page writes it — `hip/left`, not `left`. The qualifier is
  half the identity in a bilateral model, so `name` would make the two sides of the
  body indistinguishable.

  Reaches one level into a map, for the reason `string-values` gives: the parameter
  a reference does not state names ITSELF (`pelvic-tilt-deg`) and names where its
  value came from (`cho-2015-stool`), and a page that prints the note without
  printing the provenance has shown the excuse and not the source."
  [values]
  (into #{}
        (comp (mapcat (fn [v] (cond (keyword? v) [v]
                                    (map? v) (filter keyword? (vals v))
                                    (sequential? v) (filter keyword? v)
                                    :else nil)))
              (map #(subs (str %) 1))
              (filter #(<= 4 (count %))))
        values))

(defn- differential
  "Does this quantity's number move on the page when the model moves it?

  THIS IS THE WHOLE PROBE, and it replaced a threshold. Asking only `is the value
  somewhere on the page` cannot separate a rendered column from a coincidence, and
  every fix by threshold — eight tenths of the rows, or a hit rate above a control
  — is a constant nobody can derive. Measured 2026-09-07: at four postures a
  control search for values scaled by 1.37 matched as often as the real values did,
  because a page with three hundred numbers on it answers almost any question of
  the form `is this number here`.

  So the question is asked as a difference instead. Take two postures where the
  quantity takes a value at one that it does not take at the other; that value has
  to be findable at the first and NOT findable at the second. A figure that matched
  by coincidence would match at both — the coincidence does not move when the
  posture does — so the second half is what a coincidence cannot survive.

  Returns `:found`, `:absent`, or `:inconclusive` when no pair of postures gives
  the quantity a value unique to one of them: a constant cannot be probed this way
  and says so rather than guessing."
  [scenes q]
  (let [pairs (for [a scenes b scenes
                    :when (not (identical? a b))
                    :let [va (set (probeable (get-in a [:produced q])))
                          vb (set (probeable (get-in b [:produced q])))
                          only-a (remove vb va)]
                    :when (seq only-a)]
                [a b only-a])]
    (cond
      (empty? pairs) :inconclusive
      (some (fn [[a b only-a]]
              (some (fn [v] (and (found? (:texts a) v)
                                 (not (found? (:texts b) v))))
                    only-a))
            pairs)
      :found
      :else :absent)))

(defn- constant-probe
  "For a quantity the model gives the same value at every posture — a disc area at
  one stature, a published reference force — there is no difference to look for.
  It is enough that every one of its values is on the page, which for a constant is
  as much as can be asked."
  [scenes q]
  (let [vs (into #{} (mapcat #(probeable (get-in % [:produced q]))) scenes)]
    (if (empty? vs)
      :inconclusive
      (if (every? (fn [v] (some #(found? (:texts %) v) scenes)) vs)
        :found
        :absent))))

(defn probe
  "Is quantity `q` on the page, measured across several postures?

  Three shapes, because a quantity reaches a reader in three different ways:

    a number      by `differential` above, falling back to `constant-probe` only
                  when the model never moves it.
    `:from-value` the model's own STRING — a joint name, a refusal note. Looked
                  for anywhere on the page rather than in a value-bearing element:
                  a sentence the model wrote cannot coincide with a sentence a
                  view wrote. It goes stale the day the model rewords, which is
                  the day it should.
    a literal     a boolean or a keyword, whose rendering is a word this app chose.
                  Value-bearing elements only — `座位` is in the paragraph that
                  explains what seated means as readily as in the figure that says
                  it — and it has to be found at SOME posture, so a branch nothing
                  reaches cannot be claimed as shown."
  [scenes q {:keys [token]}]
  (cond
    (= :from-value token)
    (let [needles (into #{}
                        (mapcat (fn [s]
                                  (let [vs (get-in s [:produced q])]
                                    (into (string-values vs) (keyword-strings vs)))))
                        scenes)]
      (cond
        (empty? needles) :inconclusive
        (every? (fn [n] (some (fn [s] (some #(str/includes? (normalise %) n) (:all-texts s)))
                              scenes))
                needles)
        :found
        :else :absent))

    (string? token)
    (if (some (fn [s] (some #(str/includes? % token) (:texts s))) scenes)
      :found
      :absent)

    :else
    (let [d (differential scenes q)]
      (if (= :inconclusive d) (constant-probe scenes q) d))))

;; --- the census ------------------------------------------------------------------

(defn census
  "The three counts and the three lists, from the model rather than from prose.

  Cheap on purpose — one solve, no rendering — because `method-view` calls it on
  every render. The claim that `:shown` is true is not made here; it is made by
  `audit` below, which renders every view at several postures and is a test."
  [body posture solved]
  (let [prod (produced body posture solved)
        unreq (unrequested body posture solved)
        classify (fn [q] (get-in registry [q :state]))
        shown (sort (filter #(= :shown (classify %)) (keys prod)))
        hidden (sort (filter #(= :computed-not-shown (classify %)) (keys prod)))]
    {:produced prod
     :unrequested unreq
     :shown shown
     :computed-not-shown hidden
     :not-computed (sort (keys unreq))
     :unclassified (sort (remove classify (keys prod)))
     :stale-registry-entries (sort (remove (set (keys prod)) (keys registry)))
     :counts {:shown (count shown)
              :computed-not-shown (count hidden)
              :not-computed (count unreq)}}))

(defn scene
  "One posture, solved and rendered, ready for `audit`.

  `render` returns a SEQUENCE of hiccup trees, one per view.

  ⚠ `(seq ...)` and not the collection itself. A vector of trees IS a hiccup
  element as far as `value-texts` is concerned, so it would take the first tree
  for a tag and walk only the rest — silently dropping the simulator, which is
  where most of the figures are. Measured: it reported 30 of 62 shown quantities
  as absent, all of them from the view that had been skipped."
  [body posture solved render]
  (let [tree (seq (render))]
    {:produced (produced body posture solved)
     :texts (value-texts tree)
     :all-texts #{(text-of tree)}}))

(defn audit
  "Every `:shown` claim, probed across several postures.

  Returns `{q verdict}` for the union of everything any posture produced, so a
  quantity that only exists in one configuration is still asked about."
  [scenes]
  (let [qs (into #{} (mapcat #(keys (:produced %))) scenes)]
    (into {}
          (for [q qs
                :let [r (get registry q)]
                :when (= :shown (:state r))]
            [q (probe scenes q r)]))))
