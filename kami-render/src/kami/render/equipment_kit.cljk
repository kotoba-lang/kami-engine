(ns kami.render.equipment-kit
  "Portable semantic equipment/silhouette kit for KAMI characters.

  Parts are asset-independent mesh intents attached to humanoid semantics. A
  host resolves `:mesh-semantic` to an authored glTF/VRM primitive and emits the
  existing render-IR `:meshes` fields (`:id`, `:skin`, `:material`)."
  (:require [kami.render.character-material :as character-material]
            [kami.render.character-preset :as character]))

(def contract :kotoba.render/equipment-kit-v1)

(def family-boundary
  {:families #{:stylized :photoreal}
   :implemented-families #{:stylized}
   :same-api? true
   :rule "Family changes authored assets and materials, never part or attachment semantic ids."})

(def attachment-semantics
  {:head :humanoid/head
   :chest :humanoid/chest
   :hips :humanoid/hips
   :left-shoulder :humanoid/left-upper-arm
   :right-shoulder :humanoid/right-upper-arm
   :right-hand :humanoid/right-hand})

(def ^:private tier-budgets
  {:hero {:max-equipment-triangles 20000 :min-screen-height-px 180}
   :gameplay {:max-equipment-triangles 8000 :min-screen-height-px 72}
   :crowd {:max-equipment-triangles 1800 :min-screen-height-px 24}})

(defn- tier [hero gameplay crowd]
  {:hero hero :gameplay gameplay :crowd crowd})

