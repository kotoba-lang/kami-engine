(ns kami.gameplay.weapon
  "Weapons as data, and the firing state machine that reads them.

  `kami-game-scene/data/battle_royale_weapons.edn` has held a 25-entry weapon
  table — damage, headshot multiplier, fire rate, magazine, reload time, spread,
  falloff, range, projectile speed — since ADR-0046. Nothing has been able to
  execute it since the Rust `weapon_pool()` that was its parity oracle left the
  repository. The table describes weapons no code owns, and the game it was
  written for auto-fires a hitscan with no ammo, no reload and no spread.

  This namespace is the runtime that table has been missing. It is deliberately
  a *state machine over attributes* rather than an object: equip/fire/reload are
  functions from entity to entity, so the whole weapon system replays from a
  seed like everything else here.

  Fire rate, reload and magazine are enforced in one place — `can-fire?` — for
  a specific reason. Rate limiting scattered across callers is how a weapon ends
  up firing on the frame its reload finishes, and how a client and a server
  disagree about whether a shot happened."
  (:require [kami.gameplay.attributes :as attr]))

(defn load-table
  "Normalise a parsed `battle_royale_weapons.edn` into an indexed vector.

  Accepts either the wrapper map `{:battle-royale/weapons [...]}` or a bare
  vector, so a caller can hand over the file contents or a test fixture without
  a shape adapter. Every stat is coerced to double at load: the shipped table
  mixes ints and floats (`:damage 30` next to `:fire-rate 5.5`) and a division
  that silently truncates is exactly the class of bug integer stats invite."
  [data]
  (let [rows (if (map? data) (:battle-royale/weapons data) data)]
    (vec (map-indexed
           (fn [i w]
             (-> w
                 (assoc :index i)
                 (update :damage double)
                 (update :headshot-mult double)
                 (update :fire-rate double)
                 (update :magazine long)
                 (update :reload-time double)
                 (update :spread double)
                 (update :damage-falloff double)
                 (update :range double)
                 (update :projectile-speed double)))
           rows))))

(defn by-index
  "The weapon row at `i`, or nil. `-1` (unarmed) is a legitimate value, not an
  error — a downed player and a fresh drop both hold it."
  [table i]
  (let [i (long i)]
    (when (and (>= i 0) (< i (count table))) (nth table i))))

(defn equipped
  "The weapon row an entity currently holds, or nil when unarmed."
  [table e]
  (by-index table (long (attr/get e :weapon))))

(defn find-by
  "Rows matching a predicate map, e.g. `{:type :sniper-rifle :rarity :legendary}`."
  [table m]
  (filterv (fn [w] (every? (fn [[k v]] (= (get w k) v)) m)) table))

(def ^:const hitscan-speed-threshold
  "Above this muzzle velocity a shot resolves instantly.

  Derived from the shipped table rather than picked: every bullet weapon in it
  sits at 400-1000 u/s and every launcher at 90-125, so the gap is an order of
  magnitude wide and 200 falls in the middle of it. A threshold rather than a
  new column because the table has no such column and the physical distinction
  is real — a 500 u/s rifle round crosses a duelling distance inside one frame,
  a 100 u/s rocket does not and must be dodgeable."
  200.0)

(defn hitscan?
  "Whether a weapon resolves instantly or launches a travelling projectile."
  [w]
  (>= (double (:projectile-speed w 0)) hitscan-speed-threshold))

(defn shots-per-second [w] (double (:fire-rate w 1.0)))

(defn fire-interval-ms
  "Milliseconds between shots. Guards a zero/absent fire rate rather than
  dividing by it — a malformed row should give a slow weapon, not an infinity
  that makes `next-fire-at` un-representable."
  [w]
  (let [r (shots-per-second w)]
    (if (pos? r) (/ 1000.0 r) 1000.0)))

(defn equip
  "Hold weapon `i`, filling the magazine and cancelling any reload in flight.

  Cancelling matters: picking up a new gun mid-reload and having the old
  reload's completion refill the new gun is a classic duplication bug."
  [table e i]
  (let [w (by-index table i)]
    (-> e
        (attr/set :weapon (if w (:index w) -1))
        (attr/set :ammo (if w (:magazine w) 0))
        (attr/set :reload-until 0.0))))

(defn reloading? [e now-ms] (> (attr/get e :reload-until) (double now-ms)))

(defn can-fire?
  "Every gate on pulling the trigger, in one place.

  Alive, armed, not mid-reload, has a round chambered, and the fire-rate
  interval has elapsed."
  [table e now-ms]
  (let [w (equipped table e)]
    (boolean
      (and w
           (attr/alive? e)
           (not (attr/downed? e))
           (not (reloading? e now-ms))
           (pos? (attr/get e :ammo))
           (>= (double now-ms) (attr/get e :next-fire-at))))))

(defn begin-reload
  "Start a reload if one is warranted: armed, not already reloading, magazine
  not already full, and reserve ammo available.

  Returns the entity unchanged when the reload is not warranted, so a player
  who spams the reload key does not repeatedly restart the timer — the other
  classic bug in this area."
  [table e now-ms]
  (let [w (equipped table e)]
    (if (and w
             (not (reloading? e now-ms))
             (< (attr/get e :ammo) (double (:magazine w)))
             (pos? (attr/get e :reserve-ammo)))
      (attr/set e :reload-until (+ (double now-ms) (* 1000.0 (:reload-time w))))
      e)))

(defn finish-reload
  "Complete a due reload, moving rounds from the reserve into the magazine.

  Partial reloads are honoured — a reserve of 7 into an empty 30-round magazine
  gives 7, not 30 and not nothing."
  [table e now-ms]
  (let [w (equipped table e)]
    (if (and w (pos? (attr/get e :reload-until)) (<= (attr/get e :reload-until) (double now-ms)))
      (let [want (- (double (:magazine w)) (attr/get e :ammo))
            take (min want (attr/get e :reserve-ammo))]
        (-> e
            (attr/update-attr :ammo + take)
            (attr/update-attr :reserve-ammo - take)
            (attr/set :reload-until 0.0)))
      e)))

(defn consume-shot
  "Spend one round and arm the fire-rate gate.

  Auto-starts a reload when that was the last round: the alternative is a
  player holding an empty gun wondering why the trigger does nothing."
  [table e now-ms]
  (let [e' (-> e
               (attr/update-attr :ammo - 1.0)
               (attr/set :next-fire-at (+ (double now-ms) (fire-interval-ms (equipped table e)))))]
    (if (zero? (attr/get e' :ammo))
      (begin-reload table e' now-ms)
      e')))

(defn step
  "Per-tick weapon upkeep for one entity: finish a due reload."
  [table e now-ms]
  (finish-reload table e now-ms))
