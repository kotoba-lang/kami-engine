(ns kami.render.operator-fit
  "Validated operator pose and attachment transforms for stylized characters."
  (:require [kami.render.arm-ik :as arm-ik]
            [kami.render.equipment-kit :as equipment]
            [kami.render.operator-body-mesh :as body]
            [kami.render.weapon-mesh :as weapon]))

(def contract :kotoba.render/operator-fit-v1)

(def family-boundary
  {:families #{:stylized :photoreal}
   :implemented-families #{:stylized}
   :same-api? true})

(def ^:private tier-budgets
  {:hero {:minimum-clearance 0.018 :maximum-silhouette-occupancy 0.52
          :minimum-grip-separation 0.40}
   :gameplay {:minimum-clearance 0.015 :maximum-silhouette-occupancy 0.42
              :minimum-grip-separation 0.34}
   :crowd {:minimum-clearance 0.010 :maximum-silhouette-occupancy 0.28
           :minimum-grip-separation 0.20}})

(def ^:private body-volumes
  {:head {:kind :aabb :center [0.0 1.78 0.0] :half [0.17 0.20 0.16]}
   :torso {:kind :aabb :center [0.0 1.35 0.0] :half [0.35 0.34 0.18]}
   :pelvis {:kind :aabb :center [0.0 1.04 0.0] :half [0.26 0.18 0.17]}
   :upper-arm-left {:kind :aabb :center [-0.40 1.34 0.0] :half [0.12 0.24 0.12]}
   :upper-arm-right {:kind :aabb :center [0.40 1.34 0.0] :half [0.12 0.24 0.12]}})

(def equipment-layout
  "Authoritative fitted transforms and bounds consumed by equipment mesh authors."
  {:equipment/helmet
   {:transform {:position [0.0 1.90 0.0] :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}
    :volume {:kind :aabb :center [0.0 1.90 0.0] :half [0.19 0.19 0.17]}
    :mounted-on :head :silhouette-area 0.102}
   :equipment/visor
   {:transform {:position [0.0 1.82 -0.20] :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}
    :volume {:kind :aabb :center [0.0 1.82 -0.20] :half [0.16 0.07 0.025]}
    :mounted-on :head :silhouette-area 0.020}
   :equipment/shoulder-left
   {:transform {:position [-0.52 1.47 0.0] :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}
    :volume {:kind :aabb :center [-0.52 1.47 0.0] :half [0.12 0.11 0.14]}
    :mounted-on :upper-arm-left :silhouette-area 0.034}
   :equipment/shoulder-right
   {:transform {:position [0.52 1.47 0.0] :rotation [0.0 0.0 0.0 1.0] :scale [-1.0 1.0 1.0]}
    :volume {:kind :aabb :center [0.52 1.47 0.0] :half [0.12 0.11 0.14]}
    :mounted-on :upper-arm-right :silhouette-area 0.034}
   :equipment/chest-armour
   {:transform {:position [0.0 1.38 -0.245] :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}
    :volume {:kind :aabb :center [0.0 1.38 -0.245] :half [0.30 0.24 0.055]}
    :mounted-on :torso :silhouette-area 0.183}
   :equipment/backpack
   {:transform {:position [0.0 1.37 0.43] :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}
    :volume {:kind :aabb :center [0.0 1.37 0.43] :half [0.24 0.29 0.12]}
    :silhouette-area 0.110}
   :equipment/belt
   {:transform {:position [0.0 0.90 -0.19] :rotation [0.0 0.0 0.0 1.0] :scale [1.0 1.0 1.0]}
    :volume {:kind :aabb :center [0.0 0.90 -0.19] :half [0.27 0.08 0.05]}
    :mounted-on :pelvis :silhouette-area 0.060}})

