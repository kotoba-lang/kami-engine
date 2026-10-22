(ns character-scene.face-rig
  "Face-rig data tier — `character`'s MetaHuman FACS face rig as
  parity-tested EDN. Restored from the legacy
  kami-engine/kami-character-scene Rust crate's `face_rig.rs` (deleted
  in kotoba-lang/kami-engine PR #82) as part of the clj-wgsl migration
  (ADR-2607010930, com-junkawasaki/root).

  `character.control-rig/metahuman-face-rig` builds its node graph
  from a flat table of FACS Action-Unit -> bone mappings (one
  control -> multiply -> rotation-axis node triple per row). That
  **description** is what moves to EDN here (ADR-0046 / ADR-0038);
  the rig **evaluation** (`character.control-rig/evaluate`) stays
  native. `face-rig-from-edn` rebuilds a rig in the *exact same* node
  shape as `character.control-rig/metahuman-face-rig`, asserted
  node-for-node `=` it in the test suite.

  ## EDN shape (see `resources/character_scene/face_rig.edn`)

  ```edn
  {:character/face-rig
   {:au-bones [{:au \"AU12_L\" :bone 34 :axis [0.0 0.0 1.0] :max-angle 0.3} ...]}}
  ```

  Depends on `kotoba-lang/scene` (tolerant EDN accessors) only —
  `character.control-rig` is a read-only oracle used by tests, not by
  this namespace's runtime logic."
  (:require [scene :as scene]))

;; --- shipped EDN ---------------------------------------------------------------------

