(ns kami.gameplay.ballistics
  "From a trigger pull to a hit location: spread, travel, hitboxes, falloff.

  The shipped game's shooting is a host-side auto-fire that cannot miss for a
  reason the player can name. That is the single biggest reason it does not
  read as a shooter: with no spread, no falloff and no hitboxes, aiming is not
  a skill the game has.

  What this adds, all as pure functions of a deterministic RNG state:

  * **cone spread**, widened by movement and narrowed by aiming down sight, so
    the ADS blend in `aim` buys accuracy rather than just a zoom;
  * **capsule hitboxes** with a head segment, so `:headshot-mult` — a column
    that has sat unused in the weapon table since ADR-0046 — finally decides
    something;
  * **distance falloff** between the weapon's `:damage-falloff` and `:range`,
    so a shotgun is not a sniper;
  * **projectile travel** for slow weapons, so a rocket can be dodged.

  Nothing here mutates. `resolve-shot` returns a description of what happened
  and the caller decides what to do with it — which is what lets the same code
  run as the authority, as a client prediction, and as the replay oracle."
  (:require [kami.gameplay.vec3 :as v]
            [kami.gameplay.rng :as rng]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.weapon :as weapon]))

(def ^:const degrees->radians (/ Math/PI 180.0))

(defn spread-radians
  "Effective cone half-angle for a shot.

  Three multipliers, each earning its keep: aiming down sight tightens the cone
  (that is what ADS is *for*), moving loosens it (that is what makes stopping to
  shoot a decision), crouching tightens it a little more."
  [w {:keys [ads speed stance]
      :or {ads 0.0 speed 0.0 stance 0.0}}]
  (let [base (* degrees->radians (double (:spread w 0.0)))
        ads-f (- 1.0 (* 0.72 (double ads)))
        move-f (+ 1.0 (min 1.6 (* 0.16 (double speed))))
        stance-f (case (long stance) 1 0.82 2 0.68 1.0)]
    (max 0.0 (* base ads-f move-f stance-f))))

(defn scatter
  "Perturb `dir` inside a cone of `half-angle`. Returns `[dir' rng']`.

  Samples the *disc* (radius as sqrt of a uniform draw), not the angle
  directly. Sampling the angle uniformly clusters shots at the centre of the
  cone, which flatters the shooter and makes measured accuracy disagree with
  the number on the weapon card."
  [rs dir half-angle]
  (if (<= (double half-angle) 1e-9)
    [(v/normalize dir) rs]
    (let [[u1 rs1] (rng/unit rs)
          [u2 rs2] (rng/unit rs1)
          r (* (double half-angle) (Math/sqrt u1))
          theta (* 2.0 Math/PI u2)
          f (v/normalize dir)
          ref (if (> (Math/abs (v/y f)) 0.95) [1.0 0.0 0.0] v/up)
          right (v/normalize (v/cross f ref))
          upv (v/normalize (v/cross right f))
          off (v/add (v/scale right (* (Math/sin r) (Math/cos theta)))
                     (v/scale upv (* (Math/sin r) (Math/sin theta))))]
      [(v/normalize (v/add (v/scale f (Math/cos r)) off)) rs2])))

(defn falloff-multiplier
  "Damage multiplier at `distance`.

  Full damage out to `:damage-falloff`, a linear taper to `min-mult` at
  `:range`, and nothing beyond `:range`. Returning 0.0 past maximum range
  rather than a small number is deliberate: a weapon with a stated range should
  have one."
  ([w distance] (falloff-multiplier w distance 0.35))
  ([w distance min-mult]
   (let [d (double distance)
         start (double (:damage-falloff w 0.0))
         end (double (:range w 0.0))
         min-mult (double min-mult)]
     (cond
       (<= d start) 1.0
       (<= end start) 1.0
       (>= d end) 0.0
       :else (let [t (/ (- d start) (- end start))]
               (+ 1.0 (* t (- min-mult 1.0))))))))

