(ns kami.gameplay.damage
  "The damage pipeline — one path from \"a hit happened\" to \"the world changed\".

  Unreal routes every health change through a GameplayEffect for a reason:
  once damage can be applied from more than one place, the rules (team damage,
  armour, shields, the downed state, who gets the kill credit) start to
  disagree between the places. This engine is about to gain three damage
  sources at once — bullets, projectiles and the storm — so the pipeline comes
  first and the sources call into it.

  `apply-damage` returns both the new world and an **event**. The event is not
  decoration: the HUD hit-marker, the kill feed, the damage-dealt statistic and
  the audio trigger all read it, and a source that mutated health directly
  would silently produce none of them."
  (:require [kami.gameplay.attributes :as attr]))

(def same-team?
  "Team protection, defined once in `kami.gameplay.attributes` so the damage
  pipeline, perception and AI targeting cannot drift apart about who counts as
  friendly."
  attr/same-team?)

(defn absorb
  "Split `amount` across shield, then armour-reduced health.

  Returns `{:shield-loss :health-loss :absorbed :overkill}`. Shield takes the
  hit first and at face value; armour only reduces what reaches health. That
  ordering is a design decision worth stating: it makes shield a flat buffer
  and armour a percentage, so stacking both is strong but not multiplicative
  against the same point of damage."
  [target amount]
  (let [amount (max 0.0 (double amount))
        shield (attr/get target :shield)
        shield-loss (min shield amount)
        through (- amount shield-loss)
        armor (attr/get target :armor)
        to-health (* through (- 1.0 armor))
        health (attr/get target :health)
        health-loss (min health to-health)]
    {:shield-loss shield-loss
     :health-loss health-loss
     :absorbed (+ shield-loss health-loss)
     :overkill (- to-health health-loss)}))

(defn- credit-instigator
  [world instigator-id dealt killed?]
  (if (and instigator-id (get-in world [:entities instigator-id]))
    (update-in world [:entities instigator-id]
               (fn [e]
                 (cond-> (attr/update-attr e :damage-dealt + dealt)
                   killed? (attr/update-attr :kills + 1.0))))
    world))

(defn apply-damage
  "Apply `amount` to `target-id`. Returns `[world' event]`.

  `event` is always a map and always has `:kind`; a refused hit is reported as
  `:blocked` with a `:reason` rather than swallowed, because a shot that
  silently does nothing is indistinguishable from a bug in the hit detection
  that produced it.

  `:source` names what did it (`:bullet`, `:projectile`, `:storm`, `:fall`),
  which is what lets the kill feed say how someone died."
  [world target-id {:keys [amount instigator source zone weapon at-ms]
                    :or {amount 0.0 source :unknown zone :body}}]
  (let [target (get-in world [:entities target-id])
        inst (when instigator (get-in world [:entities instigator]))]
    (cond
      (nil? target)
      [world {:kind :blocked :reason :no-target :target target-id :source source}]

      (not (attr/alive? target))
      [world {:kind :blocked :reason :already-dead :target target-id :source source}]

      (and inst (same-team? inst target))
      [world {:kind :blocked :reason :friendly-fire :target target-id
              :instigator instigator :source source}]

      (<= (double amount) 0.0)
      [world {:kind :blocked :reason :zero-damage :target target-id :source source}]

      :else
      (let [{:keys [shield-loss health-loss absorbed]} (absorb target amount)
            target' (-> target
                        (attr/update-attr :shield - shield-loss)
                        (attr/update-attr :health - health-loss))
            killed? (not (pos? (attr/get target' :health)))
            target' (if killed?
                      (-> target' (attr/set :alive 0.0) (attr/set :downed 0.0))
                      target')
            world' (-> world
                       (assoc-in [:entities target-id] target')
                       (credit-instigator instigator absorbed killed?))]
        [world'
         {:kind (if killed? :kill :hit)
          :target target-id
          :instigator instigator
          :source source
          :zone zone
          :weapon weapon
          :at-ms at-ms
          :amount absorbed
          :shield-loss shield-loss
          :health-loss health-loss
          :remaining-health (attr/get target' :health)
          :remaining-shield (attr/get target' :shield)}]))))

(defn apply-many
  "Apply a sequence of damage descriptors in order.

  Returns `[world' events]`. Order matters and is preserved: two shots that
  together kill must credit the second one, and a storm tick that lands on the
  same frame as a bullet must resolve after it."
  [world descriptors]
  (reduce (fn [[w evs] {:keys [target] :as d}]
            (let [[w' ev] (apply-damage w target d)]
              [w' (conj evs ev)]))
          [world []]
          descriptors))

(defn heal
  "Restore health (never above max) and report it. Returns `[world' event]`.

  Refuses to heal the dead — a consumable used on a corpse is a bug in the
  caller, and reviving is a different mechanic with different rules."
  [world target-id amount]
  (let [target (get-in world [:entities target-id])]
    (if-not (attr/alive? target)
      [world {:kind :blocked :reason :not-alive :target target-id :source :heal}]
      (let [before (attr/get target :health)
            target' (attr/update-attr target :health + (max 0.0 (double amount)))
            gained (- (attr/get target' :health) before)]
        [(assoc-in world [:entities target-id] target')
         {:kind :heal :target target-id :amount gained
          :remaining-health (attr/get target' :health)}]))))

(defn add-shield
  "Add shield (never above max) and report it. Returns `[world' event]`."
  [world target-id amount]
  (let [target (get-in world [:entities target-id])]
    (if-not (attr/alive? target)
      [world {:kind :blocked :reason :not-alive :target target-id :source :shield}]
      (let [before (attr/get target :shield)
            target' (attr/update-attr target :shield + (max 0.0 (double amount)))
            gained (- (attr/get target' :shield) before)]
        [(assoc-in world [:entities target-id] target')
         {:kind :shield :target target-id :amount gained
          :remaining-shield (attr/get target' :shield)}]))))
