(ns kami.render.operator-body-mesh
  "Actual indexed stylized humanoid body meshes with portable skinning streams."
  (:require [kami.render.character-material :as character-material]
            [kami.render.character-preset :as character]
            [kami.render.equipment-kit :as equipment]
            [kotoba.render.character :as render-character]
            [kotoba.render.mesh :as mesh]))

(def contract :kotoba.render/operator-body-mesh-v1)

(def family-boundary
  {:families #{:stylized :photoreal}
   :implemented-families #{:stylized}
   :same-api? true
   :rule "Family changes forms and surface realization, never body-part, joint, or attachment ids."})

(def ^:private component-order
  [:pelvis :torso :neck :head
   :upper-arm-left :lower-arm-left :hand-left
   :upper-arm-right :lower-arm-right :hand-right
   :upper-leg-left :lower-leg-left :boot-left
   :upper-leg-right :lower-leg-right :boot-right])

(def ^:private tier-policy
  {:hero {:segments 12 :sphere-stacks 8 :sphere-slices 12 :max-triangles 5200}
   :gameplay {:segments 8 :sphere-stacks 5 :sphere-slices 8 :max-triangles 2300}
   :crowd {:segments 6 :sphere-stacks 4 :sphere-slices 6 :max-triangles 1400}})

(def attachment-points
  "Attachment semantics shared with equipment-kit, expressed in bind pose."
  {:humanoid/head {:joint :head :position [0.0 1.88 0.0]}
   :humanoid/chest {:joint :chest :position [0.0 1.48 0.0]}
   :humanoid/hips {:joint :hips :position [0.0 1.08 0.0]}
   :humanoid/left-upper-arm {:joint :upper-arm-left :position [-0.36 1.46 0.0]}
   :humanoid/right-upper-arm {:joint :upper-arm-right :position [0.36 1.46 0.0]}
   :humanoid/right-hand {:joint :hand-right :position [0.43 0.88 0.0]}
   :humanoid/left-hand {:joint :hand-left :position [-0.43 0.88 0.0]}})

(defn- variant-parameters [seed]
  {:shoulder-scale (+ 0.94 (* 0.03 (mod seed 5)))
   :head-scale (+ 0.95 (* 0.025 (mod (quot seed 5) 5)))
   :torso-scale (+ 0.94 (* 0.03 (mod (quot seed 25) 5)))
   :boot-scale (+ 0.94 (* 0.03 (mod (quot seed 125) 5)))})

