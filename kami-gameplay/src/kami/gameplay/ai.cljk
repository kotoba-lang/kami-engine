(ns kami.gameplay.ai
  "Bot behaviour: a small explicit state machine, not a beeline.

  The shipped bot is one line — `move-toward!` the player, from anywhere on the
  map, forever. It produces the defining texture of the current build: a clump
  of red boxes converging on you in a straight line at constant speed, which is
  what an overhead 2D game looks like even when it is rendered in 3D.

  Five states, each with a reason to exist:

  * `:idle`      — nothing perceived; hold position and sweep the view cone.
                   The sweep is not decoration. Sight is a cone, so a bot that
                   holds a fixed facing can only ever notice what walks into
                   it; measured over a 24-entrant match, bots spawned on a ring
                   all facing the same way never acquired the player at all and
                   stood still for the entire match while he shot at them from
                   210 units away. Bots that are not hunting you are also what
                   makes a map feel populated instead of aimed at you.
  * `:investigate` — heard but not seen; move to the last known position. This
                   is the state that makes sound matter.
  * `:engage`    — seen and in weapon range; face the target, keep preferred
                   spacing, fire through the real weapon state machine (so bots
                   reload, run dry and miss like players do).
  * `:reposition` — seen but badly placed (too close for a sniper, too far for a
                   shotgun); strafe rather than walk into contact.
  * `:rotate`    — outside the storm; nothing else matters until that is fixed.

  Priority is fixed and storm-first for a reason: a bot that fights while dying
  to the storm reads as broken, and the storm is the only thing that guarantees
  the match ends.

  Every function is pure and takes the RNG state explicitly, so a bot's
  decisions replay exactly."
  (:require [kami.gameplay.vec3 :as v]
            [kami.gameplay.rng :as rng]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.perception :as perc]
            [kami.gameplay.weapon :as weapon]))

(def states [:idle :investigate :engage :reposition :rotate])
(def state->code (zipmap states (map double (range))))
(def code->state (zipmap (map double (range)) states))

(defn state-of [e] (get code->state (attr/get e :ai-state) :idle))
(defn set-state [e s] (attr/set e :ai-state (get state->code s 0.0)))

(def default-profile
  "Tunables a scene may override per bot archetype.

  `:preferred-range-frac` is a fraction of the equipped weapon's **full-damage
  envelope** — its `:damage-falloff` distance — not of its maximum range. That
  distinction is the difference between an opponent and a nuisance. Measured
  over a 24-entrant headless match, engaging at a fraction of `:range` put
  assault-rifle bots at 110 units, where their own dispersion is wider than a
  human-sized target and 4% of shots connected; engaging just inside
  `:damage-falloff` puts them at 45 units, which is a fight.

  It also gives each archetype a distinct distance out of data that already
  exists: shotgun 9, SMG 22, pistol 27, assault rifle 45, sniper 180."
  {:preferred-range-frac 0.9
   :range-tolerance 0.22
   :aim-error-degrees 1.2
   :reaction-ms 220
   :strafe-speed-frac 0.7
   :storm-margin 12.0
   :scan-rate-rad-s 0.6})

(defn profile [scene] (merge default-profile (:ai/profile scene)))

(defn preferred-range
  "How far this bot wants to be from its target, given what it is holding.

  Derived from the weapon's `:damage-falloff` — the distance out to which it
  still does its full stated damage — so a bot fights where its weapon is
  actually good. Unarmed bots want to be close: they are going to have to melee
  or loot."
  [table e prof]
  (if-let [w (weapon/equipped table e)]
    (max 4.0 (* (double (:damage-falloff w)) (double (:preferred-range-frac prof))))
    6.0))

(defn aim-at
  "Point `e` at `target-pos`, with a bounded random error. Returns `[e' rng']`.

  The error is what makes bots beatable and, more importantly, what makes them
  *feel* like opponents rather than turrets. It is applied to the stored yaw and
  pitch — the same attributes a human player writes — so the bot shoots through
  exactly the same ballistics path a player does."
  [rs e target-pos prof]
  (let [from (v/with-y (:pos e) (+ (v/y (:pos e)) (attr/get e :eye-height)))
        to (v/with-y target-pos (+ (v/y target-pos) 1.2))
        d (v/sub to from)
        want-yaw (Math/atan2 (v/x d) (- (v/z d)))
        flat (Math/sqrt (+ (* (v/x d) (v/x d)) (* (v/z d) (v/z d))))
        want-pitch (Math/atan2 (v/y d) (max 1e-6 flat))
        err (* (double (:aim-error-degrees prof)) (/ Math/PI 180.0))
        [ey rs1] (rng/range-f rs (- err) err)
        [ep rs2] (rng/range-f rs1 (- err) err)]
    [(-> e (attr/set :yaw (+ want-yaw ey)) (attr/set :pitch (+ want-pitch ep))) rs2]))

