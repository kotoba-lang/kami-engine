(ns kami.gameplay.perception
  "What an AI is allowed to know — the engine's AI Perception component.

  The shipped bot logic is `nearest-tagged \"player\" ... hunt-range` followed
  by `move-toward!`: every bot on the map knows where the player is, through
  walls, at 3000 units, forever. That is the other half of why the game reads
  as a 2D toy — the opposition has no state, no senses and no reason to be
  anywhere in particular.

  Perception here is a *filter*, not a behaviour. It answers \"can this observer
  see that target right now\" from range, field of view and an occlusion
  predicate the caller supplies, and it remembers a target for a short while
  after it goes out of sight so a bot does not forget a player who stepped
  behind a crate. Deciding what to do about it is `kami.gameplay.ai`."
  (:require [kami.gameplay.vec3 :as v]
            [kami.gameplay.attributes :as attr]))

(def default-senses
  "Sight is a cone; hearing is a sphere. Splitting them is what lets a player
  sneak up behind a bot and also lets a firefight pull bots in from off-screen."
  {:sight-range 220.0
   :fov-degrees 110.0
   :hearing-range 45.0
   :gunshot-hearing-range 260.0
   :memory-seconds 4.0})

(defn senses [scene] (merge default-senses (:ai/senses scene)))

(defn in-cone?
  "Whether `target-pos` lies inside `observer`'s view cone.

  Uses the observer's yaw only. Pitch is deliberately ignored: a bot that loses
  track of a player standing on a ramp because its head was level is a worse
  bug than a bot that notices one slightly above it."
  [observer target-pos fov-degrees]
  (let [yaw (attr/get observer :yaw)
        fwd (v/normalize (v/with-y [(Math/sin yaw) 0.0 (- (Math/cos yaw))] 0.0))
        to (v/normalize (v/with-y (v/sub target-pos (:pos observer)) 0.0))]
    (if (< (v/length to) 1e-9)
      true
      (>= (v/dot fwd to) (Math/cos (* 0.5 (* (double fov-degrees) (/ Math/PI 180.0))))))))

(defn can-see?
  "Range + cone + occlusion. `occluded?` is a caller-supplied predicate of two
  world positions; when omitted, line of sight is unobstructed.

  Keeping occlusion injected means this namespace has no opinion about how the
  level is represented — a host with a real collision world and a test with
  three boxes both use the same code path."
  ([observer target s] (can-see? observer target s nil))
  ([observer target s occluded?]
   (let [eye (v/with-y (:pos observer) (+ (v/y (:pos observer)) (attr/get observer :eye-height)))
         tgt (v/with-y (:pos target) (+ (v/y (:pos target)) (* 0.7 (attr/get target :height))))]
     (boolean
       (and (attr/alive? target)
            (<= (v/dist eye tgt) (double (:sight-range s)))
            (in-cone? observer (:pos target) (:fov-degrees s))
            (not (and occluded? (occluded? eye tgt))))))))

(defn can-hear?
  "Proximity hearing, ignoring facing and walls. `loud?` widens the radius to
  the gunshot range — which is how a firefight recruits nearby bots."
  [observer target s loud?]
  (let [r (if loud? (:gunshot-hearing-range s) (:hearing-range s))]
    (and (attr/alive? target)
         (<= (v/dist-xz (:pos observer) (:pos target)) (double r)))))

(defn sense-targets
  "Every candidate this observer currently perceives, nearest first.

  Each entry carries `:how` (`:sight` or `:hearing`) so the behaviour layer can
  treat a heard contact differently from a seen one — investigate versus engage.
  Seen contacts sort ahead of heard ones; within each group, nearest first."
  [world observer-id s {:keys [occluded? loud-ids tag]
                        :or {loud-ids #{}}}]
  (let [observer (get-in world [:entities observer-id])
        loud-ids (set loud-ids)]
    (when (attr/alive? observer)
      (->> (:entities world)
           (keep (fn [[id e]]
                   (when (and (not= id observer-id)
                              (attr/alive? e)
                              (or (nil? tag) (= (:tag e) tag))
                              (not (attr/same-team? observer e)))
                     (cond
                       (can-see? observer e s occluded?)
                       {:id id :how :sight :distance (v/dist-xz (:pos observer) (:pos e))}
                       (can-hear? observer e s (contains? loud-ids id))
                       {:id id :how :hearing :distance (v/dist-xz (:pos observer) (:pos e))}
                       :else nil))))
           ;; sight first, then nearest. A bot that walks past the enemy it can
           ;; see to investigate a noise further away looks broken, and sorting
           ;; on distance alone does exactly that.
           (sort-by (juxt #(if (= :sight (:how %)) 0 1) :distance))
           vec))))

(defn remember
  "Record a contact on the observer, returning the updated entity.

  Memory is stored as an attribute deadline rather than a timer object so it
  survives the same serialisation path as everything else in the world.

  Falls back to the declared `default-senses` duration when a caller passes a
  senses map without one. A declared default is a value; the alternative on
  ClojureScript was a NaN deadline that never expires, which is a bot that
  tracks a target forever and no error anywhere."
  [observer target-id now-ms s]
  (let [secs (double (or (:memory-seconds s) (:memory-seconds default-senses)))]
    (-> observer
        (attr/set :ai-target (double target-id))
        (attr/set :ai-until (+ (double (or now-ms 0)) (* 1000.0 secs))))))

(defn remembered
  "The target the observer is still tracking, or nil once memory has lapsed."
  [observer now-ms]
  (let [t (long (attr/get observer :ai-target))]
    (when (and (>= t 0) (> (attr/get observer :ai-until) (double now-ms)))
      t)))
