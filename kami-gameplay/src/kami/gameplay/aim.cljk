(ns kami.gameplay.aim
  "Camera rig and aim direction — the third-person half of a third-person game.

  The shipped royale scene authors a camera at `{:distance 27 :height 15}` and
  the shipped ECS returns a constant for every rotation read (`get-rx`..`get-rw`
  are `0 0 0 1`, `set-rotation` is a no-op). Between those two facts the game
  cannot be anything but an overhead diorama: the camera is far enough that a
  1.9-unit actor is a few pixels tall, and nothing in the contract can express
  which way that actor is looking anyway.

  This namespace supplies the missing half:

  * a **yaw/pitch controller pose** — where the player is looking, as state the
    engine owns rather than a value the renderer invents;
  * an **over-the-shoulder boom** with a shoulder offset and a collision-aware
    pullback, i.e. an actual TPS camera rather than a fixed aerial rig;
  * the **camera-relative movement basis**, so `W` means \"away from the
    camera\" instead of \"toward +X\" — the single change that most separates a
    3D character controller from a top-down twin-stick;
  * **aim-down-sight blending** between two named rigs, which is what makes a
    weapon feel like it has a sight rather than a fire rate.

  Everything is a pure function of a pose and a scene profile. The renderer
  consumes `eye`/`target`/`fov`; it decides nothing."
  (:require [kami.gameplay.vec3 :as v]
            [kami.gameplay.attributes :as attr]))

(def default-rig
  "The hip-fire third-person rig. `:distance` and `:height` are metres behind
  and above the pawn's shoulder, not the map-scale numbers an aerial rig needs."
  {:distance 4.2
   :height 1.65
   :shoulder 0.75
   :fov 62.0
   :look-height 1.45
   :min-distance 0.6
   :pitch-min -1.15
   :pitch-max 1.05})

(def default-ads-rig
  "Sighted rig: the boom shortens, the shoulder offset almost vanishes and the
  field of view narrows. Narrowing FOV is what reads as magnification; keeping
  a little shoulder offset is what keeps it third-person."
  {:distance 1.5
   :height 1.55
   :shoulder 0.32
   :fov 42.0
   :look-height 1.5})

(defn rig
  "Resolve the rig pair from a scene's `:camera/rig` map, filling defaults."
  [scene]
  (let [c (:camera/rig scene)]
    {:hip (merge default-rig (:hip c))
     :ads (merge default-rig default-ads-rig (:ads c))}))

(defn blend-rig
  "Interpolate hip → ads by `t` in [0,1]. Blending the rig rather than snapping
  between two cameras is what removes the teleport on right-click."
  [{:keys [hip ads]} t]
  (let [t (double (clojure.core/max 0.0 (clojure.core/min 1.0 t)))
        f (fn [k] (+ (* (- 1.0 t) (double (get hip k))) (* t (double (get ads k)))))]
    (-> hip
        (assoc :distance (f :distance)
               :height (f :height)
               :shoulder (f :shoulder)
               :fov (f :fov)
               :look-height (f :look-height)))))

(defn clamp-pitch
  "Pitch is clamped, never wrapped: a camera that rolls over the top is a bug
  in every third-person game ever shipped."
  [r p]
  (let [lo (double (:pitch-min r -1.15))
        hi (double (:pitch-max r 1.05))]
    (clojure.core/max lo (clojure.core/min hi (double p)))))

(defn look-direction
  "Unit forward vector for a yaw/pitch pose.

  Yaw 0 looks down -Z and increases toward +X, matching the right-handed
  Y-up convention the render-IR already uses. Positive pitch looks up."
  [yaw pitch]
  (let [yaw (double yaw) pitch (double pitch)
        cp (Math/cos pitch)]
    (v/normalize [(* cp (Math/sin yaw))
                  (Math/sin pitch)
                  (* cp (- (Math/cos yaw)))])))