(defn decide
  "Choose this bot's state and movement goal. Returns `{:state :goal :target}`.

  Pure decision — it writes nothing. The caller applies the result, which keeps
  the policy readable and lets a test assert on the decision without stepping a
  world."
  [world table bot-id {:keys [zone senses prof now-ms occluded? loud-ids]}]
  (let [e (get-in world [:entities bot-id])
        prof (or prof default-profile)
        outside (when zone
                  (let [d (- (v/dist-xz (:pos e) (:center zone)) (:radius zone))]
                    (when (> d (- (double (:storm-margin prof)))) d)))
        seen (perc/sense-targets world bot-id senses
                                 {:occluded? occluded? :loud-ids loud-ids})
        best (first seen)
        memory (perc/remembered e now-ms)]
    (cond
      ;; storm first: nothing else matters while you are dying to it
      (and zone (some? outside) (pos? outside))
      {:state :rotate
       :goal (let [c (:center zone)
                   dir (v/normalize (v/with-y (v/sub (:pos e) c) 0.0))]
               (v/add c (v/scale dir (max 0.0 (- (:radius zone) (double (:storm-margin prof)))))))
       :target nil}

      (and best (= (:how best) :sight))
      (let [tgt (get-in world [:entities (:id best)])
            want (preferred-range table e prof)
            tol (* want (double (:range-tolerance prof)))
            d (v/dist-xz (:pos e) (:pos tgt))]
        (if (<= (Math/abs (- d want)) tol)
          {:state :engage :goal nil :target (:id best)}
          {:state :reposition
           :goal (let [dir (v/normalize (v/with-y (v/sub (:pos tgt) (:pos e)) 0.0))]
                   (if (< d want)
                     (v/add (:pos e) (v/scale dir (- (- want d))))
                     (v/add (:pos e) (v/scale dir (- d want)))))
           :target (:id best)}))

      (and best (= (:how best) :hearing))
      {:state :investigate :goal (:pos (get-in world [:entities (:id best)])) :target (:id best)}

      memory
      {:state :investigate :goal (:pos (get-in world [:entities memory])) :target memory}

      :else
      {:state :idle :goal nil :target nil})))

(defn scan
  "Sweep an idle bot's facing. Returns `[entity' rng']`.

  The direction is drawn once and stored on the entity rather than redrawn each
  tick: a fresh draw per tick is a random walk that goes nowhere, and it would
  also make every bot's sweep consume the shared RNG stream every frame."
  [rs e prof dt]
  (let [[dir rs']
        (if (zero? (attr/get e :ai-scan))
          (let [[b r2] (rng/int rs 2)] [(if (zero? b) -1.0 1.0) r2])
          [(attr/get e :ai-scan) rs])]
    [(-> e
         (attr/set :ai-scan dir)
         (attr/set :yaw (+ (attr/get e :yaw)
                           (* dir (double (:scan-rate-rad-s prof 0.6)) (double dt)))))
     rs']))

(defn apply-decision
  "Write a decision onto the bot: state, remembered target, facing and velocity.
  Returns `[entity' rng']`."
  [rs world e {:keys [state goal target]} {:keys [senses prof now-ms dt]
                                           :or {dt 0.0 now-ms 0}}]
  (let [prof (or prof default-profile)
        e (set-state e state)
        ;; Bots sight their weapon while engaging and lower it otherwise, for
        ;; the same reason a player does: the aim-down-sight blend is what
        ;; tightens the cone. A bot that never used the mechanic would be
        ;; permanently worse than the weapon it holds.
        e (attr/set e :ads (if (= state :engage) 1.0 0.0))
        e (if target (perc/remember e target now-ms senses) e)
        speed (attr/get e :move-speed)
        speed (if (= state :reposition) (* speed (double (:strafe-speed-frac prof))) speed)
        [e rs] (cond
                 (and target (get-in world [:entities target]))
                 (aim-at rs e (:pos (get-in world [:entities target])) prof)

                 (= state :idle) (scan rs e prof dt)

                 :else [e rs])
        vel (if goal
              (let [d (v/with-y (v/sub goal (:pos e)) 0.0)]
                (if (< (v/length d) 0.25) v/zero (v/scale (v/normalize d) speed)))
              v/zero)]
    [(assoc e :vel vel) rs]))
