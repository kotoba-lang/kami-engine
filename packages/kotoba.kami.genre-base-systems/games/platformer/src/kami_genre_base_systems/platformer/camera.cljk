(ns kami-genre-base-systems.platformer.camera
  "Room-lock camera for a Castlevania-style 2D side-scroller: the viewport
   never shows outside the current room's bounds (no free-scroll), and a
   room-to-room door transition is modeled as a short lerp toward the new
   room's clamped position rather than an instant cut. Pure data in,
   data out -- no rendering, no timers beyond the dt passed in each tick.")

(def default-scroll-speed
  "World units/second the camera scrolls during a room transition."
  480.0)

(def scroll-arrival-epsilon
  "Distance below which a transition is considered arrived, to avoid an
   asymptotic crawl toward the target on the final fraction of a unit."
  0.5)

(defn clamp-camera
  "Clamps a `viewport` ({:width :height}) so it never shows outside `room`
   ({:x :y :width :height}, top-left origin), biased toward centering
   `player-pos` ({:x :y}). When an axis of the room is smaller than the
   viewport, that axis is centered on the room instead of clamped (there
   is nothing to clamp against -- the whole room is visible either way).
   Returns {:x :y}, the viewport's world-space top-left."
  [room player-pos viewport]
  (let [{:keys [x y width height]} room
        {vw :width vh :height} viewport
        clamp-axis (fn [desired room-min room-size view-size]
                     (if (<= room-size view-size)
                       (+ room-min (/ (- room-size view-size) 2))
                       (-> desired (max room-min) (min (- (+ room-min room-size) view-size)))))]
    {:x (clamp-axis (- (:x player-pos) (/ vw 2)) x width vw)
     :y (clamp-axis (- (:y player-pos) (/ vh 2)) y height vh)}))

(defn camera-idle
  "A fresh, non-scrolling camera-state already locked to `room`."
  [room player-pos viewport]
  (let [{:keys [x y]} (clamp-camera room player-pos viewport)]
    {:camera-x x :camera-y y :scrolling? false :scroll-target nil :room room}))

(defn start-room-transition
  "Begins a room transition into `target-room`: the transition's target is
   `target-room`'s own clamp-camera result (i.e. the camera scrolls to
   wherever it would have hard-locked to had the player already been in
   the new room). Marks :scrolling? true; :room becomes target-room
   immediately (the game's own entity/collision logic switches rooms on
   its own schedule -- this only owns the camera's view of it)."
  [camera-state target-room player-pos viewport]
  (assoc camera-state
         :scrolling? true
         :scroll-target (clamp-camera target-room player-pos viewport)
         :room target-room))

(defn- distance [x1 y1 x2 y2]
  (let [dx (- x2 x1) dy (- y2 y1)]
    #?(:clj (Math/sqrt (+ (* dx dx) (* dy dy)))
       :cljs (js/Math.sqrt (+ (* dx dx) (* dy dy))))))

(defn step-camera
  "Advances `camera-state` by `dt` seconds.
   - Not scrolling: hard-locks to clamp-camera every tick against the
     current room (a true room-lock camera has no free-scroll drift even
     while idle -- it simply always reports where it should be).
   - Scrolling (mid room-transition): lerps camera-x/camera-y toward
     :scroll-target at default-scroll-speed world units/sec, clearing
     :scrolling? once within scroll-arrival-epsilon.
   Returns the next camera-state ({:camera-x :camera-y :scrolling?
   :scroll-target}, plus :room passed through)."
  ([camera-state player-pos viewport dt]
   (step-camera camera-state player-pos viewport dt default-scroll-speed))
  ([{:keys [camera-x camera-y scrolling? scroll-target room] :as camera-state}
    player-pos viewport dt scroll-speed]
   (if scrolling?
     (let [{tx :x ty :y} scroll-target
           dist (distance camera-x camera-y tx ty)
           step (* scroll-speed dt)]
       (if (<= dist (max step scroll-arrival-epsilon))
         (assoc camera-state :camera-x tx :camera-y ty :scrolling? false :scroll-target nil)
         (let [ratio (/ step dist)]
           (assoc camera-state
                  :camera-x (+ camera-x (* (- tx camera-x) ratio))
                  :camera-y (+ camera-y (* (- ty camera-y) ratio))))))
     (let [{:keys [x y]} (clamp-camera room player-pos viewport)]
       (assoc camera-state :camera-x x :camera-y y :scrolling? false :scroll-target nil)))))
