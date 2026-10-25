(ns kotoba.amenominaka.timeline
  "M9 (ADR-2607100100): omni.timeline parity — D7's own scoping is
  explicitly \"minimal keyframe/camera-path\", not a full USD-stage
  animation timeline (scrubbing arbitrary property animation, layers,
  etc. — out of scope, same 'documented gap over guessed implementation'
  stance as M1's MaterialX-binding gap and M7's environment-metadata gap).

  A camera path is an ordered vector of keyframes:

    [{:t 0.0 :eye [x y z] :target [x y z]}
     {:t 2.0 :eye [x y z] :target [x y z]} ...]

  `eval-at` linearly interpolates `:eye`/`:target` between the two
  keyframes bounding a given time (clamping to the first/last keyframe's
  values outside the path's range). Pure `.cljc`, no platform code —
  works identically on the JVM (tests) and in the browser (the UI's real
  requestAnimationFrame playback loop, kotoba.amenominaka.ui).")

(defn duration
  "The path's total length in seconds — the last keyframe's :t, or 0.0
  for an empty/single-keyframe path."
  [timeline]
  (if (empty? timeline) 0.0 (:t (last timeline))))

(def ^:private keyframe-spacing 2.0) ;; seconds between auto-appended keyframes

(defn add-keyframe
  "Append a new keyframe at `eye`/`target`, auto-spaced `keyframe-spacing`
  seconds after the current last one (0.0 for the first)."
  [timeline eye target]
  (conj (vec timeline)
        {:t (if (empty? timeline) 0.0 (+ (duration timeline) keyframe-spacing))
         :eye eye :target target}))

(defn clear [_timeline] [])

(defn delete-keyframe [timeline index]
  (when-not (< -1 index (count timeline))
    (throw (ex-info "camera keyframe index out of range" {:index index})))
  (vec (concat (subvec (vec timeline) 0 index) (subvec (vec timeline) (inc index)))))

(defn move-keyframe [timeline index new-time]
  (when-not (< -1 index (count timeline))
    (throw (ex-info "camera keyframe index out of range" {:index index})))
  (let [previous (when (pos? index) (:t (nth timeline (dec index))))
        next-time (when (< index (dec (count timeline))) (:t (nth timeline (inc index))))]
    (when (or (neg? new-time) (and previous (<= new-time previous)) (and next-time (>= new-time next-time)))
      (throw (ex-info "camera keyframe time must remain strictly ordered" {:index index :time new-time})))
    (assoc (vec timeline) index (assoc (nth timeline index) :t new-time))))

(defn- lerp [a b t] (+ a (* (- b a) t)))
(defn- lerp-v [a b t] (mapv (fn [x y] (lerp x y t)) a b))

(defn eval-at
  "{:eye :target} at time `t` along `timeline` — linearly interpolated
  between the two keyframes bounding `t`; clamped to the first keyframe's
  values before it starts and the last keyframe's values after it ends.
  `nil` for an empty timeline (nothing to evaluate)."
  [timeline t]
  (when (seq timeline)
    (cond
      (<= t (:t (first timeline))) (select-keys (first timeline) [:eye :target])
      (>= t (:t (last timeline))) (select-keys (last timeline) [:eye :target])
      :else
      (let [[kf-a kf-b] (first (filter (fn [[a b]] (<= (:t a) t (:t b)))
                                        (partition 2 1 timeline)))
            span (- (:t kf-b) (:t kf-a))
            frac (if (zero? span) 0.0 (/ (- t (:t kf-a)) span))]
        {:eye (lerp-v (:eye kf-a) (:eye kf-b) frac)
         :target (lerp-v (:target kf-a) (:target kf-b) frac)}))))
