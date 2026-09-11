(ns kami.gameplay.loot
  "Ground loot, pickups and consumables.

  `battle_royale_consumables.edn` describes eleven items with use times, caps
  and stack limits, and is the third table in this repository whose runtime left
  with the Rust workspace. Looting is also the mechanic that gives a battle
  royale map a *reason to have places in it*: without it, every point on the
  ground is interchangeable and the level is a flat arena — which is exactly how
  the shipped royale plays.

  Rarity-weighted drops use the shared deterministic RNG, so two clients rolling
  the same floor loot from the same seed get the same guns."
  (:require [kami.gameplay.vec3 :as v]
            [kami.gameplay.rng :as rng]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.weapon :as weapon]
            [kami.gameplay.damage :as damage]))

(def rarity-weight
  "Relative drop weight per rarity. A legendary is ~30x rarer than a common,
  which is what makes finding one an event rather than a tick of the clock."
  {:common 60 :uncommon 25 :rare 10 :epic 4 :legendary 2})

(defn load-consumables
  [data]
  (let [rows (if (map? data) (:battle-royale/consumables data) data)]
    (vec (map-indexed (fn [i c]
                        (-> c
                            (assoc :index i)
                            (update :use-time double)
                            (update :hp-restore double)
                            (update :shield-restore double)
                            (update :hp-cap double)
                            (update :shield-cap double)
                            (update :stack long)))
                      rows))))

(defn- weighted-pick
  "Pick one row by `:rarity` weight. Returns `[row rng']`."
  [rs rows]
  (let [total (reduce + 0 (map #(get rarity-weight (:rarity %) 1) rows))]
    (if (or (empty? rows) (zero? total))
      [nil (rng/next-state rs)]
      (let [[roll rs'] (rng/int rs total)]
        (loop [acc 0 [r & more] rows]
          (let [acc' (+ acc (get rarity-weight (:rarity r) 1))]
            (if (or (< roll acc') (nil? more))
              [r rs']
              (recur acc' more))))))))

(defn spawn-ground-loot
  "Scatter `count` pickups inside `radius` of `center`. Returns `[pickups rng']`.

  Each pickup is a plain map, not an ECS entity: ground loot has no behaviour,
  and keeping it out of the entity table keeps `alive-count` — which decides
  when the match ends — counting people."
  [rs {:keys [weapons consumables count radius center ammo-per-drop]
       :or {count 40 radius 300.0 center [0.0 0.0 0.0] ammo-per-drop 90}}]
  (loop [n count, rs rs, out [], id 0]
    (if (zero? n)
      [out rs]
      (let [[kind rs1] (rng/int rs 10)
            [ang rs2] (rng/range-f rs1 0.0 (* 2.0 Math/PI))
            [fr rs3] (rng/unit rs2)
            d (* (double radius) (Math/sqrt fr))
            pos [(+ (v/x center) (* d (Math/cos ang)))
                 (v/y center)
                 (+ (v/z center) (* d (Math/sin ang)))]]
        (if (< kind 6)
          (let [[w rs4] (weighted-pick rs3 weapons)]
            (recur (dec n) rs4
                   (conj out {:id id :pickup :weapon :weapon (:index w)
                              :ammo ammo-per-drop :pos pos :rarity (:rarity w)
                              :name (:name w)})
                   (inc id)))
          (let [[c rs4] (weighted-pick rs3 consumables)]
            (recur (dec n) rs4
                   (conj out {:id id :pickup :consumable :consumable (:index c)
                              :pos pos :rarity (:rarity c) :name (:name c)})
                   (inc id))))))))

(defn in-reach
  "Pickups within `radius` of `pos`, nearest first."
  [pickups pos radius]
  (->> pickups
       (filter #(<= (v/dist-xz (:pos %) pos) (double radius)))
       (sort-by #(v/dist-xz (:pos %) pos))
       vec))

(defn take-pickup
  "Apply a pickup to an entity. Returns `[entity' event]`.

  A weapon pickup swaps the held weapon and banks the reserve ammo; a
  consumable is not consumed here — it goes into the backpack, because using it
  costs `:use-time` and is a separate, interruptible action."
  [table e pickup]
  (case (:pickup pickup)
    :weapon
    [(-> (weapon/equip table e (:weapon pickup))
         (attr/update-attr :reserve-ammo + (double (:ammo pickup 0))))
     {:kind :pickup :what :weapon :weapon (:weapon pickup) :name (:name pickup)}]

    :consumable
    [(update-in e [:backpack (:consumable pickup)] (fnil inc 0))
     {:kind :pickup :what :consumable :consumable (:consumable pickup) :name (:name pickup)}]

    [e {:kind :blocked :reason :unknown-pickup}]))

(defn use-consumable
  "Consume one item from the backpack. Returns `[world' events]`.

  Honours the item's own `:hp-cap`/`:shield-cap` — a Bandage stops at 75 health
  even though the entity's maximum is 100, which is the rule that makes the
  item table meaningful rather than a list of numbers."
  [world entity-id consumables idx]
  (let [e (get-in world [:entities entity-id])
        item (first (filter #(= (:index %) idx) consumables))
        held (get-in e [:backpack idx] 0)]
    (cond
      (nil? item) [world [{:kind :blocked :reason :unknown-consumable :index idx}]]
      (zero? held) [world [{:kind :blocked :reason :none-held :index idx}]]
      (not (attr/alive? e)) [world [{:kind :blocked :reason :not-alive}]]
      :else
      (let [world (assoc-in world [:entities entity-id :backpack idx] (dec held))
            hp-room (max 0.0 (- (min (:hp-cap item) (attr/get e :health-max))
                                (attr/get e :health)))
            sh-room (max 0.0 (- (min (:shield-cap item) (attr/get e :shield-max))
                                (attr/get e :shield)))
            [world ev1] (damage/heal world entity-id (min (:hp-restore item) hp-room))
            [world ev2] (damage/add-shield world entity-id (min (:shield-restore item) sh-room))]
        [world [(assoc ev1 :consumable idx :name (:name item))
                (assoc ev2 :consumable idx :name (:name item))]]))))
