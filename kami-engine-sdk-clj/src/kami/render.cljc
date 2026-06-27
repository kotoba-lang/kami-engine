(ns kami.render
  "L2 — render-IR builder. Queries the in-memory ECS and produces one frame of
  render-IR: a plain-data, retained-by-id / immediate-by-frame draw description
  (ARCHITECTURE.md §7). The renderer (`kami-render`, Rust/wgpu) is a dumb executor
  of this IR; instancing is the default, not an optimization."
  (:require [kami.ecs  :as ecs]
            [kami.math :as m]))

(def nintendo-cream
  "Default clear color #f0ead6 (KAMI Engine prohibits dark themes; see §14)."
  [0.94 0.917 0.839 1.0])

(def ^:private default-rot [0.0 0.0 0.0 1.0])
(def ^:private default-scale [1.0 1.0 1.0])

(defn- model-of
  "Column-major model matrix from an entity's TRS components (with defaults)."
  [e]
  (m/from-trs (:transform/translation e [0.0 0.0 0.0])
              (:transform/rotation e default-rot)
              (:transform/scale e default-scale)))

(defn- asset-id
  "Resolve a mesh/material/shader ref to its string/uuid id."
  [r]
  (cond (map? r) (:asset/id r) (vector? r) (second r) :else r))

(defn camera-ir
  "Build {:view <f32×16> :proj <f32×16>} from the active camera entity in `world`
  (the one with :camera/active? true). View = inverse of the camera's world
  transform; proj from fov/near/far + `aspect`."
  [world aspect]
  (let [[_ cam] (first (ecs/query world #{:camera/active?}))]
    (when-not cam
      (throw (ex-info "camera-ir: no entity with :camera/active? true" {})))
    {:view (m/invert-rigid (model-of cam))
     :proj (m/perspective (:camera/fov cam 60.0) aspect
                          (:camera/near cam 0.1) (:camera/far cam 1000.0))}))

(def ^:private builtin-pipelines
  #{:pbr :sky :terrain :vegetation :character :water :voxel :particle :atlas})

(defn- pipeline-of
  "Choose the pipeline for an entity: explicit :shader/asset id → that registered
  pipeline; otherwise the default built-in :pbr."
  [e]
  (if-let [s (:shader/asset e)] (asset-id s) :pbr))

(defn merge-instances
  "Group renderable entities (those carrying a :mesh/asset) sharing
  (pipeline, mesh, material) into a single instanced draw. Returns a seq of
  :draw maps whose :draw/instances carries one flattened model-matrix array
  (→ a KAMI Dtype/Mat4 column in `kami.ipc`) plus a flattened tint array."
  [world]
  ;; Sort renderables by eid so both the group order (sorted by key below) AND the
  ;; per-instance order within a group are deterministic — `ecs/query` walks a set
  ;; intersection whose iteration order is unspecified. Determinism makes
  ;; `kami.ipc/pack` output byte-reproducible (the record/replay / golden surface).
  (let [renderable (sort-by #(str (:kami/eid %))
                            (map second (ecs/query world #{:mesh/asset})))
        groups (group-by (juxt pipeline-of
                               #(asset-id (:mesh/asset %))
                               #(asset-id (:material/asset %)))
                         renderable)]
    ;; NOTE: :tint is assembled per-instance from each entity's :material/tint RGBA
    ;; (default opaque white). `kami.ipc/pack` emits it as a per-draw f16 column only
    ;; under the opt-in v2 layout `(pack frame {:tint? true})`; the default v1 layout
    ;; (camera-mat4 + per-draw model-mat4, the contract the kami-clj-host Rust
    ;; decoder/fixture pins) omits it, so default output stays byte-identical.
    (for [[[pipeline mesh material] ents] (sort-by (comp str first) groups)
          :let [models (vec (mapcat model-of ents))
                tints  (vec (mapcat #(:material/tint % [1.0 1.0 1.0 1.0]) ents))]]
      {:draw/pipeline pipeline
       :draw/mesh     mesh
       :draw/material material
       :draw/instances {:count (count ents)
                        :model models
                        :tint  tints}})))

(defn draws-for
  "Build the draw-list for one render pass. Currently the single :main pass holds
  every instanced draw; multi-pass routing (shadow, postfx) is future work."
  [world pass-id]
  (case pass-id
    :main (vec (merge-instances world))
    []))

(defn frame
  "Assemble one full render-IR frame map (§7):
     {:frame/n n :frame/clear [...] :frame/camera {...} :frame/passes [...]}
  Pure given the ECS world. Serializable — the golden-test / record-replay
  surface. Hand to `kami.ipc/pack` then `kami.gpu/submit!`."
  [world {:keys [n aspect clear]
          :or   {n 0 aspect 1.7777778 clear nintendo-cream}}]
  {:frame/n      n
   :frame/clear  clear
   :frame/camera (camera-ir world aspect)
   :frame/passes [{:pass/id     :main
                   :pass/target :swapchain
                   :pass/draws  (draws-for world :main)}]})
