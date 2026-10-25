(ns kami-genre-base-systems.platformer.weapon
  "Whip and sub-weapon attack state machines for a Castlevania-style
   stylish action game: :idle -> :windup -> :active -> :recovery -> :idle
   for the whip (only :active exposes a hitbox), a resource-gated
   (hearts) fire-and-cooldown machine for sub-weapons, and the classic
   'item crash' special reachable only while both are simultaneously
   :active. Pure data in, data out -- no I/O, no entity mutation.")

(def default-whip-durations
  "Seconds spent in each non-idle whip state before advancing."
  {:windup 0.12 :active 0.18 :recovery 0.22})

(def default-whip-reach
  "Whip hitbox shape, in world units, relative to the wielder's position."
  {:length 60.0 :height 16.0 :offset 8.0})

(defn whip-idle [] {:state :idle :timer 0.0})

(defn step-whip
  "Advances a whip attack FSM by dt. `attack-pressed?` only starts a new
   swing from :idle -- holding the button through an in-progress swing
   does not re-trigger or extend it (input buffering for a queued next
   swing is a host/input-layer concern, not modeled here)."
  ([whip-state attack-pressed? dt] (step-whip whip-state attack-pressed? dt default-whip-durations))
  ([{:keys [state timer]} attack-pressed? dt durations]
   (case state
     :idle (if attack-pressed?
             {:state :windup :timer 0.0}
             {:state :idle :timer 0.0})
     :windup (let [timer' (+ timer dt)]
               (if (>= timer' (:windup durations))
                 {:state :active :timer 0.0}
                 {:state :windup :timer timer'}))
     :active (let [timer' (+ timer dt)]
               (if (>= timer' (:active durations))
                 {:state :recovery :timer 0.0}
                 {:state :active :timer timer'}))
     :recovery (let [timer' (+ timer dt)]
                 (if (>= timer' (:recovery durations))
                   {:state :idle :timer 0.0}
                   {:state :recovery :timer timer'})))))

(defn whip-active? [whip-state] (= :active (:state whip-state)))

(defn whip-hitbox
  "The whip's world-space hitbox rect ({:x :y :width :height}) while
   :active, extended from `entity` ({:x :y :facing}, facing :left/:right)
   by the weapon's reach; nil in every other state (only :active exposes
   a hitbox, by design)."
  ([entity whip-state] (whip-hitbox entity whip-state default-whip-reach))
  ([{:keys [x y facing]} whip-state {:keys [length height offset]}]
   (when (whip-active? whip-state)
     {:x (if (= facing :left) (- x offset length) (+ x offset))
      :y (- y (/ height 2))
      :width length
      :height height})))

;; -- sub-weapon (dagger/axe/holy-water/etc.), hearts-gated --------------

(def default-sub-weapon-active-duration
  "Seconds a fired sub-weapon stays :active (its own brief cooldown/lockout
   before another can be thrown) before returning to :idle."
  0.5)

(defn sub-weapon-idle [] {:state :idle :timer 0.0 :kind nil})

(defn try-fire-sub-weapon
  "Attempts to fire a sub-weapon of `kind` costing `cost` hearts. Only
   succeeds from :idle with hearts >= cost -- never mutates in place, the
   caller threads the returned state/hearts forward. Returns
   {:fired? :hearts :sub-state}."
  [sub-state hearts cost kind]
  (if (and (= :idle (:state sub-state)) (>= hearts cost))
    {:fired? true :hearts (- hearts cost) :sub-state {:state :active :timer 0.0 :kind kind}}
    {:fired? false :hearts hearts :sub-state sub-state}))

(defn step-sub-weapon
  ([sub-state dt] (step-sub-weapon sub-state dt default-sub-weapon-active-duration))
  ([{:keys [state timer kind]} dt duration]
   (case state
     :idle {:state :idle :timer 0.0 :kind nil}
     :active (let [timer' (+ timer dt)]
               (if (>= timer' duration)
                 {:state :idle :timer 0.0 :kind nil}
                 {:state :active :timer timer' :kind kind})))))

(defn sub-weapon-active? [sub-state] (= :active (:state sub-state)))

;; -- item crash: whip + sub-weapon simultaneously active ----------------

(def default-item-crash-cost
  "Extra hearts, on top of the sub-weapon's own fire cost, consumed when
   an item crash triggers."
  3)

(defn item-crash?
  "The classic Castlevania 'item crash' combo is reachable only on the
   tick both the whip and the currently-equipped sub-weapon are
   simultaneously :active."
  [whip-state sub-state]
  (and (whip-active? whip-state) (sub-weapon-active? sub-state)))

(defn try-item-crash
  "Consumes `extra-cost` hearts (beyond the sub-weapon's own fire cost)
   the moment both attacks are simultaneously active and enough hearts
   remain; a no-op (false, hearts unchanged) otherwise. Returns
   {:triggered? :hearts}. Callers should only call this once per overlap
   (e.g. gate on a rising edge in their own state) to avoid re-billing
   every tick the overlap persists."
  ([whip-state sub-state hearts] (try-item-crash whip-state sub-state hearts default-item-crash-cost))
  ([whip-state sub-state hearts extra-cost]
   (if (and (item-crash? whip-state sub-state) (>= hearts extra-cost))
     {:triggered? true :hearts (- hearts extra-cost)}
     {:triggered? false :hearts hearts})))

(defn item-crash-hitbox
  "A large, screen-spanning hitbox for the crash's brief active window,
   centered on `entity` ({:x :y})."
  ([entity] (item-crash-hitbox entity 140.0))
  ([{:keys [x y]} radius]
   {:x (- x radius) :y (- y radius) :width (* 2 radius) :height (* 2 radius)}))