(defn capsule-hit
  "Ray against an entity's capsule. Returns `{:t :zone :point}` or nil.

  `:zone` is `:head` or `:body`. The head is the top `head-frac` of the capsule
  and is tested with a tighter radius, which is what makes a headshot a thing
  you can *aim for* rather than a coin flip weighted by the multiplier."
  ([origin dir target-pos radius height] (capsule-hit origin dir target-pos radius height 0.16))
  ([origin dir target-pos radius height head-frac]
   (let [d (v/normalize dir)
         to (v/sub target-pos origin)
         axis-t (v/dot to d)]
     (when (>= axis-t 0.0)
       (let [closest (v/add origin (v/scale d axis-t))
             ;; horizontal miss distance against the capsule's vertical axis
             perp (v/dist-xz closest target-pos)]
         (when (<= perp (double radius))
           (let [y-hit (v/y closest)
                 base (v/y target-pos)
                 top (+ base (double height))]
             (when (and (>= y-hit (- base 0.05)) (<= y-hit top))
               (let [head-base (- top (* (double head-frac) (double height)))
                     head? (and (>= y-hit head-base) (<= perp (* 0.62 (double radius))))]
                 {:t axis-t
                  :zone (if head? :head :body)
                  :point closest})))))))))

(defn first-hit
  "Nearest capsule hit along a ray among `candidates`.

  `candidates` is a seq of `[id entity-map]`. Blocking geometry is handled by
  the caller passing `:max-distance` shortened to the wall hit — this function
  deliberately knows nothing about the level, so it stays testable without one."
  [origin dir candidates {:keys [max-distance exclude]
                          :or {max-distance 1e9}}]
  (->> candidates
       (keep (fn [[id e]]
               (when (and (not= id exclude) (attr/alive? e))
                 (when-let [h (capsule-hit origin dir (:pos e)
                                           (attr/get e :radius) (attr/get e :height))]
                   (when (<= (:t h) (double max-distance))
                     (assoc h :id id :entity e))))))
       (sort-by :t)
       first))

(defn resolve-shot
  "Resolve one trigger pull. Returns `[result rng']`.

  The result always describes what happened — `:kind` is one of `:hit`,
  `:miss` or `:projectile` — so a caller never has to infer a miss from a nil.
  That distinction is load-bearing for the HUD (a miss still plays a tracer and
  a wall impact) and for anti-cheat style reconciliation."
  [rs {:keys [weapon origin dir shooter candidates ads speed stance now-ms max-distance]
       :or {ads 0.0 speed 0.0 stance 0.0 now-ms 0}}]
  (let [half (spread-radians weapon {:ads ads :speed speed :stance stance})
        [sdir rs'] (scatter rs dir half)
        reach (double (or max-distance (:range weapon 1e9)))]
    (if-not (weapon/hitscan? weapon)
      [{:kind :projectile
        :origin origin
        :dir sdir
        :speed (double (:projectile-speed weapon))
        :weapon (:index weapon)
        :shooter shooter
        :spawned-at now-ms
        :max-distance reach}
       rs']
      (if-let [h (first-hit origin sdir candidates {:max-distance reach :exclude shooter})]
        (let [dist (:t h)
              mult (* (falloff-multiplier weapon dist)
                      (if (= (:zone h) :head) (double (:headshot-mult weapon 1.0)) 1.0))]
          [{:kind :hit
            :target (:id h)
            :zone (:zone h)
            :point (:point h)
            :distance dist
            :damage (* (double (:damage weapon)) mult)
            :weapon (:index weapon)
            :shooter shooter}
           rs'])
        [{:kind :miss
          :origin origin
          :dir sdir
          :distance reach
          :weapon (:index weapon)
          :shooter shooter}
         rs']))))

(defn step-projectile
  "Advance a projectile by `dt` seconds against `candidates`.

  Returns `{:projectile p'}` when it is still flying, `{:hit ...}` when it
  struck, or `{:expired ...}` when it ran out of range. Sweeping the segment
  travelled this tick, rather than testing the new point, is what stops a fast
  rocket tunnelling through a target at low frame rates."
  [p dt candidates]
  (let [step (* (:speed p) (double dt))
        from (:origin p)
        to (v/add from (v/scale (:dir p) step))
        travelled (+ (double (:travelled p 0.0)) step)]
    (if-let [h (first-hit from (:dir p) candidates
                          {:max-distance step :exclude (:shooter p)})]
      {:hit (assoc h :projectile p :travelled (+ (double (:travelled p 0.0)) (:t h)))}
      (if (>= travelled (:max-distance p))
        {:expired (assoc p :origin to :travelled travelled)}
        {:projectile (assoc p :origin to :travelled travelled)}))))
