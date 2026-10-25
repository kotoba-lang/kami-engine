(ns kotoba.amenominaka.gltf-export
  "M7 (ADR-2607100100): export a `kotoba.amenominaka.scene/compose` result
  to a binary glTF 2.0 (`.glb`) document, via `org-khronos-gltf`'s new
  multi-node scene builder (`gltf/export-glb-scene-byte-seq`, itself
  landed as part of this milestone since the repo previously only ever
  built a single-node/single-mesh document).

  Mirrors `kotoba.amenominaka.usd-export`'s exact sites -> buildings ->
  storeys -> elements tree walk and geometry-fidelity rule (real box mesh
  ONLY for an `:axis-sweep` element with a `:rectangle` profile — every
  other geometry kind is a plain transform-only node, glTF's own
  equivalent of USD's `Xform` placeholder — see `usd-export`'s
  `rectangle-axis-sweep?`/`element-kind-str`, reused here rather than
  re-implemented, so the two exporters can never silently drift apart on
  which elements get real geometry).

  Coordinate system: `bim`/`usd-export` are Z-up (USD's own convention,
  `scene->usda`'s `:upAxis :Z`) but glTF 2.0's spec REQUIRES Y-up — every
  position/normal/translation is converted via [[zup->yup]] (a
  determinant-+1 axis permutation, so face winding/handedness is
  preserved — no need to flip any triangle order).

  Material fidelity: same as `usd-export` — `bim` carries no per-material
  color/PBR data, so this uses `org-khronos-gltf`'s own default neutral-
  gray material (matching `kotoba.amenominaka.render-ir`'s
  `default-element-color`) rather than fabricating per-element materials.

  Environment presets (weather/vegetation/terrain/postfx): NOT carried
  into the glTF document. USD's export records them as custom `pr-str`'d
  string attrs on an `Environment` `Scope` prim — glTF's `extras`
  mechanism could carry equivalent data, but `org-khronos-gltf`'s node
  builder doesn't yet accept an `:extras` passthrough, and guessing at
  where to bolt one on risks a half-tested feature; left as a real,
  documented gap (same \"documented gap over guessed implementation\"
  stance `usd-export` already takes for its own MaterialX-binding gap)."
  (:require [kotoba.amenominaka.usd-export :as usd-export]
            [gltf]))

;; ── portable vector math (CLJC) ──

(defn- v- [a b] (mapv - a b))
(defn- cross [[ax ay az] [bx by bz]]
  [(- (* ay bz) (* az by)) (- (* az bx) (* ax bz)) (- (* ax by) (* ay bx))])
(defn- sqrt [x] #?(:clj (Math/sqrt x) :cljs (js/Math.sqrt x)))
(defn- v-length [[x y z]] (sqrt (+ (* x x) (* y y) (* z z))))
(defn- v-normalize [v]
  (let [l (v-length v)]
    (if (zero? l) [0.0 0.0 0.0] (mapv #(/ % l) v))))

(defn zup->yup
  "Z-up (bim/USD convention) -> Y-up (glTF's REQUIRED convention):
  `[x y z] -> [x z (- y)]`. A pure axis permutation + one sign flip,
  determinant +1 — preserves handedness/winding, so every triangle's
  face-winding computed in Z-up carries straight over correctly-oriented
  in Y-up with no index reordering needed."
  [[x y z]]
  [x z (- y)])

;; ── real box mesh (axis-sweep + rectangle profile) -> triangulated glTF mesh ──

(defn box-mesh->gltf-mesh
  "`usd-export/axis-sweep-rectangle->box-mesh`'s output (8 shared points,
  6 quad faces — fine for USD, which doesn't need per-vertex normals for
  a flat-shaded box) -> a real triangulated glTF mesh: 4 NEW vertices per
  face (24 total, not 8), each carrying that face's own flat normal (a
  correctly flat-shaded box, not an incorrectly vertex-normal-averaged
  one), 2 triangles per face (36 indices total). Converts every position/
  normal from Z-up to Y-up via [[zup->yup]].

  `axis-sweep-rectangle->box-mesh`'s own face-vertex order was verified
  (by hand-computing the cross product) to wind each face CW-from-outside,
  not CCW — harmless for USD (which never asserted on winding/normals,
  only on the raw point/index data), but glTF's default rasterization
  state expects CCW-from-outside front faces, and callers get real vertex
  normals here rather than none — so each face's vertex order is reversed
  before triangulating, giving both correct outward normals AND correct
  front-facing winding together, not just one or the other."
  [{:keys [points face-vertex-indices face-vertex-counts]}]
  (let [faces (loop [idxs face-vertex-indices counts face-vertex-counts acc []]
                (if (empty? counts)
                  acc
                  (let [c (first counts)]
                    (recur (drop c idxs) (rest counts) (conj acc (vec (take c idxs)))))))
        face-uvs [[0.0 0.0] [1.0 0.0] [1.0 1.0] [0.0 1.0]]
        {:keys [vertices indices vcount]}
        (reduce
         (fn [{:keys [vertices indices vcount]} face-idxs]
           (let [face-points (mapv #(zup->yup (nth points %)) (reverse face-idxs))
                 [p0 p1 p2 _p3] face-points
                 normal (v-normalize (cross (v- p1 p0) (v- p2 p0)))
                 new-verts (vec (mapcat (fn [p uv] (concat p normal uv)) face-points face-uvs))
                 base vcount
                 tri-indices [base (+ base 1) (+ base 2) base (+ base 2) (+ base 3)]]
             {:vertices (into vertices new-verts)
              :indices (into indices tri-indices)
              :vcount (+ vcount 4)}))
         {:vertices [] :indices [] :vcount 0}
         faces)]
    {:vertices vertices :indices indices :vertex-count vcount :index-count (count indices)}))

;; ── tree walk (mirrors usd-export/{element storey building site}->prim) ──

(defn- element->node [{:keys [id kind geometry]}]
  (let [node-name (str (usd-export/element-kind-str kind) "_" id)]
    (if (usd-export/rectangle-axis-sweep? geometry)
      {:name node-name :mesh (box-mesh->gltf-mesh (usd-export/axis-sweep-rectangle->box-mesh geometry))}
      {:name node-name})))

(defn- storey->node [{:keys [id elevation elements]}]
  (cond-> {:name (str "storey_" id) :children (mapv element->node elements)}
    (and elevation (not (zero? elevation)))
    (assoc :translation (zup->yup [0.0 0.0 (double elevation)]))))

(defn- building->node [{:keys [id storeys]}]
  {:name (str "building_" id) :children (mapv storey->node storeys)})

(defn- site->node [{:keys [id buildings]}]
  {:name (str "site_" id) :children (mapv building->node buildings)})

(defn scene->gltf-node-tree
  "The intermediate node tree (before glTF-JSON/GLB assembly) for a
  `kotoba.amenominaka.scene/compose` result — pure EDN, portable, and a
  useful unit-testable seam on its own (no byte-encoding involved).
  Throws `ex-info` on the same `:scene/building` shape check
  `usd-export/scene->usda` does."
  [scene]
  (let [building (:scene/building scene)]
    (when-not (and (map? building) (sequential? (:sites building)))
      (throw (ex-info "scene->gltf-node-tree: scene's :scene/building must be a bim/project-shaped map (a map with :sites)"
                       {:kotoba.amenominaka.gltf-export/error :invalid-scene})))
    {:name (or (:name building) "Project") :children (mapv site->node (:sites building))}))

(defn scene->glb-byte-seq
  "`scene->gltf-node-tree` -> a portable byte-int vector forming a
  complete binary glTF 2.0 (`.glb`) file, via `gltf/export-glb-scene-byte-seq`."
  [scene]
  (gltf/export-glb-scene-byte-seq [(scene->gltf-node-tree scene)]))

(defn scene->glb
  "`scene->glb-byte-seq` in the platform's native byte type (`byte[]` on
  the JVM, `js/Uint8Array` in cljs) — what a real download/save needs."
  [scene]
  (gltf/export-glb-scene [(scene->gltf-node-tree scene)]))
