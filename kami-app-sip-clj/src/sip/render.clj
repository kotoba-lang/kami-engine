(ns sip.render
  "Spirit in Physics render — the WORK-SPECIFIC facade over the generic mangaka
  render commons (`kami.mangaka.render`, ADR-2606282100).

  The generic half — STYLE-FIRST CLIP-77-budgeted composition + the image-gen
  HTTP client + comic dims — now lives in `kami-mangaka-render-clj`. This ns
  keeps only what is *about Spirit in Physics*: the Nei light/embodied focal
  rule, the 静寂→serene emotion table, the 事務所→:schwa-office location map, and
  the datalevin provenance/datom layer. We inject the three work mappers into
  `km/compose`; everything else re-derives from the commons.

  Two render facts (verified 2026-06-18) live in the commons, not here:
  CLIP 77-token truncation (own the whole window, style-first) and IP-Adapter
  returning noise on AnimagineXL 4.0 + MPS (render tag-only, keep refs as meta)."
  (:require [clojure.string :as str]
            [kami.mangaka.render :as km]
            [sip.store :as store]
            [sip.storyboard :as sb]
            [sip.lore :as lore]))

;; ---------------------------------------------------------------------------
;; Anchors (SIP's anchor bible, loaded via the commons)
;; ---------------------------------------------------------------------------

(defn anchors
  "Load the SIP EDN anchor bible from the IP repo (resources/render-anchors.edn
  under $SIP_IP_ROOT, see `sip.lore/ip-root`) — it's SIP content, not engine
  code, so it lives with the rest of the story data, not in this app's jar."
  []
  (km/read-anchors (str lore/ip-root "/resources/render-anchors.edn")))

;; ---------------------------------------------------------------------------
;; Composition — SIP-specific mappers injected into the generic commons
;; ---------------------------------------------------------------------------

(def aspect->dims km/aspect->dims)

(def ^:private nei-light-cues
  ["ポッド" "光体" "発光" "半透明" "粒子" "光の" "明滅" "覚醒" "起動"
   "pod" "glow" "translucent" "luminous" "light field" "emergence" "awaken"])

