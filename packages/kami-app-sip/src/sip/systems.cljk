(ns sip.systems
  "Runtime systems — the *living world* that breathes whether or not the player
  acts (design doc §7: the world keeps its rhythm; never a still, dead screen).

  These run each fixed step in the browser over the loaded ECS snapshot, via the
  engine's `kami.sim/defsystem` (pure over the ECS `world`, no GPU). They are
  intentionally cosmetic & gentle — sakura sway, canal shimmer, a slow day-drift
  of the light. The game's *meaning* (sessions, kokoro, awakening) is pure data
  in `sip.session` and persisted through `sip.store`; nothing here can ever harm
  a save or fail the player."
  (:require [kami.sim :as sim]
            [kami.ecs :as ecs])
  #?(:cljs (:require-macros [kami.sim :as sim])))

;; A host-side phase accumulator for the ambient animations. Cosmetic only —
;; the authoritative world lives in Datomic/Kotoba, not in this atom.
(defonce ^:private phase (atom 0.0))

(defn- yaw-quat
  "Quaternion [x y z w] for a rotation of `theta` radians about +Y."
  [theta]
  (let [h (* 0.5 theta)]
    [0.0 (Math/sin h) 0.0 (Math/cos h)]))

(sim/defsystem sakura-sway {:order 10} [world dt]
  "Drift every cherry tree a hair, so the canals are never still."
  (let [t (swap! phase + dt)
        q (yaw-quat (* 0.04 (Math/sin t)))]
    (reduce (fn [w e] (ecs/set-component w e :transform/rotation q))
            world
            (ecs/query world #{:transform/rotation :mesh/asset}))))

(sim/defsystem canal-shimmer {:order 20} [world dt]
  "A slow vertical breathing of water-surface entities (named \"canal\")."
  (let [t   @phase
        dy  (* 0.03 (Math/sin (* 0.7 t)))]
    (reduce (fn [w e]
              (let [{:keys [:transform/translation :kami/name]} (ecs/get-entity w e)]
                (if (and translation (= name "canal"))
                  (let [[x _ z] translation]
                    (ecs/set-component w e :transform/translation [x dy z]))
                  w)))
            world
            (ecs/query world #{:transform/translation}))))

(sim/defsystem day-drift {:order 30} [world dt]
  "Rotate the key light slowly across the sky — morning light is the mood of the
  water city. One full arc per long, unhurried cycle (no day/night fail state)."
  (let [t (+ @phase dt)
        q (yaw-quat (* 0.01 t))]
    (reduce (fn [w e] (ecs/set-component w e :transform/rotation q))
            world
            (ecs/query world #{:transform/rotation :light/kind}))))

(def all
  "System ids, in order, for `sim/run!`'s :systems list."
  [:sip.systems/sakura-sway :sip.systems/canal-shimmer :sip.systems/day-drift])
