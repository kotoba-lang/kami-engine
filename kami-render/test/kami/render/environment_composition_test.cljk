(ns kami.render.environment-composition-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.character-camera :as camera]
            [kami.render.environment-composition :as composition]
            [kotoba.render.foreground-density :as foreground-density]))

(def subject {:min [-0.62 0.0 -0.45] :max [0.62 2.08 0.45]})
(def resolved (camera/resolve-camera {:subject-id :operator/hero :subject-bounds subject
                                     :orbit :front}))

(deftest projects-all-aabb-corners-to-safe-screen-bounds
  (let [bounds {:min [2.1 0.0 -0.2] :max [2.6 1.2 0.3]}
        projected (composition/project-aabb (:camera resolved) bounds)]
    (is (= 8 (count (:corners projected))))
    (is (every? #(<= 0.0 % 1.0) (concat (:min projected) (:max projected))))
    (is (> (* 0.5 (+ (get-in projected [:min 0]) (get-in projected [:max 0]))) 0.5))
    (is (< (first (:depth-range projected)) (second (:depth-range projected))))))

(deftest selection-is-deterministic-grounded-and-outside-subject-padding
  (let [candidates [{:id :prop/right :priority 4
                     :bounds {:min [2.1 0.0 -0.2] :max [2.6 1.2 0.3]}}
                    {:id :prop/left :priority 7
                     :bounds {:min [-2.5 0.0 0.0] :max [-2.0 0.8 0.4]}}
                    {:id :prop/behind-subject :priority 99
                     :bounds {:min [-0.2 0.0 0.1] :max [0.2 0.8 0.35]}}]
        input {:resolved-camera resolved :subject-bounds subject :candidates candidates}
        a (composition/compose input) b (composition/compose input)]
    (is (= a b))
    (is (= [:prop/left :prop/right] (mapv :id (:placements a))))
    (is (= [:prop/behind-subject] (mapv :id (:rejected a))))
    (is (= [:subject-exclusion] (get-in a [:rejected 0 :reasons])))
    (is (every? #(<= (get-in % [:ground-contact :error]) 0.025) (:placements a)))
    (is (:world-context-retained? (:evidence a)))))

(deftest selection-never-removes-world-or-changes-subject-selection
  (let [result (composition/compose
                {:resolved-camera resolved :subject-bounds subject
                 :candidates [{:id :prop/right
                               :bounds {:min [2.1 0.0 -0.2] :max [2.6 1.2 0.3]}}]})]
    (is (= :preserve-all (get-in result [:render-selection :world :mode])))
    (is (false? (get-in result [:render-selection :world :removed?])))
    (is (= #{:operator/hero} (get-in result [:render-selection :skinned :entity-ids])))))

(deftest invalid-compositions-fail-closed-with-evidence
  (let [unsafe [{:id :prop/mask :bounds {:min [-0.3 0.0 0.0] :max [0.3 1.5 0.3]}}]]
    (is (thrown-with-msg? #?(:clj Exception :cljs js/Error) #"failed closed"
                          (composition/compose {:resolved-camera resolved
                                                :subject-bounds subject
                                                :candidates unsafe}))))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (composition/compose {:family :photoreal :resolved-camera resolved
                                     :subject-bounds subject :candidates []}))))

(deftest ground-contact-must-be-in-visible-lower-frame-band
  (let [sky-camera (assoc-in resolved [:camera :look-at] [0.0 -2.0 0.0])
        floating {:id :prop/floating-looking
                  :bounds {:min [2.1 0.0 -0.2] :max [2.6 0.10 0.3]}}
        failure (try
                  (composition/compose {:resolved-camera sky-camera
                                        :subject-bounds subject :candidates [floating]})
                  nil
                  (catch #?(:clj Exception :cljs js/Error) error error))
        data (ex-data failure)
        contact-y (get-in data [:evaluated 0 :ground-contact :projection :screen 1])]
    ;; The AABB is globally on-screen, but its base projects into sky/horizon.
    (is (<= 0.19 contact-y 0.21))
    (is (= [:ground-contact-outside-visible-ground-band]
           (get-in data [:evaluated 0 :reasons])))
    (is (= [0.38 0.92] (get-in data [:evidence :ground-contact-screen-y-range]))))
  (let [valid (composition/compose
               {:resolved-camera resolved :subject-bounds subject
                :candidates [{:id :prop/lower-frame
                              :bounds {:min [2.1 0.0 -0.2] :max [2.6 1.2 0.3]}}]})
        y (get-in valid [:placements 0 :ground-contact :projection :screen 1])]
    (is (<= 0.38 y 0.92))
    (is (= y (get-in valid [:evidence :selected-ground-contact-screen-y
                            :prop/lower-frame])))))

(deftest required-regions-reserve-slots-before-priority-fill
  (let [left (fn [id priority x]
               {:id id :priority priority :composition-region :foreground-left
                :bounds {:min [x 0.0 -0.2] :max [(+ x 0.35) 0.7 0.25]}})
        candidates [(left :prop/left-a 100 -2.8)
                    (left :prop/left-b 90 -2.4)
                    (left :prop/left-c 80 -2.0)
                    {:id :prop/right-only :priority 1
                     :composition-region :foreground-right
                     :bounds {:min [2.1 0.0 -0.2] :max [2.5 0.7 0.25]}}]
        result (composition/compose
                {:resolved-camera resolved :subject-bounds subject :candidates candidates
                 :policy {:maximum-selected 2
                          :required-composition-regions
                          #{:foreground-left :foreground-right}}})]
    (is (= #{:prop/left-a :prop/right-only} (set (map :id (:placements result)))))
    (is (= {:foreground-left 1 :foreground-right 1}
           (get-in result [:evidence :selected-region-counts])))
    (is (empty? (get-in result [:evidence :missing-composition-regions])))))

(deftest missing-required-composition-region-fails-closed
  (let [failure (try
                  (composition/compose
                   {:resolved-camera resolved :subject-bounds subject
                    :candidates [{:id :prop/left :composition-region :foreground-left
                                  :bounds {:min [-2.5 0.0 -0.2] :max [-2.1 0.7 0.25]}}]
                    :policy {:required-composition-regions
                             #{:foreground-left :foreground-right}}})
                  nil
                  (catch #?(:clj Exception :cljs js/Error) error error))]
    (is (= [:foreground-right]
           (get-in (ex-data failure) [:evidence :missing-composition-regions])))))

(deftest evidence-separates-unsafe-rejections-from-safe-capacity-truncation
  (let [candidates [{:id :prop/a :priority 3
                     :bounds {:min [2.0 0.0 -0.2] :max [2.25 0.6 0.2]}}
                    {:id :prop/b :priority 2
                     :bounds {:min [2.4 0.0 -0.2] :max [2.65 0.6 0.2]}}
                    {:id :prop/c :priority 1
                     :bounds {:min [-2.5 0.0 -0.2] :max [-2.2 0.6 0.2]}}
                    {:id :prop/unsafe-mask :priority 99
                     :bounds {:min [-0.2 0.0 0.1] :max [0.2 0.8 0.35]}}]
        result (composition/compose
                {:resolved-camera resolved :subject-bounds subject :candidates candidates
                 :policy {:maximum-selected 1}})
        evidence (:evidence result)]
    (is (= 1 (:selected-count evidence) (count (:placements result))))
    (is (= 1 (:rejected-count evidence) (count (:rejected result))))
    (is (= 2 (:unselected-safe-count evidence) (count (:unselected-safe result))))
    (is (= (set (:unselected-safe result)) (set (:unselected-safe evidence))))
    (is (= (:candidate-count evidence)
           (+ (:selected-count evidence) (:rejected-count evidence)
              (:unselected-safe-count evidence))))))

(deftest semantic-screen-side-must-match-actual-projection
  (let [right-bounds {:min [2.1 0.0 -0.2] :max [2.6 0.7 0.25]}
        failure (try
                  (composition/compose
                   {:resolved-camera resolved :subject-bounds subject
                    :candidates [{:id :prop/spoofed-left :screen-side :left
                                  :composition-region :foreground-left
                                  :bounds right-bounds}]
                    :policy {:required-composition-regions #{:foreground-left}}})
                  nil
                  (catch #?(:clj Exception :cljs js/Error) error error))]
    (is (= [:screen-side-mismatch]
           (get-in (ex-data failure) [:evaluated 0 :reasons])))
    (is (= [:foreground-left]
           (get-in (ex-data failure) [:evidence :missing-composition-regions]))))
  (let [valid (composition/compose
               {:resolved-camera resolved :subject-bounds subject
                :candidates [{:id :prop/right :screen-side :right
                              :composition-region :foreground-right
                              :bounds {:min [2.1 0.0 -0.2] :max [2.6 0.7 0.25]}}]
                :policy {:required-composition-regions #{:foreground-right}}})]
    (is (= [:prop/right] (mapv :id (:placements valid))))))

(deftest candidate-specific-ground-bands-support-building-and-foreground-depths
  (let [candidates [{:id :building/background :composition-region :building
                     :ground-contact-screen-y-range [0.44 0.52]
                     :screen-extent-range [0.02 0.35]
                     :bounds {:min [7.0 0.0 29.0] :max [9.0 5.0 31.0]}}
                    {:id :prop/foreground :composition-region :foreground-right
                     :ground-contact-screen-y-range [0.58 0.92]
                     :screen-extent-range [0.02 0.35]
                     :bounds {:min [3.5 0.0 2.7] :max [4.2 1.0 3.3]}}]
        result (composition/compose
                {:resolved-camera resolved :subject-bounds subject :candidates candidates
                 :policy {:required-composition-region-counts
                          {:building 1 :foreground-right 1}}})
        ys (get-in result [:evidence :selected-ground-contact-screen-y])]
    (is (<= 0.44 (get ys :building/background) 0.52))
    (is (<= 0.58 (get ys :prop/foreground) 0.92))
    (is (= [0.44 0.52] (get-in result [:evidence
                                       :selected-ground-contact-screen-y-ranges
                                       :building/background])))
    (is (= [0.58 0.92] (get-in result [:evidence
                                       :selected-ground-contact-screen-y-ranges
                                       :prop/foreground])))))

(deftest required-region-counts-reserve-three-per-side-before-fill
  (let [candidate (fn [side i priority]
                    (let [left? (= side :left)
                          x (+ (if left? -3.3 2.0) (* i 0.38))]
                      {:id (keyword (str "prop." (name side)) (str i))
                       :priority priority
                       :composition-region (if left? :foreground-left :foreground-right)
                       :screen-side side
                       :bounds {:min [x 0.0 0.0] :max [(+ x 0.25) 0.55 0.25]}}))
        lefts (mapv #(candidate :left % (- 100 %)) (range 5))
        rights (mapv #(candidate :right % (- 10 %)) (range 3))
        result (composition/compose
                {:resolved-camera resolved :subject-bounds subject
                 :candidates (into lefts rights)
                 :policy {:maximum-selected 6
                          :required-composition-region-counts
                          {:foreground-left 3 :foreground-right 3}}})]
    (is (= {:foreground-left 3 :foreground-right 3}
           (get-in result [:evidence :selected-region-counts])))
    (is (= {:foreground-left 3 :foreground-right 3}
           (get-in result [:evidence :required-composition-region-counts])))
    (is (empty? (get-in result [:evidence :composition-region-shortages])))
    (is (= 6 (count (:placements result))))))

(deftest region-quota-shortage-fails-closed-with-exact-count
  (let [failure (try
                  (composition/compose
                   {:resolved-camera resolved :subject-bounds subject
                    :candidates [{:id :prop/only-right
                                  :composition-region :foreground-right
                                  :bounds {:min [2.1 0.0 0.0] :max [2.4 0.5 0.2]}}]
                    :policy {:required-composition-region-counts
                             {:foreground-right 3}}})
                  nil
                  (catch #?(:clj Exception :cljs js/Error) error error))]
    (is (= {:foreground-right 2}
           (get-in (ex-data failure) [:evidence :composition-region-shortages])))))

(deftest projected-screen-extent-rejects-tiny-or-oversized-candidates
  (let [tiny {:id :prop/tiny :screen-extent-range [0.05 0.30]
              :bounds {:min [7.0 0.0 29.0] :max [7.01 0.01 29.01]}}
        oversized {:id :prop/oversized :screen-extent-range [0.001 0.05]
                   :bounds {:min [1.5 0.0 0.0] :max [3.0 1.0 0.3]}}
        failure (try
                  (composition/compose {:resolved-camera resolved :subject-bounds subject
                                        :candidates [tiny oversized]})
                  nil
                  (catch #?(:clj Exception :cljs js/Error) error error))
        evaluated (:evaluated (ex-data failure))]
    (is (every? #(some #{:screen-extent-outside-range} (:reasons %)) evaluated))
    (is (< (:screen-extent (first (filter #(= :prop/tiny (:id %)) evaluated))) 0.05))
    (is (> (:screen-extent (first (filter #(= :prop/oversized (:id %)) evaluated))) 0.05))))

(deftest render-grass-descriptor-contract-accepts-eleven-percent-extent
  (let [production-subject {:min [-0.78 0.0 -0.62] :max [0.78 1.75 0.62]}
        production-camera (camera/resolve-camera
                           {:family :stylized :subject-id 1 :orbit :three-quarter-left
                            :subject-bounds production-subject :ground-y 0.0
                            :viewport {:width 1280 :height 720}
                            :policy {:target-coverage 0.35 :vertical-fov-deg 42.0
                                     :head-safe-margin 0.12 :feet-safe-margin 0.12}})
        {[fx _ fz] :direction} (composition/camera-ground-facing production-camera)
        kit (foreground-density/foreground-kit
             {:family :stylized :tier :hero :entity-id :royale-camera-foreground
              :seed 26071812 :origin [8.0 0.0 0.0] :radius 6.0 :ground-y 0.0
              :camera-facing-direction [fx fz]})
        descriptor (first (filter #(= :foreground-grass-17 (:descriptor/id %))
                                  (get-in kit [:camera-zones :foreground])))
        [x y z] (get-in descriptor [:transform :offset])
        [width height depth] (get-in descriptor [:transform :scale])
        grass (-> (select-keys descriptor [:composition-region :screen-side :cluster-id
                                           :cluster-role :ground-contact-screen-y-range
                                           :screen-extent-range])
                  (assoc :id (:descriptor/id descriptor)
                         ;; Render normalized-unit + world-size final AABB semantics.
                         :bounds {:min [(- x (* 0.5 width)) y (- z (* 0.5 depth))]
                                  :max [(+ x (* 0.5 width)) (+ y height)
                                        (+ z (* 0.5 depth))]}))
        result (composition/compose
                {:resolved-camera production-camera :subject-bounds production-subject
                 :candidates [grass]
                 :policy {:required-composition-region-counts
                          {(:composition-region grass) 1}}})
        placement (first (:placements result))
        extent (:screen-extent placement)
        contact-y (get-in placement [:ground-contact :projection :screen 1])]
    ;; Exact kotoba-lang/render bb9ddc4 Royale origin-5 descriptor. Both values
    ;; are calculated from the final world AABB, not copied from descriptor intent.
    (is (< (Math/abs (double (+ fx 0.573576436351046))) 1.0e-9))
    (is (< (Math/abs (double (+ fz 0.8191520442889919))) 1.0e-9))
    (is (<= 0.025 extent 0.11))
    (is (<= 0.58 contact-y 0.90))
    (is (= (:cluster-id descriptor) (get-in placement [:candidate :cluster-id])))
    (is (= :vegetation (get-in placement [:candidate :cluster-role])))
    (is (= {(:composition-region grass) 1}
           (get-in result [:evidence :selected-region-counts])))))

(deftest camera-ground-facing-comes-from-resolved-orbit-not-hardcoded-z
  (let [front (composition/camera-ground-facing resolved)
        rotated-camera (camera/resolve-camera {:subject-bounds subject
                                               :orbit :three-quarter-right})
        rotated (composition/camera-ground-facing rotated-camera)
        [_ qy _ qw] (:rotation rotated)
        rotated-positive-z [(* 2.0 qy qw) 0.0 (- 1.0 (* 2.0 qy qy))]]
    (is (< (Math/abs (double (first (:direction front)))) 1.0e-9))
    (is (< (Math/abs (double (+ 1.0 (nth (:direction front) 2)))) 1.0e-9))
    (is (> (first (:direction rotated)) 0.50))
    (is (< (nth (:direction rotated) 2) -0.75))
    (is (not= (:rotation front) (:rotation rotated)))
    (is (every? #(< (Math/abs (double %)) 1.0e-9)
                (map - rotated-positive-z (:direction rotated))))
    (is (= rotated (get-in (composition/compose
                            {:resolved-camera rotated-camera :subject-bounds subject
                             :candidates [{:id :prop/right
                                           :bounds {:min [2.1 0.0 -0.2]
                                                    :max [2.6 0.6 0.3]}}]})
                           [:camera-ground-facing])))))

(deftest region-quota-reserves-required-cluster-roles-before-priority-fill
  (let [candidate (fn [id side role priority x]
                    {:id id :priority priority :screen-side side :cluster-role role
                     :composition-region (keyword (str "foreground-" (name side)))
                     :bounds {:min [x 0.0 0.0] :max [(+ x 0.24) 0.5 0.2]}})
        candidates [(candidate :left/solid-a :left :solid-prop 100 -2.8)
                    (candidate :left/solid-b :left :solid-prop 90 -2.4)
                    (candidate :left/solid-c :left :solid-prop 80 -2.0)
                    (candidate :left/vegetation :left :vegetation 1 -3.2)
                    (candidate :right/solid :right :solid-prop 50 2.0)
                    (candidate :right/vegetation :right :vegetation 2 2.4)]
        required {:foreground-left #{:vegetation :solid-prop}
                  :foreground-right #{:vegetation :solid-prop}}
        result (composition/compose
                {:resolved-camera resolved :subject-bounds subject :candidates candidates
                 :policy {:maximum-selected 6
                          :required-composition-region-counts
                          {:foreground-left 3 :foreground-right 2}
                          :required-cluster-roles-by-composition-region required}})]
    (is (= required (get-in result [:evidence
                                    :selected-cluster-roles-by-composition-region])))
    (is (empty? (get-in result [:evidence
                                :missing-cluster-roles-by-composition-region])))
    (is (contains? (set (map :id (:placements result))) :left/vegetation))))

(deftest required-cluster-role-shortage-fails-closed-with-exact-evidence
  (let [failure (try
                  (composition/compose
                   {:resolved-camera resolved :subject-bounds subject
                    :candidates [{:id :left/solid :screen-side :left
                                  :cluster-role :solid-prop
                                  :composition-region :foreground-left
                                  :bounds {:min [-2.5 0.0 0.0] :max [-2.2 0.5 0.2]}}]
                    :policy {:required-composition-region-counts {:foreground-left 2}
                             :required-cluster-roles-by-composition-region
                             {:foreground-left #{:vegetation :solid-prop}}}})
                  nil
                  (catch #?(:clj Exception :cljs js/Error) error error))]
    (is (= {:foreground-left #{:vegetation}}
           (get-in (ex-data failure)
                   [:evidence :missing-cluster-roles-by-composition-region])))
    (is (= {:foreground-left 1}
           (get-in (ex-data failure) [:evidence :composition-region-shortages])))))

(defn- measurable-candidate [side index]
  (let [x (* (if (= side :left) -1.0 1.0) (+ 1.8 (* index 0.35)))]
    {:id (keyword (name side) (str index))
     :composition-region (keyword (str "foreground-" (name side)))
     :screen-side side :cluster-role (if (even? index) :vegetation :solid-prop)
     :kind (if (even? index) :grass :rock)
     :geometry-variant (keyword (str "variant-" index))
     :ground-contact-screen-y-range [0.68 0.86]
     :bounds {:min [(- x 0.3) 0.0 -2.5] :max [(+ x 0.3) 0.8 -1.9]}}))

(def measurable-policy
  {:maximum-selected 6
   :required-composition-region-counts {:foreground-left 3 :foreground-right 3}
   :required-screen-occupancy-cells-by-composition-region
   {:foreground-left #{:lower-left} :foreground-right #{:lower-right}}
   :minimum-projected-union-area 0.040
   :minimum-projected-union-area-by-composition-region
   {:foreground-left 0.018 :foreground-right 0.018}
   :required-diversity-by-composition-region
   {:foreground-left {:kind 2 :geometry-variant 2}
    :foreground-right {:kind 2 :geometry-variant 2}}})

(deftest measurable-occupancy-area-and-diversity-gates-use-actual-projection
  (let [candidates (vec (concat (map #(measurable-candidate :left %) (range 3))
                                (map #(measurable-candidate :right %) (range 3))))
        result (composition/compose {:resolved-camera resolved :subject-bounds subject
                                     :candidates candidates :policy measurable-policy})
        evidence (:evidence result)]
    (is (= {:foreground-left #{:lower-left} :foreground-right #{:lower-right}}
           (:selected-screen-occupancy-cells-by-composition-region evidence)))
    (is (>= (:projected-union-area evidence) 0.040))
    (is (every? #(>= % 0.018)
                (vals (:projected-union-area-by-composition-region evidence))))
    (is (= 2 (count (get-in evidence [:selected-diversity-by-composition-region
                                      :foreground-left :kind]))))
    (is (empty? (:diversity-shortages-by-composition-region evidence)))))

(deftest coverage-objective-swaps-overlapping-mandatory-picks-without-losing-semantics
  (let [base (measurable-candidate :left 0)
        early [(assoc base :id :early/grass :priority 30 :cluster-role :vegetation
                      :kind :grass :geometry-variant :grass)
               (assoc base :id :early/rock :priority 29 :cluster-role :solid-prop
                      :kind :rock :geometry-variant :rock)
               (assoc base :id :early/crate :priority 28 :cluster-role :solid-prop
                      :kind :crate :geometry-variant :crate)]
        later [(assoc (measurable-candidate :left 1) :id :later/rock :priority 2
                      :cluster-role :solid-prop :kind :rock :geometry-variant :rock)
               (assoc (measurable-candidate :left 2) :id :later/grass :priority 1
                      :cluster-role :vegetation :kind :grass :geometry-variant :grass)]
        policy {:maximum-selected 3
                :required-composition-region-counts {:foreground-left 3}
                :required-screen-occupancy-cells-by-composition-region
                {:foreground-left #{:lower-left}}
                :required-cluster-roles-by-composition-region
                {:foreground-left #{:vegetation :solid-prop}}
                :required-diversity-by-composition-region
                {:foreground-left {:kind 3 :geometry-variant 3}}
                :minimum-projected-union-area-by-composition-region
                {:foreground-left 0.025}}
        result (composition/compose {:resolved-camera resolved :subject-bounds subject
                                     :candidates (vec (concat early later)) :policy policy})
        evidence (:evidence result)]
    (is (some #{:later/rock :later/grass} (map :id (:placements result))))
    (is (>= (get-in evidence [:projected-union-area-by-composition-region
                              :foreground-left]) 0.025))
    (is (empty? (:missing-cluster-roles-by-composition-region evidence)))
    (is (empty? (:diversity-shortages-by-composition-region evidence)))
    (is (pos? (get-in evidence [:coverage-selection-objective :regions
                                :foreground-left :swap-count])))))

(deftest impossible-coverage-fails-closed-with-best-achieved-evidence
  (let [base (measurable-candidate :left 0)
        candidates (mapv (fn [i]
                           (assoc base :id (keyword "overlap" (str i))
                                  :priority (- 10 i)))
                         (range 4))
        failure (try
                  (composition/compose
                   {:resolved-camera resolved :subject-bounds subject :candidates candidates
                    :policy {:maximum-selected 3
                             :required-composition-region-counts {:foreground-left 3}
                             :minimum-projected-union-area-by-composition-region
                             {:foreground-left 0.050}}})
                  nil (catch #?(:clj Exception :cljs js/Error) error error))
        evidence (:evidence (ex-data failure))]
    (is (pos? (get-in evidence [:projected-union-area-shortages-by-composition-region
                                :foreground-left])))
    (is (= (get-in evidence [:projected-union-area-by-composition-region :foreground-left])
           (get-in evidence [:coverage-selection-objective
                             :best-achieved-union-area-by-composition-region
                             :foreground-left])))
    (is (true? (get-in evidence [:coverage-selection-objective :regions
                                 :foreground-left :exhausted?])))))

(deftest occupancy-and-diversity-shortages-fail-closed-exactly
  (let [only-left [(assoc (measurable-candidate :left 0) :geometry-variant :same)
                   (assoc (measurable-candidate :left 2) :geometry-variant :same)]
        failure (try
                  (composition/compose {:resolved-camera resolved :subject-bounds subject
                                        :candidates only-left :policy measurable-policy})
                  nil (catch #?(:clj Exception :cljs js/Error) error error))
        evidence (:evidence (ex-data failure))]
    (is (= {:foreground-right #{:lower-right}}
           (:missing-screen-occupancy-cells-by-composition-region evidence)))
    (is (pos? (get-in evidence [:diversity-shortages-by-composition-region
                                :foreground-left :kind])))
    (is (pos? (get-in evidence [:projected-union-area-shortages-by-composition-region
                                :foreground-right])))))

(deftest projected-area-evidence-remains-deterministic-under-rotated-camera
  (let [rotated (camera/resolve-camera {:subject-bounds subject :orbit :three-quarter-left})
        candidate {:id :rotated/context
                   :bounds {:min [-3.0 0.0 0.5] :max [-1.5 1.0 1.5]}}
        a (composition/compose {:resolved-camera rotated :subject-bounds subject
                                :candidates [candidate]})
        b (composition/compose {:resolved-camera rotated :subject-bounds subject
                                :candidates [candidate]})]
    (is (= a b))
    (is (pos? (get-in a [:evidence :projected-union-area])))
    (is (<= (get-in a [:evidence :projected-union-area])
            (get-in a [:evidence :aggregate-projected-area])))))

(deftest facade-readability-measures-layers-separation-and-subject-occlusion
  (let [building {:id :building/readable :composition-region :environment-building
                  :bounds {:min [2.0 0.0 4.5] :max [4.0 3.0 5.5]}
                  :facade-layer-bounds
                  [{:id :facade/base :role :facade-base
                    :bounds {:min [2.1 0.3 4.45] :max [3.9 0.8 4.55]}}
                   {:id :facade/trim :role :facade-trim
                    :bounds {:min [2.1 1.2 4.44] :max [3.9 1.5 4.54]}}
                   {:id :facade/window :role :facade-window
                    :bounds {:min [2.5 1.8 4.43] :max [3.5 2.5 4.53]}}]}
        policy {:facade-readability {:required-layer-count 3 :required-distinct-roles 3
                                     :minimum-layer-extent 0.025
                                     :minimum-layer-separation 0.012
                                     :maximum-subject-overlap 0.0}}
        result (composition/compose {:resolved-camera resolved :subject-bounds subject
                                     :candidates [building] :policy policy})
        facade (get-in result [:evidence :facade-readability])]
    (is (:valid? facade))
    (is (= 3 (:readable-layer-count facade) (:distinct-role-count facade)))
    (is (>= (:minimum-layer-separation facade) 0.012)))
  (let [bad {:id :building/unreadable :bounds {:min [2.0 0.0 4.5] :max [4.0 3.0 5.5]}
             :facade-layer-bounds [{:id :only :role :facade-base
                                    :bounds {:min [2.1 0.3 4.45] :max [2.2 0.35 4.5]}}]}
        failure (try (composition/compose
                      {:resolved-camera resolved :subject-bounds subject :candidates [bad]
                       :policy {:facade-readability {:required-layer-count 3
                                                    :required-distinct-roles 3
                                                    :minimum-layer-extent 0.025}}})
                     nil (catch #?(:clj Exception :cljs js/Error) error error))]
    (is (false? (get-in (ex-data failure) [:evidence :facade-readability :valid?])))))

(deftest hero-junction-road-layer-mask-is-lower-half-and-subject-safe
  (let [eligibility {:target :road-surface :space :neighborhood-world
                     :anchor :junction-center :subject-exclusion-required? true
                     :eligible-regions #{:junction-center}}
        road (fn [id role x0 x1]
               {:id id :composition-region :junction-center :material-role role
                :attachment-eligibility eligibility
                :bounds {:min [x0 0.0 -2.7] :max [x1 0.08 -1.2]}})
        candidates [(road :road/wear :road-edge-wear -3.4 -0.9)
                    (road :road/patch :road-patch 0.9 3.4)]
        policy {:hero-junction-road-layer {:required-layer-count 2
                                           :required-material-role-count 2
                                           :minimum-union-area 0.040
                                           :screen-y-range [0.5 1.0]
                                           :maximum-subject-overlap 0.0}}
        result (composition/compose {:resolved-camera resolved :subject-bounds subject
                                     :candidates candidates :policy policy})
        road-evidence (get-in result [:evidence :hero-junction-road-layer])]
    (is (:valid? road-evidence))
    (is (= 2 (:safe-layer-count road-evidence) (:material-role-count road-evidence)))
    (is (>= (:union-area road-evidence) 0.040))))

(deftest disjoint-bounds-set-islands-do-not-inherit-coarse-subject-overlap
  (let [candidate {:id :road/disjoint :bounds-space :final-world
                   ;; Debug/enclosing AABB crosses the subject and must not be projected.
                   :bounds {:min [-2.5 0.0 -0.2] :max [2.5 0.1 0.2]}
                   :bounds-set [{:min [-2.5 0.0 -0.2] :max [-1.5 0.1 0.2]}
                                {:min [1.5 0.0 -0.2] :max [2.5 0.1 0.2]}]}
        result (composition/compose {:resolved-camera resolved :subject-bounds subject
                                     :candidates [candidate]})
        placement (first (:placements result))]
    (is (= :road/disjoint (:id placement)))
    (is (= 2 (count (:screen-pieces placement))))
    (is (pos? (get-in result [:evidence :projected-union-area])))
    (is (not (some #{:subject-exclusion} (mapcat :reasons (:rejected result)))))))

(deftest actual-overlapping-bounds-set-piece-rejects
  (let [candidate {:id :road/overlap :bounds-space :final-world
                   :bounds {:min [-2.5 0.0 -0.2] :max [2.5 0.1 0.2]}
                   :bounds-set [{:min [-2.5 0.0 -0.2] :max [-1.5 0.1 0.2]}
                                {:min [-0.2 0.0 -0.2] :max [0.2 0.1 0.2]}]}
        failure (try (composition/compose {:resolved-camera resolved
                                           :subject-bounds subject
                                           :candidates [candidate]})
                     nil (catch #?(:clj Exception :cljs js/Error) error error))]
    (is (some #{:subject-exclusion} (get-in (ex-data failure) [:evaluated 0 :reasons])))))

(deftest bounds-set-projection-remains-piece-exact-under-rotated-camera
  (let [rotated (camera/resolve-camera {:subject-bounds subject :orbit :three-quarter-right})
        candidate {:id :road/rotated-islands :bounds-space :final-world
                   :bounds {:min [1.5 0.0 0.5] :max [4.0 0.1 2.0]}
                   :bounds-set [{:min [1.5 0.0 0.5] :max [2.0 0.1 1.0]}
                                {:min [3.5 0.0 1.5] :max [4.0 0.1 2.0]}]}
        a (composition/compose {:resolved-camera rotated :subject-bounds subject
                                :candidates [candidate]})
        b (composition/compose {:resolved-camera rotated :subject-bounds subject
                                :candidates [candidate]})]
    (is (= a b))
    (is (= 2 (count (get-in a [:placements 0 :screen-pieces]))))
    (is (pos? (get-in a [:evidence :projected-union-area])))))

(deftest adversarial-320-candidate-selection-is-bounded-and-fast
  (let [candidates
        (vec (for [i (range 320)
                   :let [side (if (even? i) :left :right)
                         sign (if (= side :left) -1.0 1.0)
                         x (* sign (+ 1.5 (* 0.0125 (mod i 80))))
                         z (+ -2.4 (* 0.06 (quot i 80)))]]
               {:id (keyword "adversarial" (str i)) :priority (- 1000 i)
                :composition-region (keyword (str "foreground-" (name side)))
                :cluster-role (if (zero? (mod i 3)) :vegetation :solid-prop)
                :kind (nth [:grass :rock :crate] (mod i 3))
                :geometry-variant (keyword (str "variant-" (mod i 7)))
                :bounds {:min [(- x 0.16) 0.0 z]
                         :max [(+ x 0.16) 0.45 (+ z 0.20)]}}))
        policy {:maximum-selected 8
                :required-composition-region-counts
                {:foreground-left 3 :foreground-right 3}
                :required-cluster-roles-by-composition-region
                {:foreground-left #{:vegetation :solid-prop}
                 :foreground-right #{:vegetation :solid-prop}}
                :required-diversity-by-composition-region
                {:foreground-left {:kind 2 :geometry-variant 2}
                 :foreground-right {:kind 2 :geometry-variant 2}}}
        input {:resolved-camera resolved :subject-bounds subject
               :candidates candidates :policy policy}
        _warm (composition/compose input)
        runs #?(:clj (vec (for [_ (range 3)]
                            (let [started (System/nanoTime)
                                  result (composition/compose input)]
                              {:result result
                               :elapsed-ms (/ (- (System/nanoTime) started) 1.0e6)})))
                :cljs [{:result (composition/compose input) :elapsed-ms 0}])
        result (:result (first runs))
        timings (sort (map :elapsed-ms runs))
        median-ms (nth timings (quot (count timings) 2))
        maximum-ms (last timings)]
    (is (= 320 (get-in result [:evidence :selection-budget :candidate-count])))
    (is (= 8 (count (:placements result))))
    ;; Warm median resists shared-runner contention while the max catches runaway work.
    #?(:clj (do (is (< median-ms 2000.0))
                (is (< maximum-ms 5000.0)))
       :cljs (is true))))

(deftest selection-budget-fails-closed-before-unbounded-projection
  (let [candidates (vec (for [i (range 513)]
                          {:id (keyword "over-budget" (str i))
                           :bounds {:min [2.0 0.0 0.0] :max [2.2 0.3 0.2]}}))
        failure (try
                  (composition/compose {:resolved-camera resolved :subject-bounds subject
                                        :candidates candidates})
                  nil (catch #?(:clj Exception :cljs js/Error) error error))]
    (is (= 513 (get-in (ex-data failure) [:selection-budget :candidate-count])))
    (is (= 512 (get-in (ex-data failure)
                       [:selection-budget :limits :maximum-candidates])))))
