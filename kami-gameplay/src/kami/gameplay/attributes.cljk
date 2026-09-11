(ns kami.gameplay.attributes
  "The attribute plane — the thing the shipped ECS does not have.

  `kami.host`'s entity is `{:tag :x :y :z :vx :vy :vz}`: seven numbers and a
  string. A guest can move a point mass and nothing else. No health, no team,
  no facing, no ammo — which is why a game written against that contract reads
  as an overhead 2D toy no matter how the renderer is lit.

  Unreal solves this with an attribute set the Ability System reads and writes
  through a single typed channel. This is the same idea at this engine's scale:
  entities carry named f32 attributes, every write goes through `set-attr`, and
  every attribute declares its own clamp and default in one table. Adding a
  mechanic then costs one row of data instead of one host import.

  Two invariants the rest of the framework leans on:

  1. Reading an attribute that was never set returns its *declared* default,
     not nil and not 0. `(health e)` on a fresh entity is full health, so a
     spawn that forgot to initialise health cannot be silently born dead.
  2. Every write is clamped to the declared range. Nothing downstream needs to
     re-check that health stayed in [0, max] — including code that subtracts a
     damage number it did not compute."
  (:refer-clojure :exclude [get set]))

(def registry
  "Declared attributes: default, clamp range, and what the number means.

  `:max` may be a keyword naming another attribute, which makes the ceiling
  per-entity (a player with 100 health and a boss with 2000 share this row).
  `:reset-on-respawn?` marks the attributes a respawn restores; the rest — kills,
  placement — survive a life and belong to the match record."
  {:health        {:default 100.0 :min 0.0 :max :health-max :reset-on-respawn? true}
   :health-max    {:default 100.0 :min 1.0 :max 10000.0}
   :shield        {:default 0.0   :min 0.0 :max :shield-max  :reset-on-respawn? true}
   :shield-max    {:default 100.0 :min 0.0 :max 10000.0}
   :armor         {:default 0.0   :min 0.0 :max 0.95
                   :doc "fraction of post-shield damage absorbed"}
   :team          {:default 0.0   :min 0.0 :max 64.0}
   :yaw           {:default 0.0   :min -3.14159265 :max 3.14159265
                   :doc "facing around +Y, radians, wrapped not clamped"}
   :pitch         {:default 0.0   :min -1.4 :max 1.4
                   :doc "look elevation, radians; clamped short of straight up/down"}
   :eye-height    {:default 1.6   :min 0.0 :max 4.0}
   :weapon        {:default -1.0  :min -1.0 :max 4096.0
                   :doc "index into the weapon table, -1 = unarmed"}
   :ammo          {:default 0.0   :min 0.0 :max 4096.0}
   :reserve-ammo  {:default 0.0   :min 0.0 :max 65535.0}
   :reload-until  {:default 0.0   :min 0.0 :max 1e12 :reset-on-respawn? true}
   :next-fire-at  {:default 0.0   :min 0.0 :max 1e12 :reset-on-respawn? true}
   :ads           {:default 0.0   :min 0.0 :max 1.0
                   :doc "aim-down-sight blend, 0 hip / 1 sighted"}
   :stance        {:default 0.0   :min 0.0 :max 2.0
                   :doc "0 stand, 1 crouch, 2 prone"}
   :alive         {:default 1.0   :min 0.0 :max 1.0 :reset-on-respawn? true}
   :downed        {:default 0.0   :min 0.0 :max 1.0 :reset-on-respawn? true}
   :kills         {:default 0.0   :min 0.0 :max 4096.0}
   :damage-dealt  {:default 0.0   :min 0.0 :max 1e9}
   :place         {:default 0.0   :min 0.0 :max 4096.0}
   :height        {:default 1.9   :min 0.1 :max 20.0}
   :radius        {:default 0.55  :min 0.05 :max 20.0}
   :move-speed    {:default 6.0   :min 0.0 :max 200.0}
   :ai-state      {:default 0.0   :min 0.0 :max 8.0}
   :ai-target     {:default -1.0  :min -1.0 :max 1e9}
   :ai-until      {:default 0.0   :min 0.0 :max 1e12}
   :ai-scan       {:default 0.0   :min -1.0 :max 1.0
                   :doc "idle sweep direction; 0 means not yet chosen"}})

(def ^:private wrap-two-pi (* 2.0 Math/PI))