(def face-rig-edn
  "The canonical FACS face-rig CONFIG shipped with this crate (the AU->bone
  table). Also copied verbatim to `resources/character_scene/face_rig.edn`."
  "{:character/face-rig
 {:au-bones
  [{:au \"AU43_L\" :bone 13 :axis [1.0 0.0 0.0] :max-angle -0.5}
   {:au \"AU43_R\" :bone 15 :axis [1.0 0.0 0.0] :max-angle -0.5}
   {:au \"AU7_L\"  :bone 14 :axis [1.0 0.0 0.0] :max-angle 0.3}
   {:au \"AU7_R\"  :bone 16 :axis [1.0 0.0 0.0] :max-angle 0.3}
   {:au \"AU1\"    :bone 17 :axis [1.0 0.0 0.0] :max-angle 0.3}
   {:au \"AU2_L\"  :bone 19 :axis [1.0 0.0 0.0] :max-angle 0.25}
   {:au \"AU2_R\"  :bone 22 :axis [1.0 0.0 0.0] :max-angle 0.25}
   {:au \"AU4\"    :bone 18 :axis [1.0 0.0 0.0] :max-angle -0.2}
   {:au \"AU26\"   :bone 8  :axis [1.0 0.0 0.0] :max-angle 0.4}
   {:au \"AU30\"   :bone 8  :axis [0.0 1.0 0.0] :max-angle 0.15}
   {:au \"AU12_L\" :bone 34 :axis [0.0 0.0 1.0] :max-angle 0.3}
   {:au \"AU12_R\" :bone 35 :axis [0.0 0.0 1.0] :max-angle 0.3}
   {:au \"AU15_L\" :bone 34 :axis [0.0 0.0 1.0] :max-angle -0.2}
   {:au \"AU15_R\" :bone 35 :axis [0.0 0.0 1.0] :max-angle -0.2}
   {:au \"AU38_L\" :bone 25 :axis [1.0 0.0 0.0] :max-angle 0.15}
   {:au \"AU38_R\" :bone 26 :axis [1.0 0.0 0.0] :max-angle 0.15}
   {:au \"AU6_L\"  :bone 36 :axis [1.0 0.0 0.0] :max-angle 0.2}
   {:au \"AU6_R\"  :bone 37 :axis [1.0 0.0 0.0] :max-angle 0.2}
   {:au \"AU19\"   :bone 44 :axis [1.0 0.0 0.0] :max-angle 0.5}]}}")

;; --- errors ----------------------------------------------------------------------------

(defn ->error
  "Error map: `:type` one of `:not-a-map` `:no-au-bones`."
  [type]
  {:type type})

;; --- AuBone row --------------------------------------------------------------------------

(defn au-bone-from-map
  "Read one FACS Action-Unit -> bone mapping row from its EDN map (tolerant:
  missing -> defaults, int<->float coercion). Mirrors `add-au-bone`'s input
  row."
  [m]
  {:au (or (scene/mget m "au") "")
   :bone (max 0 (long (or (scene/mget m "bone") 0)))
   :axis (scene/vec3 (scene/mget m "axis"))
   :max-angle (scene/num (scene/mget m "max-angle"))})

(defn au-bones-from-edn
  "Parse the `:character/face-rig` -> `:au-bones` table from EDN `src`.
  Returns `[:ok [AuBone ...]]` or `[:error error-map]`."
  [src]
  (if-let [root (scene/root-map src)]
    (let [rig (scene/mget root "character/face-rig")
          rows (when (map? rig) (scene/mget rig "au-bones"))]
      (if (vector? rows)
        [:ok (into [] (comp (filter map?) (map au-bone-from-map)) rows)]
        [:error (->error :no-au-bones)]))
    [:error (->error :not-a-map)]))

;; --- rig construction ------------------------------------------------------------------

(defn- add-au-bone
  "Append 3 nodes (control-input, multiply, rotation-axis) for one FACS
  AU -> bone mapping. Returns `[nodes' next-idx]`. Ported 1:1 from
  `character.control-rig/add-au-bone` (private there; duplicated here since
  this crate rebuilds the rig from EDN rows, not a literal au-specs vector)."
  [nodes idx {:keys [au bone axis max-angle]}]
  (let [control-idx idx
        nodes (conj nodes {:name (str "ctrl_" au)
                            :node-type {:type :control-input :control-name au}
                            :inputs [] :target-bone nil})
        idx (inc idx)
        mul-idx idx
        nodes (conj nodes {:name (str "mul_" au)
                            :node-type {:type :multiply :factor max-angle}
                            :inputs [[control-idx 0]] :target-bone nil})
        idx (inc idx)
        nodes (conj nodes {:name (str "rot_" au)
                            :node-type {:type :rotation-axis :axis axis}
                            :inputs [[mul-idx 0]] :target-bone bone})
        idx (inc idx)]
    [nodes idx]))

(defn build-face-rig
  "Build a real `character.control-rig` rig map from an AU->bone table `rows`,
  replaying the exact `metahuman-face-rig` expansion: each row -> a `ctrl_*`
  ControlInput node, a `mul_*` Multiply(max-angle) node, and a `rot_*`
  RotationAxis(axis) node targeting the bone, wired control -> multiply ->
  rotation. Node indexing matches the builtin so the graphs are identical."
  [rows]
  (let [[nodes _idx] (reduce (fn [[nodes idx] row] (add-au-bone nodes idx row))
                              [[] 0] rows)]
    {:nodes nodes :eval-order (vec (range (count nodes)))
     :controls {} :bone-outputs {}}))

(defn face-rig-from-edn
  "Load the face rig from EDN `src` into a `character.control-rig`-shaped
  rig map. Returns `[:ok rig]` or `[:error error-map]`."
  [src]
  (let [[status rows-or-err] (au-bones-from-edn src)]
    (if (= status :ok)
      [:ok (build-face-rig rows-or-err)]
      [:error rows-or-err])))

;; --- shipped convenience -----------------------------------------------------------------

(defn shipped-au-bones
  "Convenience: the AU->bone table loaded from the crate-shipped `face-rig-edn`."
  []
  (au-bones-from-edn face-rig-edn))

(defn shipped-face-rig
  "Convenience: the rig map built from the shipped `face-rig-edn`."
  []
  (face-rig-from-edn face-rig-edn))