(defn move-basis
  "Camera-relative ground basis for a yaw: `{:forward v :right v}`, both flat
  on the XZ plane.

  This is the function that stops the game being top-down. With it, the analog
  stick means \"go where the camera is pointing\"; without it, the stick means
  \"go toward world +X\" and the player is steering a map cursor."
  [yaw]
  (let [f (v/normalize (v/with-y (look-direction yaw 0.0) 0.0))
        r (v/normalize (v/cross f v/up))]
    {:forward f :right r}))

(defn move-vector
  "Ground velocity direction for stick input `[ax ay]` in camera space.

  `ay` is forward/back, `ax` is strafe. The result is clamped to unit length so
  a diagonal is not faster than a cardinal — the classic bug that makes players
  run everywhere at 45 degrees."
  [yaw ax ay]
  (let [{:keys [forward right]} (move-basis yaw)
        raw (v/add (v/scale forward (double ay)) (v/scale right (double ax)))
        l (v/length raw)]
    (if (> l 1.0) (v/normalize raw) raw)))

(defn eye-position
  "Where the player's eyes are — the origin every shot is traced from.

  Shots start at the eye, not at the camera and not at the pawn's feet. Tracing
  from the camera lets a player shoot around a corner they cannot see past;
  tracing from the feet buries every shot in the ground."
  [pawn-pos eye-height]
  (v/with-y pawn-pos (+ (v/y pawn-pos) (double eye-height))))

(defn camera-pose
  "Full camera pose for a pawn: `{:eye :target :fov :forward :distance}`.

  `blocked-distance-fn`, when supplied, is called with the desired boom origin
  and direction and returns how far the boom may extend before it enters
  geometry. Passing it is what stops the camera clipping through a wall and
  showing the inside of the level; omitting it gives the unobstructed rig."
  ([pawn-pos yaw pitch r] (camera-pose pawn-pos yaw pitch r nil))
  ([pawn-pos yaw pitch r blocked-distance-fn]
   (let [pitch (clamp-pitch r pitch)
         fwd (look-direction yaw pitch)
         {:keys [right]} (move-basis yaw)
         focus (-> pawn-pos
                   (v/with-y (+ (v/y pawn-pos) (double (:look-height r))))
                   (v/add (v/scale right (double (:shoulder r)))))
         boom-dir (v/normalize (v/scale fwd -1.0))
         want (double (:distance r))
         allowed (if blocked-distance-fn
                   (clojure.core/min want (double (blocked-distance-fn focus boom-dir want)))
                   want)
         dist (clojure.core/max (double (:min-distance r 0.6)) allowed)
         eye (-> focus
                 (v/add (v/scale boom-dir dist))
                 (v/add (v/scale v/up (* 0.35 (double (:height r))))))]
     {:eye eye
      :target (v/add focus (v/scale fwd 12.0))
      :focus focus
      :forward fwd
      :fov (double (:fov r))
      :distance dist
      :pitch pitch
      :yaw yaw})))

(defn pawn-pose
  "Read the controller pose off an entity's attributes."
  [e]
  {:yaw (attr/get e :yaw)
   :pitch (attr/get e :pitch)
   :ads (attr/get e :ads)
   :eye-height (attr/get e :eye-height)})

(defn apply-look
  "Fold a mouse/stick look delta into an entity's yaw and pitch.

  `sensitivity` is radians per unit of input. Yaw wraps (via the attribute
  clamp), pitch is clamped by the rig — both handled by the writers rather than
  by every caller."
  [e rigs dyaw dpitch sensitivity]
  (let [s (double sensitivity)
        y (+ (attr/get e :yaw) (* s (double dyaw)))
        p (clamp-pitch (:hip rigs) (+ (attr/get e :pitch) (* s (double dpitch))))]
    (-> e (attr/set :yaw y) (attr/set :pitch p))))

(defn step-ads
  "Advance the aim-down-sight blend toward `held?` at `rate` per second.

  A blend rather than a boolean: the rig interpolation above is only smooth if
  the number driving it is."
  [e held? dt rate]
  (let [cur (attr/get e :ads)
        goal (if held? 1.0 0.0)
        step (* (double rate) (double dt))
        nxt (if (> goal cur)
              (clojure.core/min goal (+ cur step))
              (clojure.core/max goal (- cur step)))]
    (attr/set e :ads nxt)))