(defn- v+ [a b] (mapv + a b))
(defn- v- [a b] (mapv - a b))
(defn- v* [s v] (mapv #(* s %) v))
(defn- length [v] (#?(:clj Math/sqrt :cljs js/Math.sqrt) (reduce + (map #(* % %) v))))

(defn- rotate-y [[x y z] angle]
  (let [c (#?(:clj Math/cos :cljs js/Math.cos) angle)
        s (#?(:clj Math/sin :cljs js/Math.sin) angle)]
    [(+ (* c x) (* s z)) y (+ (* (- s) x) (* c z))]))

(defn- aabb-gap [{ca :center ha :half} {cb :center hb :half}]
  (apply max (map (fn [a b ah bh] (- (#?(:clj Math/abs :cljs js/Math.abs) (- a b)) ah bh))
                  ca cb ha hb)))

(defn- point-aabb-distance [point {:keys [center half]}]
  (length (mapv (fn [p c h] (max 0.0 (- (#?(:clj Math/abs :cljs js/Math.abs) (- p c)) h)))
                point center half)))

(defn- capsule-aabb-gap [{:keys [a b radius]} box]
  (apply min
         (for [i (range 25)
               :let [t (/ i 24.0)
                     p (v+ a (v* t (v- b a)))]]
           (- (point-aabb-distance p box) radius))))

(defn- weapon-pose [entity-id]
  (let [seed (equipment/stable-seed entity-id)
        jitter-x (* 0.004 (- (mod seed 5) 2))
        jitter-z (* -0.004 (mod (quot seed 5) 4))
        pi #?(:clj Math/PI :cljs js/Math.PI)
        yaw (* -0.75 pi)
        position [(+ 0.25 jitter-x) 1.28 (+ -0.57 jitter-z)]
        world (fn [local] (v+ position (rotate-y local yaw)))
        primary (world (get-in weapon/sockets [:primary-grip :position]))
        support (world (get-in weapon/sockets [:support-grip :position]))
        direction (rotate-y [0.0 0.0 1.0] yaw)
        rear (v+ position (v* -0.57 direction))
        muzzle (v+ position (v* 1.39 direction))]
    {:transform {:position position
                 :rotation [0.0 (#?(:clj Math/sin :cljs js/Math.sin) (* yaw 0.5))
                            0.0 (#?(:clj Math/cos :cljs js/Math.cos) (* yaw 0.5))]
                 :scale [1.0 1.0 1.0]}
     :attachment {:semantic-id :humanoid/right-hand :socket :weapon/grip-primary}
     :sockets {:primary-grip {:semantic-id :weapon/grip-primary :world-position primary}
               :support-grip {:semantic-id :weapon/grip-support :world-position support}}
     :volume {:kind :capsule :a rear :b muzzle :radius 0.14}
     :silhouette-area 0.235
     :pose-semantic :combat/two-hand-aim}))

(defn validate-fit
  "Recompute clearance, grip readability and silhouette limits for fit data."
  [{:keys [tier weapon equipment] :as fit}]
  (let [budget (get tier-budgets tier)
        mounted-gaps
        (for [[part-id {:keys [volume mounted-on]}] equipment
              [body-id body-volume] body-volumes
              :when (not= mounted-on body-id)]
          {:kind :equipment-body :part part-id :body body-id
           :gap (aabb-gap volume body-volume)})
        critical-pairs
        (for [[shoulder-id backpack-id] [[:equipment/shoulder-left :equipment/backpack]
                                         [:equipment/shoulder-right :equipment/backpack]]
              :let [a (get-in equipment [shoulder-id :volume])
                    b (get-in equipment [backpack-id :volume])]
              :when (and a b)]
          {:kind :equipment-equipment :part shoulder-id :other backpack-id
           :gap (aabb-gap a b)})
        ;; The rifle is intentionally contacted by solved arms/hands. Clearance
        ;; gates the core silhouette volumes; arm contact is governed by IK.
        weapon-gaps (for [[body-id body-volume] body-volumes
                          :when (contains? #{:head :torso :pelvis} body-id)]
                      {:kind :weapon-body :body body-id
                       :gap (capsule-aabb-gap (:volume weapon) body-volume)})
        constraints (vec (concat mounted-gaps critical-pairs weapon-gaps))
        minimum-gap (if (seq constraints) (apply min (map :gap constraints)) 999.0)
        intersections (vec (filter #(< (:gap %) (:minimum-clearance budget)) constraints))
        primary (get-in weapon [:sockets :primary-grip :world-position])
        support (get-in weapon [:sockets :support-grip :world-position])
        grip-separation (length (v- primary support))
        tier-scale ({:hero 1.0 :gameplay 0.75 :crowd 0.45} tier)
        area (* tier-scale (+ (:silhouette-area weapon)
                              (reduce + (map :silhouette-area (vals equipment)))))
        occupancy (/ area 1.755)
        chain-errors (for [[side chain] (:arm-chains fit)
                           :when (or (not (:valid? chain))
                                     (> (get-in chain [:metrics :target-error]) 0.025))]
                       side)
        errors (cond-> []
                 (seq intersections) (conj :clearance)
                 (< grip-separation (:minimum-grip-separation budget)) (conj :grip-readability)
                 (> occupancy (:maximum-silhouette-occupancy budget)) (conj :silhouette-occupancy)
                 (seq chain-errors) (conj :arm-chain-continuity))]
    {:valid? (empty? errors) :errors errors
     :minimum-clearance minimum-gap :intersections intersections
     :grip-separation grip-separation
     :silhouette-occupancy occupancy
     :invalid-arm-chains (vec chain-errors)
     :budgets budget
     :constraint-count (count constraints)}))

(defn resolve-fit
  "Resolve adjusted, validated equipment and two-hand weapon attachment data."
  [{:keys [family tier entity-id character-preset team-palette]
    :or {family :stylized tier :gameplay entity-id :operator/default
         character-preset :combat-readable team-palette {}}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Operator fit visual family is not implemented"
                    {:family family :implemented (:implemented-families family-boundary)})))
  (when-not (get tier-budgets tier)
    (throw (ex-info "Unknown operator fit tier" {:tier tier :known-tiers (set (keys tier-budgets))})))
  (let [kit (equipment/resolve-kit {:family family :tier tier :entity-id entity-id
                                    :character-preset character-preset :team-palette team-palette})
        enabled (into {}
                      (for [[part-id layout] equipment-layout
                            :let [source (equipment/part kit part-id)]
                            :when (:enabled? source)]
                        [part-id (assoc layout :source-mesh (:mesh source)
                                              :attachment-semantic (get-in source [:attachment :semantic-id]))]))
        weapon (weapon-pose entity-id)
        primary (get-in weapon [:sockets :primary-grip :world-position])
        support (get-in weapon [:sockets :support-grip :world-position])
        arm-inputs {:right {:family family :side :right
                            :shoulder [0.36 1.46 0.0] :elbow [0.40 1.10 0.0] :hand [0.40 0.76 0.0]
                            :target primary :pole [1.0 0.15 0.0]
                            :upper-length 0.58 :lower-length 0.54}
                    :left {:family family :side :left
                           :shoulder [-0.36 1.46 0.0] :elbow [-0.40 1.10 0.0] :hand [-0.40 0.76 0.0]
                           :target support :pole [-1.0 0.15 0.0]
                           :upper-length 0.58 :lower-length 0.54}}
        arm-chains (into {} (map (fn [[side input]] [side (arm-ik/solve input)]) arm-inputs))
        fit {:contract contract :family family :tier tier :entity-id entity-id
             :pose {:semantic :combat/two-hand-aim
                    :joint-targets {:hand-right {:target primary :socket :weapon/grip-primary}
                                    :hand-left {:target support :socket :weapon/grip-support}}
                    :constraints {:look-at :weapon/sight-line :elbows :outward-readable}}
             :arm-chains arm-chains
             :weapon weapon :equipment enabled
             :body {:contract body/contract :volumes body-volumes}
             :source-contracts {:equipment equipment/contract :weapon weapon/contract
                                :body body/contract}}
        validation (validate-fit fit)]
    (when-not (:valid? validation)
      (throw (ex-info "Resolved operator fit violates geometric budgets" validation)))
    (assoc fit :validation validation)))
