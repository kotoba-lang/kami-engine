(ns kami.render.fitted-equipment-mesh
  "Actual curved stylized equipment and readable grip-hand meshes fitted to operator-fit bounds."
  (:require [kami.render.character-material :as character-material]
            [kami.render.character-preset :as character]
            [kami.render.operator-fit :as fit]
            [kotoba.render.mesh :as mesh]))

(def contract :kotoba.render/fitted-equipment-mesh-v1)
(def family-boundary {:families #{:stylized :photoreal}
                      :implemented-families #{:stylized} :same-api? true})

(def ^:private tier-policy
  {:hero {:segments 12 :stacks 8 :slices 12 :max-triangles 3400}
   :gameplay {:segments 8 :stacks 5 :slices 8 :max-triangles 1700}
   :crowd {:segments 6 :stacks 4 :slices 6 :max-triangles 900}})

(def ^:private forms
  {:equipment/helmet {:forms [:dome :rim] :material-role :metal}
   :equipment/visor {:forms [:curved-lens] :material-role :visor}
   :equipment/shoulder-left {:forms [:capsule-plate] :material-role :accent}
   :equipment/shoulder-right {:forms [:capsule-plate] :material-role :accent}
   :equipment/chest-armour {:forms [:beveled-shell] :material-role :accent}
   :equipment/backpack {:forms [:rounded-pack :utility-roll] :material-role :cloth}
   :equipment/belt {:forms [:curved-belt] :material-role :cloth}})

(defn- normalize3 [[x y z]]
  (let [n (#?(:clj Math/sqrt :cljs js/Math.sqrt) (+ (* x x) (* y y) (* z z)))]
    (if (> n 1.0e-9) [(/ x n) (/ y n) (/ z n)] [0.0 1.0 0.0])))

(defn- transform [[positions normals uvs indices] [sx sy sz] [tx ty tz]]
  {:positions (vec (mapcat (fn [[x y z]] [(+ tx (* sx x)) (+ ty (* sy y)) (+ tz (* sz z))])
                           (partition 3 positions)))
   :normals (vec (mapcat (fn [[x y z]] (normalize3 [(/ x sx) (/ y sy) (/ z sz)]))
                         (partition 3 normals)))
   :uvs (vec uvs) :indices (vec indices)})

(defn- combine [meshes]
  (reduce (fn [{:keys [positions normals uvs indices]} m]
            (let [base (quot (count positions) 3)]
              {:positions (into positions (:positions m))
               :normals (into normals (:normals m)) :uvs (into uvs (:uvs m))
               :indices (into indices (map #(+ base %) (:indices m)))}))
          {:positions [] :normals [] :uvs [] :indices []} meshes))

(defn- sphere [policy size center]
  (transform (mesh/sphere (:stacks policy) (:slices policy)) size center))
(defn- cylinder [policy radius length center]
  (transform (mesh/cylinder-pipe radius 0.0 length (:segments policy)) [1.0 1.0 1.0] center))
(defn- capsule [policy radius length [x y z]]
  (combine [(cylinder policy radius length [x y z])
            (sphere policy [(* 2 radius) (* 2 radius) (* 2 radius)] [x (+ y (* length 0.5)) z])
            (sphere policy [(* 2 radius) (* 2 radius) (* 2 radius)] [x (- y (* length 0.5)) z])]))

(defn mesh-bounds [mesh]
  (let [points (partition 3 (:positions mesh))
        mn (reduce #(mapv min %1 %2) [##Inf ##Inf ##Inf] points)
        mx (reduce #(mapv max %1 %2) [##-Inf ##-Inf ##-Inf] points)]
    {:min mn :max mx :center (mapv #(* 0.5 (+ %1 %2)) mn mx)
     :half (mapv #(* 0.5 (- %2 %1)) mn mx)}))

(defn- fit-to-volume [generated {:keys [center half]}]
  (let [{source-center :center source-half :half} (mesh-bounds generated)
        scale (mapv / half source-half)
        [sx sy sz] scale [cx cy cz] source-center [tx ty tz] center]
    {:positions (vec (mapcat (fn [[x y z]] [(+ tx (* sx (- x cx)))
                                            (+ ty (* sy (- y cy)))
                                            (+ tz (* sz (- z cz)))])
                             (partition 3 (:positions generated))))
     :normals (vec (mapcat (fn [[x y z]] (normalize3 [(/ x sx) (/ y sy) (/ z sz)]))
                           (partition 3 (:normals generated))))
     :uvs (:uvs generated) :indices (:indices generated)}))

(defn- raw-part [policy part-id]
  (case part-id
    :equipment/helmet (combine [(sphere policy [0.38 0.34 0.32] [0.0 0.03 0.0])
                                (cylinder policy 0.20 0.055 [0.0 -0.13 0.0])])
    :equipment/visor (sphere policy [0.34 0.13 0.055] [0.0 0.0 0.0])
    (:equipment/shoulder-left :equipment/shoulder-right)
    (capsule policy 0.12 0.14 [0.0 0.0 0.0])
    :equipment/chest-armour (sphere policy [0.60 0.48 0.12] [0.0 0.0 0.0])
    :equipment/backpack (combine [(sphere policy [0.48 0.56 0.22] [0.0 0.0 0.0])
                                  (cylinder policy 0.08 0.36 [0.16 0.0 0.0])])
    :equipment/belt (cylinder policy 0.27 0.10 [0.0 0.0 0.0])))

(defn- grip-hand [policy side center]
  (let [sign (if (= side :left) -1.0 1.0)
        raw (combine [(sphere policy [0.16 0.19 0.13] [0.0 0.0 0.0])
                      (capsule policy 0.035 0.10 [(* sign -0.075) 0.0 -0.015])])
        volume {:center center :half [0.095 0.115 0.08]}]
    (let [socket (if (= side :left) :weapon/grip-support :weapon/grip-primary)]
      {:mesh (fit-to-volume raw volume) :volume (assoc volume :kind :aabb)
       :forms [:mitten-palm :grip-thumb :mitten-wrap-contact] :material-role :skin
       :socket socket
       :contact {:socket socket :position center :form :mitten-wrap-contact}})))

(defn resolve-meshes
  "Generate fitted actual meshes from authoritative operator-fit data."
  [{:keys [family tier entity-id character-preset team-palette]
    :or {family :stylized tier :gameplay entity-id :operator/default
         character-preset :combat-readable team-palette {}}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Fitted equipment mesh family is not implemented" {:family family})))
  (let [policy (or (get tier-policy tier)
                   (throw (ex-info "Unknown fitted equipment tier" {:tier tier})))
        resolved-fit (fit/resolve-fit {:family family :tier tier :entity-id entity-id
                                       :character-preset character-preset :team-palette team-palette})
        resolved-character (character/resolve-character
                            {:family family :preset character-preset :silhouette-tier tier})
        materials (character-material/lower-library resolved-character {:team-palette team-palette})
        equipment-meshes
        (into {}
              (for [[part-id fitted] (:equipment resolved-fit)
                    :let [spec (get forms part-id)
                          generated (fit-to-volume (raw-part policy part-id) (:volume fitted))
                          triangles (quot (count (:indices generated)) 3)]]
                [part-id {:mesh generated :bounds (mesh-bounds generated)
                          :space :operator-bind-world :lod tier
                          :fit-volume (:volume fitted) :transform (:transform fitted)
                          :forms (:forms spec) :material-role (:material-role spec)
                          :material (get materials (:material-role spec))
                          :material-ranges [{:material-role (:material-role spec)
                                             :index-start 0 :index-count (count (:indices generated))
                                             :triangle-count triangles}]}]))
        left-center (get-in resolved-fit [:arm-chains :left :centers :hand])
        right-center (get-in resolved-fit [:arm-chains :right :centers :hand])
        hands (into {}
                    (for [[hand-id hand] {:hand-left (grip-hand policy :left left-center)
                                          :hand-right (grip-hand policy :right right-center)}
                          :let [index-count (count (get-in hand [:mesh :indices]))]]
                      [hand-id (assoc hand
                                      :space :operator-bind-world :lod tier
                                      :bounds (mesh-bounds (:mesh hand))
                                      :material (get materials :skin)
                                      :material-ranges [{:material-role :skin :index-start 0
                                                         :index-count index-count
                                                         :triangle-count (quot index-count 3)}])]))
        equipment-tris (reduce + (for [[_ p] equipment-meshes]
                                    (quot (count (get-in p [:mesh :indices])) 3)))
        hand-tris (reduce + (for [[_ h] hands] (quot (count (get-in h [:mesh :indices])) 3)))
        total (+ equipment-tris hand-tris)]
    (when (> total (:max-triangles policy))
      (throw (ex-info "Fitted equipment exceeds tier triangle budget"
                      {:tier tier :triangles total :max-triangles (:max-triangles policy)})))
    {:contract contract :family family :tier tier :entity-id entity-id
     :space :operator-bind-world
     :lod {:tier tier :segments (:segments policy) :stacks (:stacks policy)
           :slices (:slices policy)}
     :fit-contract fit/contract :equipment equipment-meshes :hands hands
     :sockets (get-in resolved-fit [:weapon :sockets])
     :arm-chains (:arm-chains resolved-fit)
     :occupancy (get-in resolved-fit [:validation :silhouette-occupancy])
     :budget {:triangle-count total :max-triangles (:max-triangles policy)
              :headroom (- (:max-triangles policy) total)}}))