(defn- normalize3 [[x y z]]
  (let [length (#?(:clj Math/sqrt :cljs js/Math.sqrt) (+ (* x x) (* y y) (* z z)))]
    (if (> length 1.0e-8) [(/ x length) (/ y length) (/ z length)] [0.0 1.0 0.0])))

(defn- transform [[positions normals uvs indices] [sx sy sz] [tx ty tz]]
  {:positions (vec (mapcat (fn [[x y z]] [(+ tx (* sx x)) (+ ty (* sy y)) (+ tz (* sz z))])
                           (partition 3 positions)))
   :normals (vec (mapcat (fn [[x y z]] (normalize3 [(/ x sx) (/ y sy) (/ z sz)]))
                         (partition 3 normals)))
   :uvs (vec uvs) :indices (vec indices)})

(defn- combine [meshes]
  (reduce
   (fn [{:keys [positions normals uvs indices]} generated]
     (let [base (quot (count positions) 3)]
       {:positions (into positions (:positions generated))
        :normals (into normals (:normals generated))
        :uvs (into uvs (:uvs generated))
        :indices (into indices (map #(+ base %) (:indices generated)))}))
   {:positions [] :normals [] :uvs [] :indices []} meshes))

(defn- sphere-form [{:keys [sphere-stacks sphere-slices]} size center]
  (transform (mesh/sphere sphere-stacks sphere-slices) size center))

(defn- cylinder-form [{:keys [segments]} radius length center]
  (transform (mesh/cylinder-pipe radius 0.0 length segments) [1.0 1.0 1.0] center))

(defn- capsule-form [policy radius length [x y z]]
  (combine [(cylinder-form policy radius length [x y z])
            (sphere-form policy [(* radius 2.0) (* radius 2.0) (* radius 2.0)]
                         [x (+ y (* length 0.5)) z])
            (sphere-form policy [(* radius 2.0) (* radius 2.0) (* radius 2.0)]
                         [x (- y (* length 0.5)) z])]))

(defn- component-specs [variant]
  (let [{:keys [shoulder-scale head-scale torso-scale boot-scale]} variant
        sx (* 0.40 shoulder-scale)]
    {:head {:shape :sphere :size [(* 0.31 head-scale) (* 0.36 head-scale) (* 0.29 head-scale)]
            :center [0.0 1.78 0.0] :material-role :skin :weights [[:head 1.0]]}
     :neck {:shape :cylinder :radius 0.085 :length 0.13 :center [0.0 1.59 0.0]
            :material-role :skin :weights [[:neck 0.72] [:head 0.28]]}
     :torso {:shape :beveled-form :size [(* 0.66 torso-scale) 0.64 0.34]
             :center [0.0 1.35 0.0] :material-role :cloth
             :weights [[:spine 0.35] [:chest 0.65]]}
     :pelvis {:shape :beveled-form :size [0.48 0.32 0.30] :center [0.0 1.04 0.0]
              :material-role :accent :weights [[:hips 1.0]]}
     :upper-arm-left {:shape :capsule :radius 0.105 :length 0.36 :center [(- sx) 1.34 0.0]
                      :material-role :cloth :weights [[:shoulder-left 0.22] [:upper-arm-left 0.78]]}
     :lower-arm-left {:shape :capsule :radius 0.086 :length 0.34 :center [(- sx) 0.99 0.0]
                      :material-role :cloth :weights [[:upper-arm-left 0.20] [:lower-arm-left 0.80]]}
     :hand-left {:shape :sphere :size [0.15 0.19 0.12] :center [(- sx) 0.76 0.0]
                 :material-role :skin :weights [[:hand-left 1.0]]}
     :upper-arm-right {:shape :capsule :radius 0.105 :length 0.36 :center [sx 1.34 0.0]
                       :material-role :cloth :weights [[:shoulder-right 0.22] [:upper-arm-right 0.78]]}
     :lower-arm-right {:shape :capsule :radius 0.086 :length 0.34 :center [sx 0.99 0.0]
                       :material-role :cloth :weights [[:upper-arm-right 0.20] [:lower-arm-right 0.80]]}
     :hand-right {:shape :sphere :size [0.15 0.19 0.12] :center [sx 0.76 0.0]
                  :material-role :skin :weights [[:hand-right 1.0]]}
     :upper-leg-left {:shape :capsule :radius 0.14 :length 0.42 :center [-0.15 0.78 0.0]
                      :material-role :cloth :weights [[:hips 0.18] [:upper-leg-left 0.82]]}
     :lower-leg-left {:shape :capsule :radius 0.115 :length 0.39 :center [-0.15 0.37 0.0]
                      :material-role :cloth :weights [[:upper-leg-left 0.18] [:lower-leg-left 0.82]]}
     :boot-left {:shape :beveled-form :size [(* 0.25 boot-scale) 0.20 (* 0.42 boot-scale)]
                 :center [-0.15 0.10 0.075] :material-role :metal :weights [[:foot-left 1.0]]}
     :upper-leg-right {:shape :capsule :radius 0.14 :length 0.42 :center [0.15 0.78 0.0]
                       :material-role :cloth :weights [[:hips 0.18] [:upper-leg-right 0.82]]}
     :lower-leg-right {:shape :capsule :radius 0.115 :length 0.39 :center [0.15 0.37 0.0]
                       :material-role :cloth :weights [[:upper-leg-right 0.18] [:lower-leg-right 0.82]]}
     :boot-right {:shape :beveled-form :size [(* 0.25 boot-scale) 0.20 (* 0.42 boot-scale)]
                  :center [0.15 0.10 0.075] :material-role :metal :weights [[:foot-right 1.0]]}}))

(defn- component-mesh [policy {:keys [shape size center radius length]}]
  (case shape
    :sphere (sphere-form policy size center)
    :beveled-form (sphere-form policy size center)
    :cylinder (cylinder-form policy radius length center)
    :capsule (capsule-form policy radius length center)))

(defn- lanes [influences]
  (let [padded (take 4 (concat influences (repeat [:root 0.0])))]
    {:joints (mapv #(get render-character/joint-index (first %)) padded)
     :weights (mapv second padded)}))

(defn- append-component [{:keys [positions normals uvs indices joints weights ranges]}
                         policy component-id spec]
  (let [generated (component-mesh policy spec)
        vertex-base (quot (count positions) 3)
        vertex-count (quot (count (:positions generated)) 3)
        index-start (count indices)
        component-indices (mapv #(+ vertex-base %) (:indices generated))
        skin (lanes (:weights spec))]
    {:positions (into positions (:positions generated))
     :normals (into normals (:normals generated))
     :uvs (into uvs (:uvs generated))
     :indices (into indices component-indices)
     :joints (into joints (repeat vertex-count (:joints skin)))
     :weights (into weights (repeat vertex-count (:weights skin)))
     :ranges (conj ranges {:component component-id :material-role (:material-role spec)
                           :index-start index-start :index-count (count component-indices)
                           :triangle-count (quot (count component-indices) 3)})}))

(defn resolve-operator
  "Generate an actual indexed, skinned stylized operator body for one tier."
  [{:keys [family tier entity-id character-preset team-palette]
    :or {family :stylized tier :gameplay entity-id :operator/default
         character-preset :combat-readable team-palette {}}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Operator body visual family is not implemented"
                    {:family family :implemented (:implemented-families family-boundary)})))
  (let [policy (get tier-policy tier)]
    (when-not policy
      (throw (ex-info "Unknown operator body tier"
                      {:tier tier :known-tiers (set (keys tier-policy))})))
    (let [seed (equipment/stable-seed entity-id)
          variant (variant-parameters seed)
          specs (component-specs variant)
          assembled (reduce (fn [acc id] (append-component acc policy id (get specs id)))
                            {:positions [] :normals [] :uvs [] :indices []
                             :joints [] :weights [] :ranges []}
                            component-order)
          triangles (quot (count (:indices assembled)) 3)
          resolved-character (character/resolve-character
                              {:family family :preset character-preset :silhouette-tier tier})
          materials (character-material/lower-library
                     resolved-character {:team-palette team-palette})
          used-roles (set (map :material-role (:ranges assembled)))]
      (when (> triangles (:max-triangles policy))
        (throw (ex-info "Generated operator body exceeds tier triangle budget"
                        {:tier tier :triangles triangles :max-triangles (:max-triangles policy)})))
      {:contract contract :family family :tier tier :entity-id entity-id
       :mesh {:positions (:positions assembled) :normals (:normals assembled)
              :uvs (:uvs assembled) :indices (:indices assembled)
              :joints (:joints assembled) :weights (:weights assembled)}
       :material-ranges (:ranges assembled)
       :materials (select-keys materials used-roles)
       :components (into {} (for [id component-order
                                  :let [spec (get specs id)]]
                              [id {:semantic-id (keyword "operator.body" (name id))
                                   :shape (:shape spec) :material-role (:material-role spec)
                                   :skinning (if (= 1 (count (:weights spec))) :rigid :two-bone-blend)
                                   :influences (:weights spec)}]))
       :rig {:joint-order render-character/joint-order :joint-index render-character/joint-index
             :mode :linear-blend-skinning :lanes 4}
       :attachments attachment-points
       :equipment-compatible? true
       :silhouette {:source (get character/silhouette-tiers tier)
                    :proportions variant}
       :outline-policy {:mode :screen-space :source :render/style
                        :material-roles used-roles}
       :variation {:seed seed :parameters variant}
       :budget {:triangle-count triangles :max-triangles (:max-triangles policy)
                :headroom (- (:max-triangles policy) triangles)}})))
