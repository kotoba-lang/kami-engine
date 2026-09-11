(ns kami.render.environment-composition
  "Camera-safe environment composition using production character camera evidence."
  (:require [kami.render.character-camera :as character-camera]))

(def contract :kotoba.render/camera-safe-environment-composition-v1)
(def family-boundary {:families #{:stylized :photoreal}
                      :implemented-families #{:stylized} :same-api? true})

(def ^:private default-policy
  {:safe-screen-bounds {:min [0.04 0.04] :max [0.96 0.96]}
   :ground-contact-screen-y-range [0.38 0.92]
   :subject-padding 0.035 :ground-contact-tolerance 0.025
   :maximum-selected 8 :required-composition-regions #{}
   :required-composition-region-counts {}
   :required-cluster-roles-by-composition-region {}
   :screen-occupancy-grid [4 4]
   :required-screen-occupancy-cells-by-composition-region {}
   :minimum-projected-union-area 0.0
   :minimum-projected-union-area-by-composition-region {}
   :required-diversity-by-composition-region {}})

(def ^:private default-selection-budget
  {:maximum-candidates 512 :maximum-projected-pieces 4096
   :maximum-constraints 128})

(defn- radians [degrees] (* degrees (/ #?(:clj Math/PI :cljs js/Math.PI) 180.0)))
(defn- v- [a b] (mapv - a b))
(defn- dot [a b] (reduce + (map * a b)))
(defn- cross [[ax ay az] [bx by bz]]
  [(- (* ay bz) (* az by)) (- (* az bx) (* ax bz)) (- (* ax by) (* ay bx))])
(defn- normalize [v]
  (let [n (#?(:clj Math/sqrt :cljs js/Math.sqrt) (dot v v))]
    (when (> n 1.0e-9) (mapv #(/ % n) v))))

(defn camera-ground-facing
  "Return the camera-facing ground-plane orientation for +Z-forward props.

  The direction points from camera look-at toward camera position. Renderer and
  Studio use this before producing final world AABBs for camera-facing props."
  [resolved-camera]
  (when-not (= character-camera/contract (:contract resolved-camera))
    (throw (ex-info "Ground-facing orientation requires a resolved production camera"
                    {:expected character-camera/contract :actual (:contract resolved-camera)})))
  (let [{:keys [position look-at]} (:camera resolved-camera)
        direction (normalize [(- (nth position 0) (nth look-at 0)) 0.0
                              (- (nth position 2) (nth look-at 2))])]
    (when-not direction
      (throw (ex-info "Camera has no ground-plane facing direction"
                      {:position position :look-at look-at})))
    (let [[x _ z] direction
          yaw (#?(:clj Math/atan2 :cljs js/Math.atan2) x z)
          half (* 0.5 yaw)]
      {:direction direction :yaw-radians yaw
       :rotation [0.0 (#?(:clj Math/sin :cljs js/Math.sin) half) 0.0
                  (#?(:clj Math/cos :cljs js/Math.cos) half)]
       :forward-axis :positive-z :plane :ground-xz})))

(defn- corners [{:keys [min max]}]
  (for [x [(nth min 0) (nth max 0)]
        y [(nth min 1) (nth max 1)]
        z [(nth min 2) (nth max 2)]] [x y z]))

(defn- camera-basis [{:keys [position look-at up]}]
  (let [forward (normalize (v- look-at position))
        right (normalize (cross up forward))
        corrected-up (when (and forward right) (normalize (cross forward right)))]
    (when-not (and forward right corrected-up)
      (throw (ex-info "Environment projection requires a non-degenerate camera basis"
                      {:position position :look-at look-at :up up})))
    {:forward forward :right right :up corrected-up}))

(defn project-point
  "Project a world point to normalized screen coordinates using a resolved camera map."
  [camera point]
  (let [{:keys [forward right up]} (camera-basis camera)
        relative (v- point (:position camera))
        depth (dot relative forward)
        aspect (/ (double (get-in camera [:viewport :width]))
                  (double (get-in camera [:viewport :height])))
        tan-half (#?(:clj Math/tan :cljs js/Math.tan)
                  (* 0.5 (radians (:vertical-fov-deg camera))))]
    (when (> depth (:near camera))
      (let [ndc-x (/ (dot relative right) (* depth tan-half aspect))
            ndc-y (/ (dot relative up) (* depth tan-half))]
        {:screen [(+ 0.5 (* 0.5 ndc-x)) (- 0.5 (* 0.5 ndc-y))]
         :depth depth :in-front? true}))))

(defn- projection-context [camera]
  (merge (camera-basis camera)
         {:position (:position camera) :near (:near camera)
          :aspect (/ (double (get-in camera [:viewport :width]))
                     (double (get-in camera [:viewport :height])))
          :tan-half (#?(:clj Math/tan :cljs js/Math.tan)
                     (* 0.5 (radians (:vertical-fov-deg camera))))}))

(defn- project-point* [{:keys [position forward right up near aspect tan-half]} point]
  (let [relative (v- point position) depth (dot relative forward)]
    (when (> depth near)
      {:screen [(+ 0.5 (* 0.5 (/ (dot relative right) (* depth tan-half aspect))))
                (- 0.5 (* 0.5 (/ (dot relative up) (* depth tan-half))))]
       :depth depth :in-front? true})))

(defn- project-aabb* [projection bounds]
  (let [projected (mapv #(project-point* projection %) (corners bounds))]
    (when (every? some? projected)
      (let [points (map :screen projected)
            mn (reduce #(mapv min %1 %2) [##Inf ##Inf] points)
            mx (reduce #(mapv max %1 %2) [##-Inf ##-Inf] points)]
        {:min mn :max mx :corners projected
         :depth-range [(apply min (map :depth projected))
                       (apply max (map :depth projected))]}))))

(defn project-aabb
  "Project all eight AABB corners; returns nil if any corner is behind the near plane."
  [camera bounds]
  (let [projected (mapv #(project-point camera %) (corners bounds))]
    (when (every? some? projected)
      (let [points (map :screen projected)
            mn (reduce #(mapv min %1 %2) [##Inf ##Inf] points)
            mx (reduce #(mapv max %1 %2) [##-Inf ##-Inf] points)]
        {:min mn :max mx :corners projected
         :depth-range [(apply min (map :depth projected))
                       (apply max (map :depth projected))]}))))

(defn- pad-rect [{:keys [min max]} padding]
  {:min (mapv #(- % padding) min) :max (mapv #(+ % padding) max)})

(defn- within? [{mn :min mx :max} {safe-min :min safe-max :max}]
  (every? true? (concat (map <= safe-min mn) (map <= mx safe-max))))

(defn- intersects? [{a-min :min a-max :max} {b-min :min b-max :max}]
  (every? true? (map (fn [amn amx bmn bmx] (and (<= amn bmx) (<= bmn amx)))
                     a-min a-max b-min b-max)))

(defn- rect-area [{mn :min mx :max}]
  (* (max 0.0 (- (first mx) (first mn)))
     (max 0.0 (- (second mx) (second mn)))))

(defn- intersection-area [{a-min :min a-max :max} {b-min :min b-max :max}]
  (* (max 0.0 (- (min (first a-max) (first b-max))
                   (max (first a-min) (first b-min))))
     (max 0.0 (- (min (second a-max) (second b-max))
                   (max (second a-min) (second b-min))))))

(defn- intersection-rect [{a-min :min a-max :max} {b-min :min b-max :max}]
  (let [mn [(max (first a-min) (first b-min)) (max (second a-min) (second b-min))]
        mx [(min (first a-max) (first b-max)) (min (second a-max) (second b-max))]]
    (when (and (< (first mn) (first mx)) (< (second mn) (second mx)))
      {:min mn :max mx})))

(defn- rect-union-area [rects]
  (let [xs (sort (distinct (mapcat (juxt #(get-in % [:min 0]) #(get-in % [:max 0])) rects)))]
    (reduce
     + 0.0
     (for [[x0 x1] (partition 2 1 xs)
           :when (< x0 x1)
           :let [intervals (sort-by first
                                    (for [r rects
                                          :when (and (< (get-in r [:min 0]) x1)
                                                     (> (get-in r [:max 0]) x0))]
                                      [(get-in r [:min 1]) (get-in r [:max 1])]))
                 merged (reduce (fn [{:keys [end total]} [a b]]
                                  (if (> a end)
                                    {:end b :total (+ total (- b a))}
                                    {:end (max end b)
                                     :total (+ total (max 0.0 (- b end)))}))
                                {:end ##-Inf :total 0.0} intervals)]]
       (* (- x1 x0) (:total merged))))))

(defn- screen-cell-rect [[cols rows] cell]
  (let [[col row] (case cell
                    :lower-left [0 (dec rows)]
                    :lower-right [(dec cols) (dec rows)]
                    cell)]
    {:min [(/ col (double cols)) (/ row (double rows))]
     :max [(/ (inc col) (double cols)) (/ (inc row) (double rows))]}))

(defn- rect-center [{:keys [min max]}] (mapv #(* 0.5 (+ %1 %2)) min max))
(defn- enclose-rects [rects]
  (when (seq rects)
    {:min [(apply min (map #(get-in % [:min 0]) rects))
           (apply min (map #(get-in % [:min 1]) rects))]
     :max [(apply max (map #(get-in % [:max 0]) rects))
           (apply max (map #(get-in % [:max 1]) rects))]}))
(defn- distance2 [a b]
  (#?(:clj Math/sqrt :cljs js/Math.sqrt) (reduce + (map (fn [x y] (let [d (- x y)] (* d d))) a b))))

(defn- reserve-distinct [eligible reserved region attribute required]
  (loop [remaining eligible result (vec reserved)
         values (set (keep #(get-in % [:candidate attribute])
                           (filter #(= region (:composition-region %)) reserved)))]
    (if (or (>= (count values) required) (empty? remaining))
      result
      (let [candidate (first remaining) value (get-in candidate [:candidate attribute])]
        (if (and (= region (:composition-region candidate)) value
                 (not (contains? values value))
                 (not (some #(= (:id candidate) (:id %)) result)))
          (recur (rest remaining) (conj result candidate) (conj values value))
          (recur (rest remaining) result values))))))

(defn- union-of [entries]
  (rect-union-area (mapcat :screen-pieces entries)))

(defn- incremental-union-gain [selected-pieces candidate]
  (let [intersections (keep identity
                            (for [candidate-piece (:screen-pieces candidate)
                                  selected-piece selected-pieces]
                              (intersection-rect candidate-piece selected-piece)))]
    (max 0.0 (- (:screen-area candidate) (rect-union-area intersections)))))

(defn- coverage-fill [selected candidates count]
  (loop [chosen (vec selected)
         selected-pieces (vec (mapcat :screen-pieces selected))
         remaining (vec candidates)
         slots count]
    (if (or (zero? slots) (empty? remaining))
      chosen
      (let [best (reduce (fn [winner candidate]
                           (let [gain (incremental-union-gain selected-pieces candidate)]
                             (if (or (nil? winner) (> gain (:gain winner)))
                               {:candidate candidate :gain gain} winner)))
                         nil remaining)
            id (get-in best [:candidate :id])]
        (recur (conj chosen (:candidate best))
               (into selected-pieces (:screen-pieces (:candidate best)))
               (vec (remove #(= id (:id %)) remaining))
               (dec slots))))))

(defn- region-constraints-valid? [selected region policy]
  (let [entries (filter #(= region (:composition-region %)) selected)
        cells (set (mapcat :occupied-screen-cells entries))
        roles (set (keep :cluster-role entries))]
    (and (every? cells (get-in policy [:required-screen-occupancy-cells-by-composition-region
                                       region] #{}))
         (every? roles (get-in policy [:required-cluster-roles-by-composition-region
                                       region] #{}))
         (every? (fn [[attribute required]]
                   (>= (count (set (keep #(get-in % [:candidate attribute]) entries))) required))
                 (get-in policy [:required-diversity-by-composition-region region] {})))))

(defn- optimize-region-coverage [selected eligible region policy maximum-swaps target-area]
  (loop [current (vec selected) swaps 0]
    (let [region-selected (vec (filter #(= region (:composition-region %)) current))
          selected-ids (set (map :id current))
          alternatives (remove #(contains? selected-ids (:id %)) eligible)
          base (union-of region-selected)
          best (when (< base target-area)
                 (reduce
                (fn [winner [out replacement]]
                  (let [trial (mapv #(if (= (:id %) (:id out)) replacement %) current)
                        trial-region (filter #(= region (:composition-region %)) trial)
                        gain (- (union-of trial-region) base)]
                    (if (and (> gain 1.0e-12) (region-constraints-valid? trial region policy)
                             (or (nil? winner) (> gain (:gain winner))))
                      {:selected trial :gain gain :out (:id out) :in (:id replacement)} winner)))
                nil (for [out region-selected replacement alternatives] [out replacement])))]
      (if (and best (< swaps maximum-swaps))
        (recur (:selected best) (inc swaps))
        {:selected current :swap-count swaps :exhausted? (nil? best)
         :best-union-area (union-of (filter #(= region (:composition-region %)) current))}))))

(defn- facade-readability-evidence [projection subject-screen selected policy]
  (when policy
    (let [minimum-extent (:minimum-layer-extent policy 0.0)
          maximum-overlap (:maximum-subject-overlap policy 0.0)
          layers (vec (for [entry selected
                            layer (get-in entry [:candidate :facade-layer-bounds])
                            :let [screen (project-aabb* projection (:bounds layer))
                                  extent (when screen
                                           (max (- (get-in screen [:max 0]) (get-in screen [:min 0]))
                                                (- (get-in screen [:max 1]) (get-in screen [:min 1]))))
                                  overlap (if screen (intersection-area screen subject-screen) 0.0)]]
                        (assoc layer :candidate-id (:id entry) :screen-bounds screen
                               :screen-extent extent :subject-overlap overlap
                               :readable? (and screen (>= extent minimum-extent)
                                               (<= overlap maximum-overlap)))))
          readable (vec (filter :readable? layers))
          centers (mapv #(rect-center (:screen-bounds %)) readable)
          separations (for [i (range (count centers)) j (range (inc i) (count centers))]
                        (distance2 (nth centers i) (nth centers j)))
          minimum-separation (if (seq separations) (apply min separations) ##Inf)
          roles (set (keep :role readable))
          valid? (and (>= (count readable) (:required-layer-count policy 0))
                      (>= (count roles) (:required-distinct-roles policy 0))
                      (>= minimum-separation (:minimum-layer-separation policy 0.0)))]
      {:valid? valid? :layer-count (count layers) :readable-layer-count (count readable)
       :distinct-roles roles :distinct-role-count (count roles)
       :minimum-layer-separation minimum-separation
       :required-layer-count (:required-layer-count policy 0)
       :required-distinct-roles (:required-distinct-roles policy 0)
       :minimum-layer-extent minimum-extent :maximum-subject-overlap maximum-overlap
       :layers layers})))

(defn- road-layer-evidence [subject-screen selected policy]
  (when policy
    (let [y-range (:screen-y-range policy [0.5 1.0])
          layers (vec (for [entry selected
                            :let [eligibility (get-in entry [:candidate :attachment-eligibility])
                                  screen (:screen-bounds entry)
                                  overlap (reduce + 0.0
                                                  (map #(intersection-area % subject-screen)
                                                       (:screen-pieces entry)))
                                  center-y (second (rect-center screen))]
                            :when (= (:required-target policy :road-surface) (:target eligibility))]
                        {:id (:id entry) :material-role (get-in entry [:candidate :material-role])
                         :screen-bounds screen :screen-pieces (:screen-pieces entry)
                         :subject-overlap overlap
                         :center-y center-y :eligibility eligibility
                         :safe? (and (:subject-exclusion-required? eligibility)
                                     (= (:required-space policy :neighborhood-world)
                                        (:space eligibility))
                                     (= (:required-anchor policy :junction-center)
                                        (:anchor eligibility))
                                     (contains? (:eligible-regions eligibility)
                                                (:composition-region entry))
                                     (<= overlap (:maximum-subject-overlap policy 0.0))
                                     (<= (first y-range) center-y (second y-range)))}))
          safe (vec (filter :safe? layers))
          roles (set (keep :material-role safe))
          union-area (rect-union-area (mapcat :screen-pieces safe))
          valid? (and (>= (count safe) (:required-layer-count policy 0))
                      (>= (count roles) (:required-material-role-count policy 0))
                      (>= union-area (:minimum-union-area policy 0.0)))]
      {:valid? valid? :layer-count (count layers) :safe-layer-count (count safe)
       :material-roles roles :material-role-count (count roles) :union-area union-area
       :required-layer-count (:required-layer-count policy 0)
       :required-material-role-count (:required-material-role-count policy 0)
       :minimum-union-area (:minimum-union-area policy 0.0) :screen-y-range y-range
       :required-target (:required-target policy :road-surface)
       :required-space (:required-space policy :neighborhood-world)
       :required-anchor (:required-anchor policy :junction-center)
       :layers layers})))

(defn- ground-contact [{:keys [min max]}]
  [(* 0.5 (+ (nth min 0) (nth max 0))) (nth min 1)
   (* 0.5 (+ (nth min 2) (nth max 2)))])

(defn compose
  "Select deterministic environment candidates that are safe in a production camera.

  Candidates are `{:id ... :bounds {:min [...] :max [...]} :priority number}`.
  The returned placements retain world semantics and never alter skinned selection."
  [{:keys [family resolved-camera subject-bounds candidates ground-y policy]
    :or {family :stylized candidates [] ground-y 0.0 policy {}}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Environment composition family is not implemented" {:family family})))
  (when-not (= character-camera/contract (:contract resolved-camera))
    (throw (ex-info "Environment composition requires a resolved production camera"
                    {:expected character-camera/contract :actual (:contract resolved-camera)})))
  (when-not (= :preserve-all (get-in resolved-camera [:render-selection :world :mode]))
    (throw (ex-info "Environment composition refuses a camera that removes world context"
                    {:render-selection (:render-selection resolved-camera)})))
  (let [policy (merge default-policy policy)
        camera (:camera resolved-camera)
        projection (projection-context camera)
        subject-screen (some-> (project-aabb* projection subject-bounds)
                               (pad-rect (:subject-padding policy)))
        _ (when-not subject-screen
            (throw (ex-info "Environment composition cannot project subject bounds"
                            {:subject-bounds subject-bounds})))
        required-cells (:required-screen-occupancy-cells-by-composition-region policy)
        all-required-cells (set (mapcat identity (vals required-cells)))
        selection-budget (merge default-selection-budget (:selection-budget policy))
        candidate-count (count candidates)
        projected-piece-count (reduce + 0 (map #(max 1 (count (:bounds-set %))) candidates))
        constraint-count (+ (reduce + 0 (map count (vals required-cells)))
                            (reduce + 0 (map count (vals (:required-cluster-roles-by-composition-region policy))))
                            (reduce + 0 (map count (vals (:required-diversity-by-composition-region policy))))
                            (count (:required-composition-region-counts policy))
                            (if (:facade-readability policy) 1 0)
                            (if (:hero-junction-road-layer policy) 1 0))
        budget-evidence {:candidate-count candidate-count
                         :projected-piece-count projected-piece-count
                         :constraint-count constraint-count :limits selection-budget}
        _ (when (or (> candidate-count (:maximum-candidates selection-budget))
                    (> projected-piece-count (:maximum-projected-pieces selection-budget))
                    (> constraint-count (:maximum-constraints selection-budget)))
            (throw (ex-info "Environment composition failed closed: selection budget exceeded"
                            {:contract contract :selection-budget budget-evidence})))
        ordered (sort-by (juxt #( - (or (:priority %) 0)) #(pr-str (:id %))) candidates)
        ordered-index (into {} (map-indexed (fn [index candidate] [(:id candidate) index]) ordered))
        evaluated
        (mapv (fn [{:keys [id bounds] :as candidate}]
                (let [piece-bounds (vec (or (seq (:bounds-set candidate)) [bounds]))
                      projected-pieces (mapv #(project-aabb* projection %) piece-bounds)
                      screen-pieces (vec (keep identity projected-pieces))
                      all-in-front? (= (count piece-bounds) (count screen-pieces))
                      screen (when all-in-front? (enclose-rects screen-pieces))
                      contacts (mapv ground-contact piece-bounds)
                      contact-screens (mapv #(project-point* projection %) contacts)
                      contact-screen (first contact-screens)
                      contact-screen-y (some-> contact-screen :screen second)
                      ground-band (or (:ground-contact-screen-y-range candidate)
                                      (:ground-contact-screen-y-range policy))
                      screen-extent (when screen
                                      (apply max
                                             (for [piece screen-pieces]
                                               (max (- (get-in piece [:max 0]) (get-in piece [:min 0]))
                                                    (- (get-in piece [:max 1]) (get-in piece [:min 1]))))))
                      extent-range (:screen-extent-range candidate)
                      ground-errors (mapv #(#?(:clj Math/abs :cljs js/Math.abs)
                                              (- (second %) ground-y)) contacts)
                      ground-error (apply max ground-errors)
                      screen-side (:screen-side candidate)
                      side-valid? (case screen-side
                                    nil true
                                    :left (and screen (<= (get-in screen [:max 0])
                                                           (get-in subject-screen [:min 0])))
                                    :right (and screen (>= (get-in screen [:min 0])
                                                            (get-in subject-screen [:max 0])))
                                    false)
                      reasons (cond-> []
                                (not all-in-front?) (conj :behind-camera)
                                (and (seq (:bounds-set candidate))
                                     (not= :final-world (:bounds-space candidate)))
                                (conj :bounds-set-space-invalid)
                                (and screen (not-every? #(within? % (:safe-screen-bounds policy))
                                                        screen-pieces))
                                (conj :outside-safe-screen)
                                (and screen (some #(intersects? % subject-screen) screen-pieces))
                                (conj :subject-exclusion)
                                (and screen-extent extent-range
                                     (not (<= (first extent-range) screen-extent
                                              (second extent-range))))
                                (conj :screen-extent-outside-range)
                                (not side-valid?) (conj :screen-side-mismatch)
                                (> ground-error (:ground-contact-tolerance policy))
                                (conj :not-grounded)
                                (or (some nil? contact-screens)
                                    (some #(not (within? {:min (:screen %) :max (:screen %)}
                                                         (:safe-screen-bounds policy)))
                                          (keep identity contact-screens)))
                                (conj :ground-contact-not-visible))
                      reasons (cond-> reasons
                                (some (fn [projected]
                                        (let [y (some-> projected :screen second)]
                                          (and y (not (<= (first ground-band) y
                                                          (second ground-band))))))
                                      contact-screens)
                                (conj :ground-contact-outside-visible-ground-band))]
                  {:id id :candidate candidate :composition-region (:composition-region candidate)
                   :cluster-role (:cluster-role candidate)
                   :kind (:kind candidate) :geometry-variant (:geometry-variant candidate)
                   :screen-side screen-side
                   :screen-bounds screen :screen-pieces screen-pieces
                   :screen-extent screen-extent
                   :screen-area (if screen (rect-union-area screen-pieces) 0.0)
                   :occupied-screen-cells
                   (set (for [cell all-required-cells
                              :when (and screen
                                         (some #(pos? (intersection-area
                                                       % (screen-cell-rect
                                                          (:screen-occupancy-grid policy) cell)))
                                               screen-pieces))]
                          cell))
                   :ground-contact-screen-y-range ground-band
                   :screen-extent-range extent-range
                   :ground-contact {:world (first contacts) :projection contact-screen
                                    :error ground-error}
                   :ground-contacts (mapv (fn [world projection error]
                                            {:world world :projection projection :error error})
                                          contacts contact-screens ground-errors)
                   :accepted? (empty? reasons) :reasons reasons}))
              ordered)
        eligible (vec (filter :accepted? evaluated))
        eligible-by-region (group-by :composition-region eligible)
        required-roles (:required-cluster-roles-by-composition-region policy)
        required-counts (merge-with max
                                    (zipmap (:required-composition-regions policy) (repeat 1))
                                    (:required-composition-region-counts policy)
                                    (into {} (map (fn [[region roles]] [region (count roles)]))
                                          required-roles))
        required-regions (vec (sort-by pr-str (keys required-counts)))
        cell-reserved
        (vec (mapcat (fn [region]
                       (keep (fn [cell]
                               (first (filter #(and (= region (:composition-region %))
                                                    (contains? (:occupied-screen-cells %) cell))
                                              (get eligible-by-region region []))))
                             (sort-by pr-str (get required-cells region #{}))))
                     (sort-by pr-str (keys required-cells))))
        cell-reserved-ids (set (map :id cell-reserved))
        role-reserved
        (vec (mapcat (fn [region]
                       (keep (fn [role]
                               (when-not (some #(and (= region (:composition-region %))
                                                     (= role (:cluster-role %)))
                                               cell-reserved)
                                 (first (filter #(and (= region (:composition-region %))
                                                      (= role (:cluster-role %))
                                                      (not (contains? cell-reserved-ids (:id %))))
                                                (get eligible-by-region region [])))))
                             (sort-by pr-str (get required-roles region #{}))))
                     required-regions))
        semantic-reserved (vec (distinct (concat cell-reserved role-reserved)))
        diversity-reserved
        (reduce (fn [reserved [region requirements]]
                  (reduce (fn [r [attribute required]]
                            (reserve-distinct (get eligible-by-region region [])
                                              r region attribute required))
                          reserved (sort-by (comp pr-str key) requirements)))
                semantic-reserved
                (sort-by (comp pr-str key) (:required-diversity-by-composition-region policy)))
        semantic-reserved-ids (set (map :id diversity-reserved))
        quota-reserved
        (vec (mapcat (fn [region]
                       (let [current (vec (filter #(= region (:composition-region %))
                                                  diversity-reserved))
                             already (count current)
                             remaining (max 0 (- (get required-counts region) already))]
                         (remove #(contains? semantic-reserved-ids (:id %))
                                 (coverage-fill current
                                                (filter #(not (contains? semantic-reserved-ids
                                                                         (:id %)))
                                                        (get eligible-by-region region []))
                                                remaining))))
                     required-regions))
        reserved (vec (concat diversity-reserved quota-reserved))
        reserved-ids (set (map :id reserved))
        fillers (remove #(contains? reserved-ids (:id %)) eligible)
        initially-selected (coverage-fill reserved fillers
                                          (max 0 (- (:maximum-selected policy)
                                                    (count reserved))))
        coverage-regions (sort-by pr-str
                                  (keys (:minimum-projected-union-area-by-composition-region policy)))
        optimization
        (reduce (fn [{:keys [selected regions]} region]
                  (let [result (optimize-region-coverage
                                selected (get eligible-by-region region []) region policy
                                (get-in policy [:coverage-selection-budget :maximum-swaps-per-region] 16)
                                (get-in policy [:minimum-projected-union-area-by-composition-region
                                                region] 0.0))]
                    {:selected (:selected result) :regions (assoc regions region (dissoc result :selected))}))
                {:selected initially-selected :regions (sorted-map)} coverage-regions)
        selected (vec (sort-by #(get ordered-index (:id %) ##Inf) (:selected optimization)))
        selected-ids (set (map :id selected))
        rejected (vec (remove :accepted? evaluated))
        unselected-safe (vec (remove #(contains? selected-ids (:id %)) eligible))
        region-counts (frequencies (keep :composition-region selected))
        region-shortages (into (sorted-map)
                               (keep (fn [[region required]]
                                       (let [missing (- required (get region-counts region 0))]
                                         (when (pos? missing) [region missing]))))
                               required-counts)
        missing-regions (vec (keys region-shortages))
        selected-role-coverage
        (into (sorted-map)
              (for [region (sort-by pr-str (keys required-roles))]
                [region (set (keep :cluster-role
                                   (filter #(= region (:composition-region %)) selected)))]))
        missing-roles
        (into (sorted-map)
              (keep (fn [[region roles]]
                      (let [missing (set (remove (get selected-role-coverage region #{}) roles))]
                        (when (seq missing) [region missing]))))
              required-roles)
        selected-cell-coverage
        (into (sorted-map)
              (for [region (sort-by pr-str (keys required-cells))]
                [region (set (mapcat :occupied-screen-cells
                                     (filter #(= region (:composition-region %)) selected)))]))
        missing-cells
        (into (sorted-map)
              (keep (fn [[region cells]]
                      (let [missing (set (remove (get selected-cell-coverage region #{}) cells))]
                        (when (seq missing) [region missing]))))
              required-cells)
        diversity-coverage
        (into (sorted-map)
              (for [[region requirements] (:required-diversity-by-composition-region policy)]
                [region
                 (into (sorted-map)
                       (for [[attribute _] requirements]
                         [attribute
                          (set (keep #(get-in % [:candidate attribute])
                                     (filter #(= region (:composition-region %)) selected)))]))]))
        diversity-shortages
        (into (sorted-map)
              (keep (fn [[region requirements]]
                      (let [shortages (into (sorted-map)
                                            (keep (fn [[attribute required]]
                                                    (let [missing (- required
                                                                     (count (get-in diversity-coverage
                                                                                    [region attribute] #{})))]
                                                      (when (pos? missing) [attribute missing]))))
                                            requirements)]
                        (when (seq shortages) [region shortages]))))
              (:required-diversity-by-composition-region policy))
        aggregate-area (reduce + 0.0 (map :screen-area selected))
        union-area (rect-union-area (mapcat :screen-pieces selected))
        region-union-areas
        (into (sorted-map)
              (for [region (sort-by pr-str (set (keep :composition-region selected)))]
                [region (rect-union-area
                         (mapcat :screen-pieces
                                 (filter #(= region (:composition-region %)) selected)))]))
        area-shortage (max 0.0 (- (:minimum-projected-union-area policy) union-area))
        region-area-shortages
        (into (sorted-map)
              (keep (fn [[region minimum]]
                      (let [missing (- minimum (get region-union-areas region 0.0))]
                        (when (pos? missing) [region missing]))))
              (:minimum-projected-union-area-by-composition-region policy))
        facade-evidence (facade-readability-evidence projection subject-screen selected
                                                       (:facade-readability policy))
        road-evidence (road-layer-evidence subject-screen selected
                                           (:hero-junction-road-layer policy))
        evidence {:valid? (and (boolean (seq selected)) (empty? missing-regions)
                               (empty? missing-roles) (empty? missing-cells)
                               (empty? diversity-shortages) (zero? area-shortage)
                               (empty? region-area-shortages)
                               (or (nil? facade-evidence) (:valid? facade-evidence))
                               (or (nil? road-evidence) (:valid? road-evidence)))
                  :candidate-count (count candidates)
                  :selected-count (count selected)
                  :rejected-count (count rejected)
                  :unselected-safe-count (count unselected-safe)
                  :unselected-safe (mapv :id unselected-safe)
                  :safe-screen-bounds (:safe-screen-bounds policy)
                  :ground-contact-screen-y-range (:ground-contact-screen-y-range policy)
                  :selected-ground-contact-screen-y
                  (into {} (map (fn [entry]
                                  [(:id entry) (get-in entry [:ground-contact :projection :screen 1])])
                                selected))
                  :selected-ground-contact-screen-y-ranges
                  (into {} (map (juxt :id :ground-contact-screen-y-range) selected))
                  :selected-screen-extents
                  (into {} (map (juxt :id :screen-extent) selected))
                  :required-composition-regions required-regions
                  :required-composition-region-counts required-counts
                  :selected-region-counts region-counts
                  :missing-composition-regions missing-regions
                  :composition-region-shortages region-shortages
                  :required-cluster-roles-by-composition-region required-roles
                  :selected-cluster-roles-by-composition-region selected-role-coverage
                  :missing-cluster-roles-by-composition-region missing-roles
                  :screen-occupancy-grid (:screen-occupancy-grid policy)
                  :required-screen-occupancy-cells-by-composition-region required-cells
                  :selected-screen-occupancy-cells-by-composition-region selected-cell-coverage
                  :missing-screen-occupancy-cells-by-composition-region missing-cells
                  :required-diversity-by-composition-region
                  (:required-diversity-by-composition-region policy)
                  :selected-diversity-by-composition-region diversity-coverage
                  :diversity-shortages-by-composition-region diversity-shortages
                  :aggregate-projected-area aggregate-area :projected-union-area union-area
                  :minimum-projected-union-area (:minimum-projected-union-area policy)
                  :projected-union-area-by-composition-region region-union-areas
                  :projected-union-area-shortage area-shortage
                  :projected-union-area-shortages-by-composition-region region-area-shortages
                  :coverage-selection-objective
                  {:strategy :mandatory-then-incremental-union-greedy-with-bounded-swaps
                   :regions (:regions optimization)
                   :best-achieved-union-area union-area
                   :best-achieved-union-area-by-composition-region region-union-areas
                   :maximum-swaps-per-region
                   (get-in policy [:coverage-selection-budget :maximum-swaps-per-region] 16)}
                  :facade-readability facade-evidence :hero-junction-road-layer road-evidence
                  :selection-budget budget-evidence
                  :subject-exclusion subject-screen :ground-y ground-y
                  :deterministic-order (mapv :id evaluated)
                  :world-context-retained? true}]
    (when-not (:valid? evidence)
      (throw (ex-info "Environment composition failed closed: unsafe or missing required regions"
                      {:contract contract :evidence evidence :evaluated evaluated})))
    {:contract contract :family family :camera-contract character-camera/contract
     :camera-ground-facing (camera-ground-facing resolved-camera)
     :placements (mapv #(select-keys % [:id :candidate :screen-bounds :screen-pieces
                                        :screen-extent
                                        :ground-contact-screen-y-range :screen-extent-range
                                        :ground-contact :ground-contacts]) selected)
     :rejected (mapv #(select-keys % [:id :reasons :screen-bounds :screen-pieces
                                      :screen-extent
                                      :ground-contact-screen-y-range :screen-extent-range
                                      :ground-contact :ground-contacts])
                     rejected)
     :unselected-safe (mapv :id unselected-safe)
     :render-selection {:world {:mode :preserve-all :removed? false}
                        :skinned (get-in resolved-camera [:render-selection :skinned])}
     :evidence evidence}))
