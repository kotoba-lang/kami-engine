(ns sip.session
  "Ghost-space session — the non-combat core loop, as a *pure* state machine.

  This is the deliberate replacement for combat. A Ghost Hacker does not fix
  anyone; they accompany. So there is no enemy, no damage, no failure: every
  input either helps or holds steady — `grace` and the client's `kokoro` (心音)
  never drop below their floor. The only thing measured is how *gently* you were
  present (`grace`), which becomes the Album/figure record — never a pass/fail.

  Four phases (design doc §3.2):
    :observe   — the landscape IS the client's emotion; read it.
    :resonate  — match the breathing rhythm (tempo closeness), don't attack.
    :accompany — choose to stay / ask / be silent (never 'fix' / 'lecture').
    :name      — give the fear a name; it softens. Scene clears → :complete.

  Pure & deterministic (no rng, no clock): trivially testable, and replayable
  from the Datomic/Kotoba log."
  (:require [clojure.string :as str]))

(def phase-order [:observe :resonate :accompany :name :complete])

(defn begin
  "Open a session for `client` (persona id) whose Ghost-space manifests `emotion`
  (a map of emotion→score, sourced from the story-bible emotion analysis). The
  client's heart (`kokoro`) starts low/unsettled; our `grace` starts neutral."
  [client emotion]
  {:client   client
   :emotion  emotion
   :phase    :observe
   :grace    0.5
   :kokoro   (max 0.15 (- 0.45 (* 0.3 (:turbulence emotion 0.0))))
   :insights #{}
   :log      [[:open client]]})

(defn- clamp01 [x] (-> x (max 0.0) (min 1.0)))

(defn- log [s ev] (update s :log conj ev))

;; --- :observe ---------------------------------------------------------------

(defn observe
  "Read the landscape. Surfaces the dominant feeling as an Insight and moves to
  the resonance phase. Reading hurts no one — it only reveals."
  [{:keys [emotion] :as s}]
  (let [dominant (->> emotion (sort-by val >) ffirst)]
    (-> s
        (update :insights conj (or dominant :unspoken))
        (assoc :phase :resonate)
        (log [:observe dominant]))))

;; --- :resonate --------------------------------------------------------------

(defn resonate
  "Breathe with the client. `closeness` ∈ [0,1] is how well the player matched
  the rhythm this beat (1 = in sync). Good matches raise kokoro & grace; a poor
  match simply does nothing — you can always try the next breath. Never drops."
  [s closeness]
  (let [c (clamp01 closeness)
        gain (* 0.12 c)]
    (-> s
        (update :kokoro (comp clamp01 +) gain)
        (update :grace  (comp clamp01 +) (* 0.04 c))
        (log [:resonate c]))))

(defn ready-to-accompany?
  "The client settles enough to be accompanied once their heart steadies a little.
  This is a soft gate, not a wall — there is no timer forcing it."
  [s]
  (>= (:kokoro s) 0.45))

(defn to-accompany [s] (assoc s :phase :accompany))

;; --- :accompany -------------------------------------------------------------

(def accompaniments
  "The available ways to be present. Note what is ABSENT: :fix, :solve, :lecture.
  Tamaki's whole practice is that none of those belong here."
  {:stay    {:kokoro 0.10 :grace 0.10 :insight :togetherness}
   :ask     {:kokoro 0.06 :grace 0.08 :insight :the-question}
   :silence {:kokoro 0.08 :grace 0.12 :insight :held-space}})

(defn accompany
  "Choose `how` ∈ #{:stay :ask :silence}. Each gently raises the client's heart
  and your grace, and surfaces a small insight. Repeatable."
  [s how]
  (if-let [{:keys [kokoro grace insight]} (accompaniments how)]
    (-> s
        (update :kokoro (comp clamp01 +) kokoro)
        (update :grace  (comp clamp01 +) grace)
        (update :insights conj insight)
        (log [:accompany how]))
    s))

(defn ready-to-name? [s] (>= (:kokoro s) 0.7))

(defn to-name [s] (assoc s :phase :name))

;; --- :name ------------------------------------------------------------------

(defn name-core
  "Give the fear at the heart of the Ghost-space a name. Naming softens it: the
  landscape clears, the heart comes to rest, the session completes."
  [s nm]
  (let [nm (str/trim (str nm))]
    (if (str/blank? nm)
      s
      (-> s
          (assoc :named nm :phase :complete)
          (update :kokoro (comp clamp01 +) 0.2)
          (update :grace  (comp clamp01 +) 0.1)
          (log [:name nm])))))

(defn complete? [s] (= :complete (:phase s)))

;; --- reward (no gems, no xp — only Insight + Bond + a gentle memory) ---------

(defn outcome
  "What the session leaves behind. `:grace` becomes the Album record; `:insights`
  feed the Insight Web; `:bond` is the co-presence credited to the Ghost Agent.
  There is no currency and no failure path."
  [{:keys [client grace kokoro insights named] :as _s}]
  {:client   client
   :grace    grace
   :kokoro   kokoro
   :insights (vec insights)
   :named    named
   ;; Math/round already yields a long on the JVM and an integer-valued number
   ;; in cljs; no `long` coercion (which cljs lacks) needed.
   :bond     (Math/round (* 10.0 grace))})
