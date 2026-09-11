(ns kami.gameplay.world
  "The composed step: `(step world dt inputs) -> [world' events]`.

  This is the parity oracle. When the Rust workspace left this repository it
  took `battle_royale.rs` with it, and the EDN tables it used to validate
  against — weapons, storm, consumables — were left describing a runtime that no
  longer existed. Their headers still say `stays the builtin fallback AND the
  parity oracle`, pointing at a file that is gone. This namespace is the
  replacement, and being portable `.cljc` it is an oracle every host can run
  rather than one only a native build could.

  Everything is a pure function of `(world, dt, inputs)`. There is no clock read,
  no atom, no global RNG. Two consequences the engine needs:

  * a match replays exactly from a seed and an input log, which is what makes
    lockstep co-op and server reconciliation possible at all;
  * a test can assert on a whole match instead of on a frame.

  System order is fixed and load-bearing, and is asserted by name in the tests:
  reload before firing (so a reload that completes this tick arms the weapon),
  fire before movement (so shots resolve against the positions the shooter
  actually saw), movement before the storm (so a player who reached safety this
  tick is safe this tick)."
  (:require [kami.gameplay.vec3 :as v]
            [kami.gameplay.rng :as rng]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.aim :as aim]
            [kami.gameplay.weapon :as weapon]
            [kami.gameplay.ballistics :as ball]
            [kami.gameplay.damage :as damage]
            [kami.gameplay.zone :as zone]
            [kami.gameplay.perception :as perc]
            [kami.gameplay.ai :as ai]
            [kami.gameplay.match :as match]))

(def system-order
  "The fixed system order. Exported so a test can assert it rather than trust a
  comment, and so a host that reimplements the loop has something to conform to."
  [:weapon-upkeep :ai :player-intent :fire :integrate :zone :deaths :match])

(defn make-entity
  "A gameplay entity: a tag, a position, a velocity and an attribute map."
  [{:keys [tag pos vel attrs] :or {tag "actor" pos [0.0 0.0 0.0] vel [0.0 0.0 0.0]}}]
  {:tag tag :pos (vec (map double pos)) :vel (vec (map double vel))
   :attrs (or attrs {}) :backpack {}})

(defn make-world
  "Build a world from a scene map plus the three canonical data tables.

  `:scene` is the authored EDN the game ships; `:weapons`, `:storm` and
  `:consumables` are the shared tables. Keeping them separate is the point —
  the game authors composition, the engine owns rules."
  [{:keys [scene entities weapons storm consumables seed]
    :or {scene {} entities {} seed 20260822}}]
  (let [wt (weapon/load-table (or weapons []))
        rs (rng/seed seed)
        zplan (zone/plan (merge {:phases storm
                                 :start-radius (get-in scene [:zone :start-radius] 1000.0)
                                 :center (get-in scene [:zone :center] [0.0 0.0 0.0])
                                 :rng-seed seed}))]
    {:scene scene
     :entities entities
     :projectiles []
     :pickups []
     :weapons wt
     :consumables (if consumables (vec consumables) [])
     :zone-plan zplan
     :rigs (aim/rig scene)
     :senses (perc/senses scene)
     :ai-profile (ai/profile scene)
     :rng rs
     :clock {:now-ms 0 :elapsed-s 0.0 :tick 0}
     :match {:state :pending :entrants 0 :next-place 0}}))

(defn zone-now [world]
  (zone/state-at (:zone-plan world) (get-in world [:clock :elapsed-s])))

(defn- candidates
  "The `[id entity]` pairs ballistics may hit."
  [world]
  (seq (:entities world)))

