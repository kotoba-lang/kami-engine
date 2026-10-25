(ns skeleton-scene.clip
  "EDN-authored animation clips — EDN → `skeleton`'s `animation-clip`.

  Data-tier counterpart for skeletal *motion* (alongside
  `skeleton-scene`'s humanoid constraint table): a dance/idle/gesture
  clip authored as plain EDN, loaded into the clip `skeleton/evaluate`
  + `skeleton/evaluate-blend` play. Bone names resolve to indices via
  a caller-supplied map (VRM humanoid name -> skeleton index), so one
  clip retargets onto any skeleton. `skeleton` stays untouched — the
  EDN dependency lives only here (ADR-0038).

  Restored from the legacy kami-engine/kami-skeleton-scene Rust
  crate's `src/clip.rs` (deleted in kotoba-lang/kami-engine PR #82
  'Remove Rust workspace from kami-engine') as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root).

  ```edn
  {:name \"wave\" :duration 2.0 :loop true
   :tracks [{:bone \"rightUpperArm\" :interp :cubic
             :keys [{:t 0.0 :rot [0 0 0 1]} {:t 1.0 :rot [0 0 0.38 0.92]}]}]}
  ```

  Zero-dep portable CLJC."
  (:require [scene :as scene]
            [skeleton :as skeleton]))

(defn- opt-vec3
  "Read an optional 3-vector `v`; nil when absent or an empty vector."
  [v]
  (when (and (vector? v) (seq v))
    (scene/vec3 v)))

(defn- opt-quat
  "Read a quaternion `[x y z w]`; nil when absent or an empty vector.
  Missing trailing components default to `0.0`, except `w` which
  defaults to `1.0` (identity)."
  [v]
  (when (and (vector? v) (seq v))
    (let [g (fn [i default] (if (contains? v i) (scene/num (get v i)) default))]
      [(g 0 0.0) (g 1 0.0) (g 2 0.0) (g 3 1.0)])))

(defn- ident
  "Read an identifier string from a keyword (its bare name, no
  namespace) or a string; nil otherwise."
  [v]
  (cond
    (keyword? v) (name v)
    (string? v) v
    :else nil))

(defn clip-from-edn
  "Parse an EDN animation clip. `bone-index` maps a track's `:bone`
  name to a skeleton bone index; tracks whose bone doesn't resolve are
  dropped. Returns nil only if the top form isn't a map."
  [src bone-index]
  (when-let [root (scene/root-map src)]
    (let [name (let [n (scene/mget root "name")] (if (string? n) n "clip"))
          looping (boolean (scene/mget root "loop"))
          tracks-raw (let [t (scene/mget root "tracks")] (if (vector? t) t []))
          max-t (atom 0.0)
          tracks
          (into []
                (keep
                 (fn [t]
                   (when (map? t)
                     (when-let [bone (some-> (ident (scene/mget t "bone")) bone-index)]
                       (let [interp (if-let [n (ident (scene/mget t "interp"))]
                                      (skeleton/interpolation-by-name n)
                                      :linear)
                             keys-raw (let [k (scene/mget t "keys")] (if (vector? k) k []))
                             keyframes
                             (into []
                                   (keep
                                    (fn [k]
                                      (when (map? k)
                                        (let [time (scene/num (scene/mget k "t"))]
                                          (swap! max-t max time)
                                          (skeleton/keyframe
                                           {:time time
                                            :position (opt-vec3 (scene/mget k "pos"))
                                            :rotation (opt-quat (scene/mget k "rot"))
                                            :scale (opt-vec3 (scene/mget k "scale"))})))))
                                   keys-raw)]
                         (when (seq keyframes)
                           (skeleton/bone-track bone keyframes interp)))))))
                 tracks-raw)
          duration (let [d (scene/num (scene/mget root "duration"))]
                     (if (> d 0.0) d @max-t))]
      (skeleton/animation-clip {:name name :duration duration :tracks tracks :looping looping}))))
