(ns kami.render.capture-lifecycle
  "Pure settle/freeze lifecycle for production character captures."
  (:require [kami.render.character-camera :as character-camera]
            [kami.render.environment-composition :as composition]))

(def contract :kotoba.render/production-capture-lifecycle-v1)
(def capture-presence-schema
  "Authoritative WebGPU queue-submit evidence schema (WebGPU merge 247ae4b)."
  :kotoba.webgpu/capture-presence-evidence-v2)
(def default-policy
  {:stable-frames 3 :transform-epsilon 1.0e-9
   :timeout-ms 5000
   :require-submitted-presence? true
   :safe-frame {:min [0.08 0.08] :max [0.92 0.92]}
   :coverage-range character-camera/coverage-range})

(defn initial-state [] {:contract contract :phase :idle :generation 0})

(defn- radians [degrees]
  (* degrees (/ #?(:clj Math/PI :cljs js/Math.PI) 180.0)))

(defn- transform-value [transform]
  {:translation (vec (or (:translation transform) (:position transform) [0.0 0.0 0.0]))
   :rotation-y-deg (double (or (:rotation-y-deg transform) (:yaw-deg transform) 0.0))})

(defn- bounds-corners [{:keys [min max]}]
  (for [x [(nth min 0) (nth max 0)]
        y [(nth min 1) (nth max 1)]
        z [(nth min 2) (nth max 2)]] [x y z]))

(defn world-subject-bounds
  "Resolve a local subject AABB through host translation and Y rotation."
  [local-bounds transform]
  (when-not (and (= 3 (count (:min local-bounds))) (= 3 (count (:max local-bounds))))
    (throw (ex-info "Capture lifecycle requires local subject bounds"
                    {:contract contract :local-bounds local-bounds})))
  (let [{:keys [translation rotation-y-deg]} (transform-value transform)
        a (radians rotation-y-deg)
        c (#?(:clj Math/cos :cljs js/Math.cos) a)
        s (#?(:clj Math/sin :cljs js/Math.sin) a)
        points (map (fn [[x y z]]
                      (mapv + translation [(+ (* c x) (* s z)) y
                                           (+ (* (- s) x) (* c z))]))
                    (bounds-corners local-bounds))]
    {:min (reduce #(mapv min %1 %2) [##Inf ##Inf ##Inf] points)
     :max (reduce #(mapv max %1 %2) [##-Inf ##-Inf ##-Inf] points)}))

(defn request-freeze
  "Begin a new capture generation. Host fields are explicit and portable."
  [state {:keys [subject-id subject-local-bounds orbit viewport obstacles ground-y policy]
          :or {orbit :three-quarter-right ground-y 0.0 policy {}} :as request}]
  (when-not (and subject-id subject-local-bounds)
    (throw (ex-info "Capture freeze requires subject identity and local bounds"
                    {:contract contract :request request})))
  {:contract contract :phase :settling :generation (inc (:generation state 0))
   :request {:subject-id subject-id :subject-local-bounds subject-local-bounds
             :orbit orbit :viewport viewport :obstacles obstacles :ground-y ground-y}
   :policy (merge default-policy policy) :stable-count 0 :snapshot nil})

(defn- transform-delta [a b]
  (if (and a b)
    (apply max (map #(#?(:clj Math/abs :cljs js/Math.abs) (double (- %1 %2)))
                    (concat (:translation a) [(:rotation-y-deg a)])
                    (concat (:translation b) [(:rotation-y-deg b)])))
    ##Inf))

(defn- intersection-area [{a-min :min a-max :max} {b-min :min b-max :max}]
  (* (max 0.0 (- (min (first a-max) (first b-max)) (max (first a-min) (first b-min))))
     (max 0.0 (- (min (second a-max) (second b-max)) (max (second a-min) (second b-min))))))

(defn- presence-evidence [resolved bounds transform policy]
  (let [screen (composition/project-aabb (:camera resolved) bounds)
        safe (:safe-frame policy)
        center (mapv #(* 0.5 (+ %1 %2)) (:min bounds) (:max bounds))
        stale-camera-delta (apply max (map #(#?(:clj Math/abs :cljs js/Math.abs)
                                                (double (- %1 %2)))
                                           center (get-in resolved [:camera :look-at])))
        area (when screen (* (- (get-in screen [:max 0]) (get-in screen [:min 0]))
                             (- (get-in screen [:max 1]) (get-in screen [:min 1]))))
        overlap (if screen (intersection-area screen safe) 0.0)
        coverage (if screen (- (get-in screen [:max 1]) (get-in screen [:min 1])) 0.0)
        fully-inside? (and screen
                           (every? true? (map <= (:min safe) (:min screen)))
                           (every? true? (map <= (:max screen) (:max safe))))
        meaningful? (and area (pos? area) (>= (/ overlap area) 0.95))
        ground-y (get-in bounds [:min 1])
        ground-contact? (and screen
                             (<= (get-in safe [:min 1]) (get-in screen [:max 1])
                                 (get-in safe [:max 1]))
                             (<= ground-y (get-in bounds [:max 1])))
        valid? (and fully-inside? meaningful?
                    (<= (first (:coverage-range policy)) coverage
                        (second (:coverage-range policy))) ground-contact?
                    (<= stale-camera-delta (:transform-epsilon policy)))]
    {:valid? valid? :projected-corners (vec (:corners screen)) :screen-bounds (select-keys screen [:min :max])
     :fully-inside-safe-frame? (boolean fully-inside?) :meaningfully-intersects-safe-frame? (boolean meaningful?)
     :coverage coverage :coverage-range (:coverage-range policy)
     :ground-contact? (boolean ground-contact?) :stale-camera-delta stale-camera-delta
     :subject-world-transform transform :subject-world-bounds bounds}))

(defn snapshot-tick
  "Snapshot a simulation tick. Movement after settling starts fails closed."
  [state {:keys [tick subject-id subject-transform present? submitted-render-frame-count
                 submitted-subject-presence]
          :or {present? true submitted-render-frame-count 0}}]
  (when-not (= :settling (:phase state))
    (throw (ex-info "Capture snapshot requires settling phase" {:contract contract :state state})))
  (let [expected (get-in state [:request :subject-id])
        transform (transform-value subject-transform)
        previous (get-in state [:snapshot :transform])
        delta (transform-delta previous transform)
        epsilon (get-in state [:policy :transform-epsilon])]
    (when-not (and present? (= expected subject-id))
      (throw (ex-info "Capture subject absent" {:contract contract :valid? false :expected expected
                                                 :actual subject-id :tick tick})))
    (when (and previous (> delta epsilon))
      (throw (ex-info "Capture subject moved during settle"
                      {:contract contract :valid? false :tick tick :transform-delta delta
                       :expected-transform previous :actual-transform transform})))
    (let [stable-count (inc (:stable-count state))
          state (assoc state :stable-count stable-count
                       :snapshot {:tick tick :first-tick (or (get-in state [:snapshot :first-tick]) tick)
                                  :transform transform
                                  :submitted-render-frame-count submitted-render-frame-count
                                  :submitted-subject-presence submitted-subject-presence})]
      (if (< stable-count (get-in state [:policy :stable-frames]))
        state
        (let [{:keys [subject-id subject-local-bounds orbit viewport obstacles ground-y]}
              (:request state)
              bounds (world-subject-bounds subject-local-bounds transform)
              resolved (character-camera/resolve-camera
                        (cond-> {:subject-id subject-id :subject-bounds bounds :orbit orbit
                                 :ground-y ground-y :obstacles (or obstacles [])}
                          viewport (assoc :viewport viewport)))
              evidence (presence-evidence resolved bounds transform (:policy state))
              submission-valid?
              (or (not (get-in state [:policy :require-submitted-presence?]))
                  (and (= capture-presence-schema (:schema submitted-subject-presence))
                       (true? (:submitted? submitted-subject-presence))
                       (contains? (set (:entity-ids submitted-subject-presence)) subject-id)
                       (pos? (:draw-count submitted-subject-presence 0))))]
          (when-not (and (:valid? evidence) submission-valid?)
            (throw (ex-info "Capture subject presence failed closed"
                            {:contract contract
                             :evidence (assoc evidence
                                              :submitted-subject-presence-valid?
                                              (boolean submission-valid?)
                                              :submitted-subject-presence
                                              submitted-subject-presence)
                             :tick tick})))
          (assoc state :phase :frozen :frozen? true :resolved-camera resolved
                 :evidence (assoc evidence
                                  :state :settled :frozen? true :subject-entity-id subject-id
                                  :simulation-tick-before (get-in state [:snapshot :first-tick])
                                  :simulation-tick-after tick
                                  :submitted-render-frame-count submitted-render-frame-count
                                  :submitted-subject-presence-valid? true
                                  :submitted-subject-presence submitted-subject-presence
                                  :timeout-ms (get-in state [:policy :timeout-ms])
                                  :resolved-camera-eye (get-in resolved [:camera :position])
                                  :resolved-camera-target (get-in resolved [:camera :look-at])
                                  :projected-subject-bounds (:screen-bounds evidence)
                                  :projected-subject-visible? (:valid? evidence))
                 :render-selection (:render-selection resolved)))))))

(defn release [state]
  (when-not (= :frozen (:phase state))
    (throw (ex-info "Capture release requires frozen phase" {:contract contract :state state})))
  {:contract contract :phase :released :state :released :frozen? false
   :generation (:generation state)
   :released-tick (get-in state [:snapshot :tick])})

(defn timeout
  "Close an unfinished request without publishing a camera."
  [state elapsed-ms]
  (when-not (= :settling (:phase state))
    (throw (ex-info "Capture timeout requires settling phase" {:contract contract :state state})))
  (let [limit (get-in state [:policy :timeout-ms])]
    (when (< elapsed-ms limit)
      (throw (ex-info "Capture timeout requested before deadline"
                      {:contract contract :elapsed-ms elapsed-ms :timeout-ms limit})))
    {:contract contract :phase :failed :state :timeout :frozen? false
     :generation (:generation state) :timeout-ms limit :elapsed-ms elapsed-ms
     :evidence {:valid? false :state :timeout :timeout-ms limit
                :subject-entity-id (get-in state [:request :subject-id])}}))

(defn reset [state]
  {:contract contract :phase :idle :generation (:generation state 0)})