(defn wrap-angle
  "Fold an angle into (-pi, pi] — the same half-open interval `atan2` returns,
  so a wrapped yaw and a freshly computed one are directly comparable.

  Yaw is periodic; clamping it would make a bot that turns past pi stick facing
  sideways forever. The `<= 0.0` branch is what puts the boundary at +pi rather
  than -pi: without it, a yaw of exactly pi lands on -pi and a turn that should
  have been a no-op reads as a 360-degree spin."
  [a]
  (let [a (double a)
        m (mod (+ a Math/PI) wrap-two-pi)
        m (if (<= m 0.0) (+ m wrap-two-pi) m)]
    (- m Math/PI)))

(defn spec
  "The declared row for `k`, or nil when the attribute is not registered."
  [k]
  (clojure.core/get registry k))

(defn default
  "Declared default for `k`. Unregistered attributes default to 0.0 — they are
  free-form game scratch space, not part of the engine contract."
  [k]
  (or (:default (spec k)) 0.0))

(defn get
  "Read attribute `k` off entity map `e`, falling back to the declared default."
  [e k]
  (double (or (get-in e [:attrs k]) (default k))))

(defn- resolve-max [e k]
  (let [m (:max (spec k))]
    (cond
      (keyword? m) (get e m)
      (number? m) (double m)
      :else 1e12)))

(defn- finite
  "Coerce to a double, refusing values that are not numbers.

  This exists because the two platforms disagree about what a bad write does,
  and the disagreement runs the wrong way. On the JVM `(double nil)` throws
  immediately; in ClojureScript it yields NaN, `(+ NaN x)` yields NaN, and every
  comparison against NaN is false — so `clamp`'s `cond` falls through to
  `:else` and stores it. From there a NaN deadline never expires, a NaN
  position never collides and a NaN health is neither alive nor dead, silently
  and forever.

  Found by running the suite on both platforms: a missing `:now-ms` threw on the
  JVM and passed green under Node, which is the failure this repository keeps
  finding in its own gates — the check that could not be made returning the same
  value as the check that found nothing wrong.

  A non-finite attribute write is always a caller bug, so it fails loudly on
  both platforms instead."
  [k v]
  (when (nil? v)
    (throw (ex-info "attribute write of nil" {:attribute k})))
  (let [d (double v)]
    (when-not #?(:clj (Double/isFinite d) :cljs (js/isFinite d))
      (throw (ex-info "attribute write of a non-finite value"
                      {:attribute k :value v})))
    d))

(defn clamp
  "Clamp `v` into `k`'s declared range for this entity. Yaw wraps instead.

  Refuses nil and non-finite writes on both platforms — see `finite`."
  [e k v]
  (let [v (finite k v)]
    (if (= k :yaw)
      (wrap-angle v)
      (let [lo (double (or (:min (spec k)) -1e12))
            hi (resolve-max e k)]
        (cond (< v lo) lo (> v hi) hi :else v)))))

(defn set
  "Write attribute `k`, clamped. Returns the updated entity."
  [e k v]
  (assoc-in e [:attrs k] (clamp e k v)))

(defn update-attr
  "Read-modify-write `k` through the clamp. `(update-attr e :health - 30)`."
  [e k f & args]
  (set e k (apply f (get e k) args)))

(defn set-many
  "Write a map of attributes in one call, each clamped in turn."
  [e m]
  (reduce-kv set e m))

(defn alive?
  "An entity is alive when it is registered, `:alive` is set and health is
  above zero. All three are checked because a caller that zeroed health without
  clearing `:alive`, or vice versa, is a bug we would rather not let shoot."
  [e]
  (boolean (and e (pos? (get e :alive)) (pos? (get e :health)))))

(defn downed? [e] (and (some? e) (pos? (get e :downed))))

(defn effective-health
  "Shield plus health — the pool a damage number has to chew through."
  [e]
  (+ (get e :shield) (get e :health)))

(defn respawn
  "Restore the attributes declared `:reset-on-respawn?`, leaving the match
  record (kills, damage dealt, placement) intact."
  [e]
  (reduce (fn [acc [k s]]
            (if (:reset-on-respawn? s) (set acc k (default k)) acc))
          e registry))

(def ^:const free-for-all-team
  "Team 0 is the free-for-all team: its members damage each other. Any other
  team number is a real squad and is protected from its own members. Solo
  battle royale therefore puts every player on team 0 and needs no special
  case anywhere else."
  0.0)

(defn same-team?
  "Whether two entities are protected from each other. Lives here rather than
  in the damage pipeline because perception and AI targeting need the same
  answer, and two definitions of `friendly` is how friendly fire gets shipped."
  [a b]
  (let [ta (get a :team) tb (get b :team)]
    (and (= ta tb) (not= ta free-for-all-team))))
