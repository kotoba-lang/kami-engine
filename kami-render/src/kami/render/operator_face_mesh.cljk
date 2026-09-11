(ns kami.render.operator-face-mesh
  "Readable visor-compatible stylized facial features authored on the operator head."
  (:require [kami.render.character-material :as character-material]
            [kami.render.character-preset :as character]
            [kotoba.render.mesh :as mesh]))

(def contract :kotoba.render/operator-face-mesh-v1)
(def family-boundary {:families #{:stylized :photoreal}
                      :implemented-families #{:stylized} :same-api? true})

(def ^:private tier-policy
  {:hero {:segments 10 :stacks 6 :slices 10 :max-triangles 1300}
   :gameplay {:segments 8 :stacks 5 :slices 8 :max-triangles 850}
   :crowd {:segments 6 :stacks 4 :slices 6 :max-triangles 520}})

(def ^:private component-order
  [:eye-left :eye-right :eyebrow-left :eyebrow-right :nose :mouth])

(def ^:private specs
  {:eye-left {:form :ellipsoid :size [0.065 0.042 0.022] :center [-0.085 1.82 -0.282]
              :material-role :emissive}
   :eye-right {:form :ellipsoid :size [0.065 0.042 0.022] :center [0.085 1.82 -0.282]
               :material-role :emissive}
   :eyebrow-left {:form :capsule-x :radius 0.014 :length 0.105 :center [-0.085 1.885 -0.278]
                  :material-role :accent}
   :eyebrow-right {:form :capsule-x :radius 0.014 :length 0.105 :center [0.085 1.885 -0.278]
                   :material-role :accent}
   :nose {:form :ellipsoid :size [0.038 0.060 0.032] :center [0.0 1.765 -0.298]
          :material-role :skin}
   :mouth {:form :capsule-x :radius 0.012 :length 0.105 :center [0.0 1.705 -0.286]
           :material-role :accent}})

(defn- normalize3 [[x y z]]
  (let [n (#?(:clj Math/sqrt :cljs js/Math.sqrt) (+ (* x x) (* y y) (* z z)))]
    (if (> n 1.0e-9) [(/ x n) (/ y n) (/ z n)] [0.0 1.0 0.0])))

(defn- transform [[positions normals uvs indices] [sx sy sz] [tx ty tz]]
  {:positions (vec (mapcat (fn [[x y z]] [(+ tx (* sx x)) (+ ty (* sy y)) (+ tz (* sz z))])
                           (partition 3 positions)))
   :normals (vec (mapcat (fn [[x y z]] (normalize3 [(/ x sx) (/ y sy) (/ z sz)]))
                         (partition 3 normals)))
   :uvs (vec uvs) :indices (vec indices)})

(defn- rotate-y-cylinder-to-x [[positions normals uvs indices] [tx ty tz]]
  {:positions (vec (mapcat (fn [[x y z]] [(+ tx y) (+ ty x) (+ tz z)])
                           (partition 3 positions)))
   :normals (vec (mapcat (fn [[x y z]] [y x z]) (partition 3 normals)))
   :uvs (vec uvs) :indices (vec indices)})

(defn mesh-bounds [generated]
  (let [points (partition 3 (:positions generated))
        mn (reduce #(mapv min %1 %2) [##Inf ##Inf ##Inf] points)
        mx (reduce #(mapv max %1 %2) [##-Inf ##-Inf ##-Inf] points)]
    {:min mn :max mx :center (mapv #(* 0.5 (+ %1 %2)) mn mx)
     :half (mapv #(* 0.5 (- %2 %1)) mn mx)}))

(defn- generate [policy {:keys [form size center radius length]}]
  (case form
    :ellipsoid (transform (mesh/sphere (:stacks policy) (:slices policy)) size center)
    :capsule-x (rotate-y-cylinder-to-x
                (mesh/cylinder-pipe radius 0.0 length (:segments policy)) center)))

(defn resolve-face
  "Generate facial landmark geometry in operator bind-world space."
  [{:keys [family tier character-preset team-palette]
    :or {family :stylized tier :gameplay character-preset :combat-readable team-palette {}}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Face visual family is not implemented" {:family family})))
  (let [policy (or (get tier-policy tier)
                   (throw (ex-info "Unknown face tier" {:tier tier})))
        resolved-character (character/resolve-character
                            {:family family :preset character-preset :silhouette-tier tier})
        materials (character-material/lower-library resolved-character {:team-palette team-palette})
        parts (into {}
                    (for [id component-order
                          :let [spec (get specs id) generated (generate policy spec)
                                count-indices (count (:indices generated))]]
                      [id {:semantic-id (keyword "operator.face" (name id))
                           :mesh generated :bounds (mesh-bounds generated)
                           :space :operator-bind-world :lod tier :form (:form spec)
                           :material-role (:material-role spec)
                           :material (get materials (:material-role spec))
                           :material-ranges [{:material-role (:material-role spec)
                                              :index-start 0 :index-count count-indices
                                              :triangle-count (quot count-indices 3)}]}]))
        triangles (reduce + (map #(get-in % [:material-ranges 0 :triangle-count]) (vals parts)))]
    (when (> triangles (:max-triangles policy))
      (throw (ex-info "Face exceeds tier triangle budget" {:tier tier :triangles triangles})))
    {:contract contract :family family :tier tier :space :operator-bind-world
     :head-attachment {:semantic-id :humanoid/head :joint :head}
     :visor-compatible? true :parts parts
     :bounds (mesh-bounds {:positions (vec (mapcat #(get-in % [:mesh :positions]) (vals parts)))})
     :occupancy {:face-area 0.024 :head-area 0.122 :ratio (/ 0.024 0.122)
                 :torso-mask? false}
     :budget {:triangle-count triangles :max-triangles (:max-triangles policy)
              :headroom (- (:max-triangles policy) triangles)}}))