(def parts
  {:equipment/helmet
   {:part/id :equipment/helmet :mesh-semantic :character.equipment/helmet
    :attachment {:semantic-id :humanoid/head :mode :rigid
                 :local-transform {:position [0.0 0.08 0.0]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}}
    :material-role :metal :silhouette-landmark :head
    :outline-policy {:participates? true :weight 1.0 :crease-weight 0.68}
    :tiers (tier {:enabled? true :triangles 3200 :lod :lod0}
                 {:enabled? true :triangles 1200 :lod :lod1}
                 {:enabled? true :triangles 350 :lod :lod2})
    :variation {:variants 4 :channels #{:crest :side-panel :antenna}}}

   :equipment/visor
   {:part/id :equipment/visor :mesh-semantic :character.equipment/visor
    :attachment {:semantic-id :humanoid/head :mode :rigid
                 :local-transform {:position [0.0 0.04 -0.10]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}}
    :material-role :visor :silhouette-landmark :face
    :outline-policy {:participates? true :weight 0.72 :crease-weight 0.35}
    :tiers (tier {:enabled? true :triangles 800 :lod :lod0}
                 {:enabled? true :triangles 300 :lod :lod1}
                 {:enabled? false :triangles 0 :lod :culled :reason :subpixel-detail})
    :variation {:variants 3 :channels #{:shape :emissive-strip}}}

   :equipment/shoulder-left
   {:part/id :equipment/shoulder-left :mesh-semantic :character.equipment/shoulder-armour
    :attachment {:semantic-id :humanoid/left-upper-arm :mode :rigid
                 :local-transform {:position [0.0 0.12 0.0]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}}
    :material-role :accent :silhouette-landmark :shoulders
    :outline-policy {:participates? true :weight 1.0 :crease-weight 0.72}
    :tiers (tier {:enabled? true :triangles 1200 :lod :lod0}
                 {:enabled? true :triangles 500 :lod :lod1}
                 {:enabled? true :triangles 120 :lod :lod2})
    :variation {:variants 3 :channels #{:profile :decal-slot}}}

   :equipment/shoulder-right
   {:part/id :equipment/shoulder-right :mesh-semantic :character.equipment/shoulder-armour
    :attachment {:semantic-id :humanoid/right-upper-arm :mode :rigid
                 :local-transform {:position [0.0 0.12 0.0]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [-1.0 1.0 1.0]}}
    :material-role :accent :silhouette-landmark :shoulders
    :outline-policy {:participates? true :weight 1.0 :crease-weight 0.72}
    :tiers (tier {:enabled? true :triangles 1200 :lod :lod0}
                 {:enabled? true :triangles 500 :lod :lod1}
                 {:enabled? true :triangles 120 :lod :lod2})
    :variation {:variants 3 :channels #{:profile :decal-slot}}}

   :equipment/chest-armour
   {:part/id :equipment/chest-armour :mesh-semantic :character.equipment/chest-armour
    :attachment {:semantic-id :humanoid/chest :mode :rigid
                 :local-transform {:position [0.0 0.0 -0.03]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}}
    :material-role :accent :silhouette-landmark :torso
    :outline-policy {:participates? true :weight 0.92 :crease-weight 0.78}
    :tiers (tier {:enabled? true :triangles 2600 :lod :lod0}
                 {:enabled? true :triangles 1000 :lod :lod1}
                 {:enabled? true :triangles 300 :lod :lod2})
    :variation {:variants 4 :channels #{:plate-layout :damage-mask :decal-slot}}}

   :equipment/backpack
   {:part/id :equipment/backpack :mesh-semantic :character.equipment/backpack
    :attachment {:semantic-id :humanoid/chest :mode :rigid
                 :local-transform {:position [0.0 0.0 0.18]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}}
    :material-role :cloth :silhouette-landmark :equipment
    :outline-policy {:participates? true :weight 1.0 :crease-weight 0.56}
    :tiers (tier {:enabled? true :triangles 2400 :lod :lod0}
                 {:enabled? true :triangles 800 :lod :lod1}
                 {:enabled? true :triangles 220 :lod :lod2})
    :variation {:variants 4 :channels #{:pouch-layout :roll :antenna}}}

   :equipment/belt
   {:part/id :equipment/belt :mesh-semantic :character.equipment/belt
    :attachment {:semantic-id :humanoid/hips :mode :rigid
                 :local-transform {:position [0.0 0.08 0.0]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}}
    :material-role :cloth :silhouette-landmark :equipment
    :outline-policy {:participates? true :weight 0.82 :crease-weight 0.48}
    :tiers (tier {:enabled? true :triangles 1000 :lod :lod0}
                 {:enabled? true :triangles 350 :lod :lod1}
                 {:enabled? false :triangles 0 :lod :culled :reason :merged-into-torso})
    :variation {:variants 3 :channels #{:pouch-layout :holster-side}}}

   :equipment/weapon-primary
   {:part/id :equipment/weapon-primary :mesh-semantic :character.weapon/stylized-rifle
    :mesh-contract :kotoba.render/weapon-mesh-v2
    :attachment {:semantic-id :humanoid/right-hand :socket :weapon/grip-primary :mode :rigid
                 :local-transform {:position [0.0 0.0 0.0]
                                   :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}}
    :material-role :weapon :silhouette-landmark :weapon
    :outline-policy {:participates? true :weight 1.0 :crease-weight 0.76}
    :tiers (tier {:enabled? true :triangles 6500 :lod :lod0}
                 {:enabled? true :triangles 2400 :lod :lod1}
                 {:enabled? true :triangles 650 :lod :lod2})
    :variation {:variants 5 :channels #{:barrel :optic :stock :magazine}}}})

(defn- char-codes [s]
  #?(:clj (map int s)
     :cljs (map #(.charCodeAt s %) (range (count s)))))

(defn stable-seed
  "Portable deterministic seed over an entity/part id (same on CLJ and CLJS)."
  [value]
  (reduce (fn [acc code] (mod (+ (* acc 31) code) 2147483647))
          7 (char-codes (str value))))

(defn- resolve-part [entity-id tier-id materials [part-id part]]
  (let [tier-data (get-in part [:tiers tier-id])
        variants (get-in part [:variation :variants])
        seed (stable-seed (str entity-id "|" (name part-id)))
        role (:material-role part)]
    [part-id
     (assoc part
            :tier tier-id
            :enabled? (:enabled? tier-data)
            :geometry {:triangles (:triangles tier-data) :lod (:lod tier-data)
                       :reason (:reason tier-data)}
            :material (get materials role)
            :variation (assoc (:variation part) :seed seed :variant-index (mod seed variants))
            :mesh (cond-> {:id part-id :mesh-semantic (:mesh-semantic part)
                           :skin :character/rig :material role
                           :attachment (:attachment part)}
                    (:mesh-contract part) (assoc :mesh-contract (:mesh-contract part))))]))

(defn resolve-kit
  "Resolve all semantic parts for an entity and silhouette tier.

  Disabled tier parts remain in `:parts` as explicit cull evidence. `:meshes`
  contains only enabled render-IR-compatible mesh intents."
  [{:keys [family tier entity-id character-preset team-palette]
    :or {family :stylized tier :gameplay entity-id :character/default
         character-preset :combat-readable}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Equipment visual family is not implemented"
                    {:family family :implemented (:implemented-families family-boundary)})))
  (let [budget (get tier-budgets tier)]
    (when-not budget
      (throw (ex-info "Unknown equipment silhouette tier"
                      {:tier tier :known-tiers (set (keys tier-budgets))})))
    (let [character (character/resolve-character
                     {:family family :preset character-preset :silhouette-tier tier})
          materials (character-material/lower-library character {:team-palette (or team-palette {})})
          resolved (into {} (map (partial resolve-part entity-id tier materials) parts))
          enabled (filter (comp :enabled? val) resolved)
          triangles (reduce + (map #(get-in % [1 :geometry :triangles]) enabled))
          max-triangles (:max-equipment-triangles budget)]
      (when (> triangles max-triangles)
        (throw (ex-info "Equipment kit exceeds tier triangle budget"
                        {:tier tier :triangles triangles :max-triangles max-triangles})))
      {:contract contract
       :family family
       :tier tier
       :entity-id entity-id
       :character-contract (:contract character)
       :skinning {:rig :character/rig
                  :attachment-semantics attachment-semantics
                  :mode :rigid-bone-attachment}
       :parts resolved
       :meshes (mapv (comp :mesh val) enabled)
       :material-registry (select-keys materials (set (map (comp :material-role val) enabled)))
       :budget (assoc budget :resolved-triangles triangles
                      :headroom-triangles (- max-triangles triangles))
       :outline-policy {:mode :screen-space :source :render/style
                        :per-part? true}
       :lod-policy (get-in character [:silhouette :lod-policy])})))

(defn part
  "Resolve one semantic part from a kit, including explicitly culled parts."
  [kit part-id]
  (or (get-in kit [:parts part-id])
      (throw (ex-info "Equipment kit has no semantic part"
                      {:part-id part-id :known-parts (set (keys (:parts kit)))}))))
