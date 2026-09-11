(ns kami.render.character-camera
  "Deterministic production character framing that preserves environment context.")

(def contract :kotoba.render/production-character-camera-v1)
(def family-boundary {:families #{:stylized :photoreal}
                      :implemented-families #{:stylized} :same-api? true})

(def coverage-range [0.28 0.42])
(def ^:private default-policy
  {:target-coverage 0.35 :vertical-fov-deg 42.0 :near 0.05 :far 600.0
   :head-safe-margin 0.12 :feet-safe-margin 0.12
   :minimum-ground-clearance 0.35 :collision-padding 0.12
   :settle-stable-frames 3 :maximum-attempts 15})

(defn- radians [degrees] (* degrees (/ #?(:clj Math/PI :cljs js/Math.PI) 180.0)))
(defn- v+ [a b] (mapv + a b))
(defn- v- [a b] (mapv - a b))
(defn- v* [s v] (mapv #(* s %) v))
(defn- length [v] (#?(:clj Math/sqrt :cljs js/Math.sqrt) (reduce + (map #(* % %) v))))

(defn- inside-aabb? [[x y z] {:keys [min max]} padding]
  (and (<= (- (nth min 0) padding) x (+ (nth max 0) padding))
       (<= (- (nth min 1) padding) y (+ (nth max 1) padding))
       (<= (- (nth min 2) padding) z (+ (nth max 2) padding))))

(defn- segment-occluded? [a b obstacle padding]
  (boolean (some #(inside-aabb? (v+ a (v* (/ % 24.0) (v- b a))) obstacle padding)
                 (range 1 24))))

(defn- distance-for-coverage [subject-height coverage fov-deg]
  (/ (* 0.5 subject-height)
     (#?(:clj Math/tan :cljs js/Math.tan) (* 0.5 coverage (radians fov-deg)))))

(defn- candidate [center distance yaw-deg ground-y policy]
  (let [yaw (radians yaw-deg)
        eye-lift (* distance 0.045)
        position [(+ (nth center 0) (* distance (#?(:clj Math/sin :cljs js/Math.sin) yaw)))
                  (max (+ ground-y (:minimum-ground-clearance policy))
                       (+ (nth center 1) eye-lift))
                  (- (nth center 2) (* distance (#?(:clj Math/cos :cljs js/Math.cos) yaw)))]]
    {:position position :look-at center :yaw-deg yaw-deg :distance (length (v- position center))}))

(defn resolve-camera
  "Resolve renderer/studio camera data or fail closed when collision/occlusion has no solution.

  Subject bounds are world AABB `{:min [x y z] :max [x y z]}`. Obstacles use the
  same representation. Only the skinned selection is narrowed; world visibility
  is always preserved."
  [{:keys [family subject-id subject-bounds orbit obstacles ground-y viewport policy]
    :or {family :stylized subject-id :operator/default orbit :three-quarter-right
         obstacles [] ground-y 0.0 viewport {:width 1920 :height 1080} policy {}}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Character camera family is not implemented" {:family family})))
  (when-not (and subject-bounds (= 3 (count (:min subject-bounds))) (= 3 (count (:max subject-bounds))))
    (throw (ex-info "Character camera requires subject world bounds" {:subject-bounds subject-bounds})))
  (when-not (and (pos? (:width viewport 0)) (pos? (:height viewport 0)))
    (throw (ex-info "Character camera requires a positive viewport" {:viewport viewport})))
  (let [policy (merge default-policy policy)
        mn (:min subject-bounds) mx (:max subject-bounds)
        center (mapv #(* 0.5 (+ %1 %2)) mn mx)
        height (- (nth mx 1) (nth mn 1))
        target (:target-coverage policy)
        base-distance (distance-for-coverage height target (:vertical-fov-deg policy))
        requested ({:front 0.0 :three-quarter-right 35.0 :three-quarter-left -35.0} orbit)
        _ (when-not requested (throw (ex-info "Unknown character camera orbit" {:orbit orbit})))
        yaws (distinct [requested 0.0 35.0 -35.0 60.0 -60.0])
        attempts (for [distance-scale [1.0 1.12 1.25]
                       yaw yaws
                       :let [c (candidate center (* base-distance distance-scale) yaw ground-y policy)
                             collision? (some #(inside-aabb? (:position c) % (:collision-padding policy)) obstacles)
                             occlusion? (some #(segment-occluded? (:position c) center %
                                                                  (:collision-padding policy)) obstacles)]
                       :while (< 0 (:maximum-attempts policy))]
                   (assoc c :collision? (boolean collision?) :occlusion? (boolean occlusion?)))
        indexed (map-indexed vector (take (:maximum-attempts policy) attempts))
        [attempt-index selected] (first (filter (fn [[_ c]] (not (or (:collision? c) (:occlusion? c)))) indexed))]
    (when-not selected
      (throw (ex-info "Character camera failed closed: no collision-free visible framing"
                      {:contract contract :attempts (vec (map second indexed))
                       :subject-id subject-id :valid? false})))
    (let [coverage (/ (* 2.0 (#?(:clj Math/atan :cljs js/Math.atan)
                                (/ (* 0.5 height) (:distance selected))))
                      (radians (:vertical-fov-deg policy)))
          vertical-margin (* 0.5 (- 1.0 coverage))
          ground-clearance (- (second (:position selected)) ground-y)
          ground-visible? (>= vertical-margin (:feet-safe-margin policy))
          horizon-visible? (> (second (:position selected)) (second (:look-at selected)))
          evidence {:valid? (and (<= (first coverage-range) coverage (second coverage-range))
                                 (>= vertical-margin (:head-safe-margin policy))
                                 (>= vertical-margin (:feet-safe-margin policy))
                                 (>= ground-clearance (:minimum-ground-clearance policy))
                                 ground-visible? horizon-visible?)
                    :coverage coverage :coverage-range coverage-range
                    :head-safe-margin vertical-margin :feet-safe-margin vertical-margin
                    :ground-clearance ground-clearance :ground-visible? ground-visible?
                    :horizon-visible? horizon-visible?
                    :collision-free? true :subject-occlusion-free? true
                    :environment-context-retained? true}]
      (when-not (:valid? evidence)
        (throw (ex-info "Character camera framing evidence failed closed" evidence)))
      {:contract contract :family family :subject-id subject-id
       :camera {:position (:position selected) :look-at (:look-at selected) :up [0.0 1.0 0.0]
                :vertical-fov-deg (:vertical-fov-deg policy) :near (:near policy) :far (:far policy)
                :viewport viewport}
       :orbit {:requested orbit :resolved-yaw-deg (:yaw-deg selected)}
       :render-selection {:skinned {:mode :subject-only :entity-ids #{subject-id}}
                          :world {:mode :preserve-all :removed? false}}
       :settle {:deterministic? true :attempt-index attempt-index
                :stable-frames (:settle-stable-frames policy) :position-delta 0.0
                :key [subject-id orbit subject-bounds viewport]}
       :evidence evidence})))