(defn- fire-one
  "Resolve one entity's trigger pull. Returns `[world' events]`."
  [world id now-ms]
  (let [e (get-in world [:entities id])
        w (weapon/equipped (:weapons world) e)
        [res rs'] (ball/resolve-shot
                    (:rng world)
                    {:weapon w
                     :origin (aim/eye-position (:pos e) (attr/get e :eye-height))
                     :dir (aim/look-direction (attr/get e :yaw) (attr/get e :pitch))
                     :shooter id
                     :candidates (candidates world)
                     :ads (attr/get e :ads)
                     :speed (v/length (:vel e))
                     :stance (attr/get e :stance)
                     :now-ms now-ms})
        world (-> world
                  (assoc :rng rs')
                  (update-in [:entities id] #(weapon/consume-shot (:weapons world) % now-ms)))]
    (case (:kind res)
      :hit (let [[world ev] (damage/apply-damage world (:target res)
                                                 {:amount (:damage res)
                                                  :instigator id
                                                  :source :bullet
                                                  :zone (:zone res)
                                                  :weapon (:weapon res)
                                                  :at-ms now-ms})]
             [world [(assoc res :kind :shot) ev]])
      :projectile [(update world :projectiles conj res) [(assoc res :kind :shot-projectile)]]
      [world [(assoc res :kind :shot)]])))

(defn- step-projectiles
  "Advance every projectile and apply the ones that connected."
  [world dt now-ms]
  (reduce
    (fn [[w evs] p]
      (let [r (ball/step-projectile p dt (candidates w))]
        (cond
          (:projectile r) [(update w :projectiles conj (:projectile r)) evs]
          (:hit r) (let [h (:hit r)
                         wpn (weapon/by-index (:weapons w) (:weapon p))
                         mult (* (ball/falloff-multiplier wpn (:travelled h))
                                 (if (= (:zone h) :head)
                                   (double (:headshot-mult wpn 1.0)) 1.0))
                         [w ev] (damage/apply-damage w (:id h)
                                                     {:amount (* (double (:damage wpn)) mult)
                                                      :instigator (:shooter p)
                                                      :source :projectile
                                                      :zone (:zone h)
                                                      :weapon (:weapon p)
                                                      :at-ms now-ms})]
                     [w (conj evs ev)])
          :else [w (conj evs {:kind :projectile-expired :weapon (:weapon p)})])))
    [(assoc world :projectiles []) []]
    (:projectiles world)))

(defn- apply-player-intent
  "Fold one controller's intent into its pawn: look, ADS, stance, movement.

  Movement goes through `aim/move-vector`, so the stick is camera-relative. That
  one call is the difference between driving a character and dragging a cursor."
  [world id {:keys [look-dx look-dy move-x move-y ads? crouch? sprint?]
             :or {look-dx 0.0 look-dy 0.0 move-x 0.0 move-y 0.0}} dt]
  (let [rigs (:rigs world)
        sens (get-in world [:scene :input/look-sensitivity] 0.0032)]
    (update-in world [:entities id]
               (fn [e]
                 (let [e (aim/apply-look e rigs look-dx look-dy sens)
                       e (aim/step-ads e ads? dt 6.0)
                       e (attr/set e :stance (if crouch? 1.0 0.0))
                       base (attr/get e :move-speed)
                       mult (cond (pos? (attr/get e :ads)) (- 1.0 (* 0.45 (attr/get e :ads)))
                                  sprint? 1.45
                                  crouch? 0.55
                                  :else 1.0)
                       dir (aim/move-vector (attr/get e :yaw) move-x move-y)]
                   (assoc e :vel (v/scale dir (* base mult))))))))

(defn- integrate
  "Move every entity by its velocity. `:collide` on the scene, when supplied, is
  a function of `[from to entity]` returning the position actually reached — the
  seam a host with a real collision world plugs into without this namespace
  needing to know what a wall is."
  [world dt]
  (let [collide (get-in world [:scene :collide])]
    (update world :entities
            (fn [ents]
              (reduce-kv
                (fn [acc id e]
                  (let [to (v/add (:pos e) (v/scale (:vel e) dt))
                        to (if collide (collide (:pos e) to e) to)]
                    (assoc acc id (assoc e :pos to))))
                {} ents)))))

(defn step
  "Advance the world by `dt` seconds. Returns `[world' events]`.

  `inputs` maps entity id to a controller intent map (see
  `apply-player-intent`); every entity tagged `bot` that has no entry is driven
  by `kami.gameplay.ai`. `:fire?` in an intent is a request, not a shot — it is
  filtered by `weapon/can-fire?` exactly as a bot's is, so a player and a bot
  cannot get different fire rates out of the same gun."
  [world dt inputs]
  (let [dt (double dt)
        now-ms (+ (get-in world [:clock :now-ms]) (* 1000.0 dt))
        world (-> world
                  (assoc-in [:clock :now-ms] now-ms)
                  (update-in [:clock :elapsed-s] + dt)
                  (update-in [:clock :tick] inc))
        world (if (= :pending (get-in world [:match :state])) (match/begin world) world)
        before-alive (set (match/alive-ids world))
        zone-state (zone-now world)

        ;; 1. weapon upkeep — reloads complete before anything can fire
        world (update world :entities
                      (fn [ents]
                        (reduce-kv (fn [acc id e]
                                     (assoc acc id (weapon/step (:weapons world) e now-ms)))
                                   {} ents)))

        ;; 2. AI decides and writes facing + velocity
        [world ai-fire]
        (reduce
          (fn [[w firing] [id e]]
            (if (or (contains? inputs id) (not= "bot" (:tag e)) (not (attr/alive? e)))
              [w firing]
              (let [d (ai/decide w (:weapons w) id
                                 {:zone zone-state :senses (:senses w)
                                  :prof (:ai-profile w) :now-ms now-ms})
                    [e' rs'] (ai/apply-decision (:rng w) w e d
                                                {:senses (:senses w) :prof (:ai-profile w)
                                                 :now-ms now-ms :dt dt})]
                [(-> w (assoc-in [:entities id] e') (assoc :rng rs'))
                 (if (and (= (:state d) :engage)
                          (weapon/can-fire? (:weapons w) e' now-ms))
                   (conj firing id) firing)])))
          [world []]
          (vec (:entities world)))

        ;; 3. player intent
        world (reduce-kv (fn [w id intent]
                           (if (get-in w [:entities id]) (apply-player-intent w id intent dt) w))
                         world inputs)

        ;; 4. fire — players and bots through the identical gate
        player-fire (keep (fn [[id intent]]
                            (when (and (:fire? intent)
                                       (weapon/can-fire? (:weapons world) (get-in world [:entities id]) now-ms))
                              id))
                          inputs)
        [world shot-events]
        (reduce (fn [[w evs] id]
                  (let [[w' es] (fire-one w id now-ms)] [w' (into evs es)]))
                [world []]
                (concat (sort player-fire) (sort ai-fire)))

        [world proj-events] (step-projectiles world dt now-ms)

        ;; 5. integrate
        world (integrate world dt)

        ;; 6. storm
        zone-state (zone-now world)
        [world storm-events] (damage/apply-many world (zone/tick-damage world zone-state dt))

        ;; 7/8. placements and match end
        world (match/record-deaths world before-alive)
        world (match/resolve-end world)]
    [world (vec (concat shot-events proj-events
                        (remove #(= :blocked (:kind %)) storm-events)))]))

(defn run
  "Step until `:ended` or `max-ticks`, collecting events.

  Returns `{:world :events :ticks :ended? :standings}`. `input-fn` is called with
  `[world tick]` and returns the input map for that tick, which is how a test
  drives a scripted player without a renderer."
  [world {:keys [dt max-ticks input-fn]
          :or {dt (/ 1.0 30.0) max-ticks 20000 input-fn (constantly {})}}]
  (loop [w world, t 0, evs []]
    (if (or (>= t max-ticks) (match/ended? w))
      {:world w :events evs :ticks t :ended? (match/ended? w)
       :standings (match/standings w)}
      (let [[w' es] (step w dt (input-fn w t))]
        (recur w' (inc t) (into evs es))))))
