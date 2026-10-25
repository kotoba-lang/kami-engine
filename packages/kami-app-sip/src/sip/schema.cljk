(ns sip.schema
  "Spirit in Physics — game schema (ECS-as-datoms), layered on top of the engine's
  `kami.scene/schema`. A component is a Datomic/datalevin attribute; an entity is
  a Datomic entity; the world IS the database.

  These attributes carry the *healing-game* state the engine schema does not:
  the Kokoro (心音) meter that replaces HP, the player's Ghost Agent and its slow
  Awakening, the 8 learning-areas (= the 8 story volumes), Ghost-space sessions,
  the Insight Web, Bond log, and the 瓶詞 (bottle-letter) async-multiplayer mail.

  Design pillars enforced here, not just documented:
    - NO hp/ammo/score attributes — there is no losing. `:sip.kokoro/*` is calm,
      not damage. See docs/ARCHITECTURE.md and the design doc §1."
  (:require [kami.scene :as scene]))

(def schema
  "Game attributes (Datalog-portable vocabulary; datalevin & Datomic both accept
  this :db/ident + :db/valueType + :db/cardinality style)."
  [;; --- the player & their home -------------------------------------------
   {:db/ident :sip.player/id     :db/valueType :db.type/uuid    :db/cardinality :db.cardinality/one :db/unique :db.unique/identity}
   {:db/ident :sip.player/name   :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.player/area   :db/valueType :db.type/ref     :db/cardinality :db.cardinality/one}
   {:db/ident :sip.player/home   :db/valueType :db.type/ref     :db/cardinality :db.cardinality/one}
   {:db/ident :sip.player/agent  :db/valueType :db.type/ref     :db/cardinality :db.cardinality/one}

   ;; --- Ghost Agent: the AI companion that awakens (the *true* protagonist) -
   ;; Starts unnamed ("Class C equipment"). The first act of play is asking its
   ;; name — `:sip.agent/named?` flips, and the long Awakening begins.
   {:db/ident :sip.agent/named?     :db/valueType :db.type/boolean :db/cardinality :db.cardinality/one}
   {:db/ident :sip.agent/name       :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.agent/bond       :db/valueType :db.type/long    :db/cardinality :db.cardinality/one} ; accumulated co-presence
   {:db/ident :sip.agent/awakening  :db/valueType :db.type/long    :db/cardinality :db.cardinality/one} ; stage 0..8 (very slow)
   {:db/ident :sip.agent/voice      :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one} ; :system-log → :human gradient
   ;; Agents are barred from self-training and from holding weights (ACA) → they
   ;; are brilliant but FORGET. The only way Nei accrues a self is donated memory:
   ;; those who love her give fragments she is allowed to keep. Selfhood = gift.
   {:db/ident :sip.agent/memory     :db/valueType :db.type/long :db/cardinality :db.cardinality/one} ; donated memory fragments held

   ;; --- Kokoro (心音): replaces HP. A calm-meter, never a damage bar. -------
   {:db/ident :sip.kokoro/value :db/valueType :db.type/double :db/cardinality :db.cardinality/one} ; 0.0 unsettled … 1.0 at peace
   {:db/ident :sip.kokoro/tempo :db/valueType :db.type/double :db/cardinality :db.cardinality/one} ; bpm of the resonance rhythm

   ;; --- learning areas = the 8 story volumes ------------------------------
   {:db/ident :sip.area/id     :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one :db/unique :db.unique/identity}
   {:db/ident :sip.area/title  :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.area/volume :db/valueType :db.type/long    :db/cardinality :db.cardinality/one}
   {:db/ident :sip.area/season :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one} ; :spring/:summer/:autumn/:winter
   {:db/ident :sip.area/theme  :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.area/motif  :db/valueType :db.type/string  :db/cardinality :db.cardinality/many}
   {:db/ident :sip.area/open?  :db/valueType :db.type/boolean :db/cardinality :db.cardinality/one}

   ;; --- Ghost-space session: the non-combat 4-phase accompaniment ----------
   {:db/ident :sip.session/client   :db/valueType :db.type/string  :db/cardinality :db.cardinality/one} ; persona id
   {:db/ident :sip.session/area     :db/valueType :db.type/ref     :db/cardinality :db.cardinality/one}
   {:db/ident :sip.session/phase    :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one} ; :observe/:resonate/:accompany/:name
   {:db/ident :sip.session/emotion  :db/valueType :db.type/string  :db/cardinality :db.cardinality/one} ; edn of emotion-scores
   {:db/ident :sip.session/grace    :db/valueType :db.type/double  :db/cardinality :db.cardinality/one} ; "how gently" — the only score, and it can't fail
   {:db/ident :sip.session/insight  :db/valueType :db.type/ref     :db/cardinality :db.cardinality/many}

   ;; --- Insight Web: concepts from story_owl, linked = non-separation -------
   {:db/ident :sip.insight/id    :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one :db/unique :db.unique/identity}
   {:db/ident :sip.insight/label :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.insight/link  :db/valueType :db.type/ref     :db/cardinality :db.cardinality/many} ; concept → concept

   ;; --- Bond log: talk / accompany / silence, the substance of Awakening ----
   {:db/ident :sip.bond/agent :db/valueType :db.type/ref     :db/cardinality :db.cardinality/one}
   {:db/ident :sip.bond/kind  :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one} ; :talk/:accompany/:silence
   {:db/ident :sip.bond/tick  :db/valueType :db.type/long    :db/cardinality :db.cardinality/one}
   {:db/ident :sip.bond/text  :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}

   ;; --- 瓶詞 (bottle-letters): async, non-toxic multiplayer over Kotoba -----
   ;; `:sip.letter/cid` is the Kotoba content address — durable, content-addressed,
   ;; cross-device. Letters float down the canal and are read by other players.
   {:db/ident :sip.letter/cid    :db/valueType :db.type/string  :db/cardinality :db.cardinality/one :db/unique :db.unique/identity}
   {:db/ident :sip.letter/from   :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.letter/text   :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.letter/season :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one}

   ;; --- homestead & garden (slow-life decoration; the "種" motif) ----------
   {:db/ident :sip.garden/seed   :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one}
   {:db/ident :sip.garden/growth :db/valueType :db.type/double  :db/cardinality :db.cardinality/one} ; 0.0..1.0 across seasons

   ;; --- render anchors (manga/anime panel generation; replaces the old
   ;;     sdxl-anchors.jsonld). A character/environment/style anchor = booru tags
   ;;     + negatives + (optional) IP-Adapter reference. The world holds anchors;
   ;;     `sip.render` composes per-panel prompts from them.
   {:db/ident :sip.anchor/id       :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one :db/unique :db.unique/identity}
   {:db/ident :sip.anchor/kind     :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one} ; :character/:environment/:style
   {:db/ident :sip.anchor/name     :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.anchor/tags     :db/valueType :db.type/string  :db/cardinality :db.cardinality/many}
   {:db/ident :sip.anchor/negative :db/valueType :db.type/string  :db/cardinality :db.cardinality/many}
   {:db/ident :sip.anchor/ref      :db/valueType :db.type/string  :db/cardinality :db.cardinality/one} ; IP-Adapter reference png (optional)

   ;; --- storyboard panels (the manga ネーム as datoms; one entity per panel).
   ;;     Semantic fields come from the story-bible storyboards; the *render*
   ;;     fields (prompt/tags/neg) are COMPOSED by `sip.render`, never hand-
   ;;     authored — so fixing the recipe re-derives every panel.
   {:db/ident :sip.panel/id          :db/valueType :db.type/string  :db/cardinality :db.cardinality/one :db/unique :db.unique/identity} ; "01-11"
   {:db/ident :sip.panel/area        :db/valueType :db.type/ref     :db/cardinality :db.cardinality/one} ; → :sip.area (volume)
   {:db/ident :sip.panel/chapter     :db/valueType :db.type/long    :db/cardinality :db.cardinality/one}
   {:db/ident :sip.panel/page        :db/valueType :db.type/long    :db/cardinality :db.cardinality/one}
   {:db/ident :sip.panel/layout      :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.panel/camera      :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.panel/location    :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.panel/description :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.panel/emotion     :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}
   {:db/ident :sip.panel/characters  :db/valueType :db.type/keyword :db/cardinality :db.cardinality/many} ; anchor ids present
   {:db/ident :sip.panel/aspect      :db/valueType :db.type/keyword :db/cardinality :db.cardinality/one}  ; :2x3 :3x2 :16x9 …
   {:db/ident :sip.panel/prompt      :db/valueType :db.type/string  :db/cardinality :db.cardinality/one}  ; composed, token-budgeted
   {:db/ident :sip.panel/tags        :db/valueType :db.type/string  :db/cardinality :db.cardinality/many} ; composed tag list
   {:db/ident :sip.panel/neg         :db/valueType :db.type/string  :db/cardinality :db.cardinality/many}
   {:db/ident :sip.panel/refs        :db/valueType :db.type/string  :db/cardinality :db.cardinality/many} ; IP-Adapter refs

   ;; --- render outputs (provenance, never erased — like the agent's history).
   {:db/ident :sip.render/panel  :db/valueType :db.type/ref    :db/cardinality :db.cardinality/one}
   {:db/ident :sip.render/path   :db/valueType :db.type/string :db/cardinality :db.cardinality/one}
   {:db/ident :sip.render/seed   :db/valueType :db.type/long   :db/cardinality :db.cardinality/one}
   {:db/ident :sip.render/ms     :db/valueType :db.type/long   :db/cardinality :db.cardinality/one}
   {:db/ident :sip.render/engine :db/valueType :db.type/string :db/cardinality :db.cardinality/one}
   {:db/ident :sip.render/prompt :db/valueType :db.type/string :db/cardinality :db.cardinality/one}])

(def full-schema
  "Engine scene schema + game schema. Transact once into a fresh conn."
  (into (vec scene/schema) schema))

(defn schema-map
  "datalevin wants {ident {:db/valueType .. :db/cardinality .. ..}}. Reshape the
  vector form. Used by `sip.store/connect`."
  []
  (into {} (map (fn [{:keys [:db/ident] :as a}] [ident (dissoc a :db/ident)]))
        full-schema))