(defn- nei-form
  "Pick Nei's embodied (:nei) vs light-figure (:nei-light) anchor from the panel's
  prose. Light-form for pod/awakening/abstract beats; embodied otherwise."
  [{:keys [description emotion colorNote location]}]
  (let [blob (str/lower-case (str description " " emotion " " colorNote " " location))]
    (if (some #(str/includes? blob (str/lower-case %)) nei-light-cues) :nei-light :nei)))

(defn focal-character
  "パネル1キャラクター — one character per panel. The first dialogue speaker if
  they're in the cast (natural shot / reverse-shot), else the first listed.
  Applies the nei→nei-light swap for awakening/ghost-space beats. nil if none.
  This is SIP's `:focal-character` mapper for `km/compose`."
  [panel]
  (let [chars (:characters panel)
        sp    (some-> (first (:dialogue panel)) :speaker str str/lower-case keyword)
        pick  (or (some #{sp} chars) (first chars))]
    (when pick (if (= pick :nei) (nei-form panel) pick))))

(def ^:private emotion->tag
  {"静寂" "serene" "静けさ" "serene" "目覚め" "awakening mood"
   "温もり" "warm mood" "温かさ" "warm mood" "やわらか" "tender"
   "戸惑い" "puzzled expression" "問い" "questioning expression"
   "歓び" "joyful expression" "喜び" "joyful expression" "受容" "gentle expression"
   "緊張" "tense" "接近" "intimate" "非分離" "intimate"
   "見守り" "watchful expression" "充足" "content expression"
   "発見" "wonder" "驚き" "surprised expression" "歓喜" "joyful expression"
   "ためらい" "hesitant expression" "余韻" "lingering quiet"})

(defn mood-tags
  "SIP's `:emotion->tags` mapper: Japanese emotion prose → ≤2 booru mood tags."
  [emotion]
  (->> emotion->tag
       (keep (fn [[jp en]] (when (str/includes? (str emotion) jp) en)))
       distinct (take 2) vec))

(defn env-key
  "SIP's `:location->env` mapper: storyboard location prose → environment anchor
  key. The character-named keys (schwa-office / tamaki-apartment) and the
  water-city default are SIP-specific — they live here, never in the commons."
  [location]
  (let [l (str location)]
    (cond
      (re-find #"事務所|office|デスク|オフィス" l) :schwa-office
      (re-find #"キッチン|アパート|kitchen|apartment|テーブル|窓際|室内" l) :tamaki-apartment
      (re-find #"遊歩道|並木|沿い|walkway|path|道" l) :canal-path
      (re-find #"運河|水の都|canal|水面" l) :water-city
      :else :water-city)))

(defn compose
  "Panel map + SIP anchors → render spec, by injecting SIP's three work mappers
  (focal-character / env-key / mood-tags) into the generic `km/compose`. The
  STYLE-FIRST word budgeting + tag grouping all live in the commons now."
  [{:keys [anchors panel]}]
  (km/compose {:anchors anchors
               :panel panel
               :mappers {:focal-character focal-character
                         :location->env   env-key
                         :emotion->tags   mood-tags}}))

;; image-gen HTTP client lives in the commons; re-export for callers/CLI.
(def render! km/render!)

;; ---------------------------------------------------------------------------
;; Datoms — anchors + panels into the world (datalevin)
;; ---------------------------------------------------------------------------

(defn anchors-tx
  "EDN anchor bible → :sip.anchor/* datoms (characters + environments)."
  [an]
  (concat
   (for [[id {:keys [name tags negative ref]}] (:characters an)]
     (cond-> {:sip.anchor/id id :sip.anchor/kind :character
              :sip.anchor/tags (vec tags) :sip.anchor/negative (vec negative)}
       name (assoc :sip.anchor/name name)
       ref  (assoc :sip.anchor/ref ref)))
   (for [[id {:keys [tags]}] (:environments an)]
     {:sip.anchor/id id :sip.anchor/kind :environment :sip.anchor/tags (vec tags)})))

(defn panel-tx
  "One storyboard panel + its composed spec → a :sip.panel/* datom map."
  [an panel]
  (let [{:keys [tags prompt neg refs aspect]} (compose {:anchors an :panel panel})]
    (cond-> {:sip.panel/id (:id panel)
             :sip.panel/area [:sip.area/id (:area panel)]
             :sip.panel/chapter (long (:chapter panel))
             :sip.panel/aspect aspect
             :sip.panel/prompt prompt
             :sip.panel/tags tags
             :sip.panel/neg neg
             :sip.panel/characters (mapv keyword (:characters panel))}
      (:page panel)        (assoc :sip.panel/page (:page panel))
      (:layout panel)      (assoc :sip.panel/layout (:layout panel))
      (:camera panel)      (assoc :sip.panel/camera (:camera panel))
      (:location panel)    (assoc :sip.panel/location (:location panel))
      (:description panel) (assoc :sip.panel/description (:description panel))
      (:emotion panel)     (assoc :sip.panel/emotion (:emotion panel))
      (seq refs)           (assoc :sip.panel/refs refs))))

(defn- areas-tx
  "The 8 learning-areas (= story volumes) as :sip.area datoms, so panels can
  resolve their :sip.panel/area lookup-ref. Mirrors `sip.world/game-tx`."
  []
  (for [{:keys [id title volume season theme motifs]} (lore/volumes)]
    {:sip.area/id id :sip.area/title title :sip.area/volume volume
     :sip.area/season season :sip.area/theme theme
     :sip.area/motif (vec motifs) :sip.area/open? (= volume 1)}))

(defn load!
  "Connect the world at `dir`, transact areas + anchors + all storyboard panels
  (each with its composed prompt). Returns {:areas n :anchors n :panels n}."
  [dir]
  (let [an   (anchors)
        conn (store/connect dir)
        ps   (sb/panels)]
    (store/transact! conn (vec (areas-tx)))
    (store/transact! conn (vec (anchors-tx an)))
    (store/transact! conn (mapv #(panel-tx an %) ps))
    {:areas (count (areas-tx)) :anchors (count (:characters an)) :panels (count ps)}))

;; ---------------------------------------------------------------------------
;; Batch render + provenance (render outputs are datoms, never erased)
;; ---------------------------------------------------------------------------

(defn render-one!
  "Compose + render panel `p` to `out-dir/<id>.png`, then record a
  `:sip.render/*` provenance datom against the panel. Returns the render map."
  [conn an p out-dir & {:keys [seed steps] :or {seed 4242 steps 28}}]
  (let [spec (compose {:anchors an :panel p})
        out  (str out-dir "/" (:id p) ".png")
        r    (render! spec out :seed seed :steps steps)]
    (store/transact! conn
      [{:sip.render/panel  [:sip.panel/id (:id p)]
        :sip.render/path   (:path r)
        :sip.render/seed   (long (or (:seed r) seed))
        :sip.render/ms     (long (or (:ms r) 0))
        :sip.render/engine (str "image-gen " (:model an "animagine-xl-4.0"))
        :sip.render/prompt (:prompt spec)}])
    (assoc r :id (:id p))))

(defn render-all!
  "Connect the world at `dir`, ensure anchors+panels are loaded, then render every
  panel to `out-dir` recording provenance. `:only` limits to a panel-id prefix
  (e.g. \"02-\" for one chapter); `:limit` caps the count. Returns a summary."
  [dir out-dir & {:keys [only limit seed steps]}]
  (let [an   (anchors)
        conn (store/connect dir)
        ps   (cond->> (sb/panels)
               only  (filter #(str/starts-with? (str (:id %)) only))
               limit (take limit))]
    (println "rendering" (count ps) "panel(s) →" out-dir)
    (let [done (doall
                (for [p ps]
                  (try
                    (let [r (render-one! conn an p out-dir
                                         :seed (or seed 4242) :steps (or steps 28))]
                      (println "  ✓" (:id p) "→" (:path r) (str "(" (:ms r) "ms)"))
                      r)
                    (catch Exception e
                      (println "  ✗" (:id p) (.getMessage e)) nil))))]
      {:rendered (count (filter some? done)) :total (count ps) :out out-dir})))

;; ---------------------------------------------------------------------------
;; CLI
;; ---------------------------------------------------------------------------

(defn -main
  "  compose <panel-id>            — print the composed prompt (no server, no DB)
   render  <panel-id> [out.png]  — compose + render via image-gen /generate
   load    [dir]                 — transact anchors + all panels into datalevin"
  [& [cmd a b]]
  (case cmd
    "compose"
    (let [an (anchors) p (sb/panel-by-id a) spec (compose {:anchors an :panel p})]
      (println "panel  " a "→ focal" (focal-character p) "aspect" (:aspect spec) (:dims spec))
      (println "words  " (count (str/split (:prompt spec) #"\s+")))
      (println "prompt " (:prompt spec))
      (println "neg    " (str/join ", " (take 8 (:neg spec)) ) "…"))

    "render"
    (let [an (anchors) p (sb/panel-by-id a)
          spec (compose {:anchors an :panel p})
          out (or b (str (System/getProperty "java.io.tmpdir") "/sip-render-" a ".png"))]
      (println "rendering" a "→" out)
      (println "prompt:" (:prompt spec))
      (let [r (render! spec out :seed 4242)]
        (println "done:" (:path r) "seed" (:seed r) (:ms r) "ms")))

    "load"
    (let [dir (or a (str (System/getProperty "java.io.tmpdir") "/sip-world"))]
      (println "loading anchors + panels into" dir)
      (println (load! dir)))

    "render-all"
    ;; render-all [only-prefix] [out-dir] — batch render + record provenance
    (let [out (or b (str (or (System/getenv "SIP_IP_ROOT") "../../260208-spirit-in-physics")
                         "/resources/images/sip-render"))
          dir (str (System/getProperty "java.io.tmpdir") "/sip-world")]
      (println (render-all! dir out :only (when (and a (not= a "all")) a))))

    (println "usage: compose <id> | render <id> [out] | render-all [prefix] [dir] | load [dir]"))
  (flush))
