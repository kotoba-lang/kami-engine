(ns kotoba.amenominaka.usd-export
  "M1 (ADR-2607100100): export a `kotoba.amenominaka.scene/compose` result
  to a viewable `.usda` (USD ASCII) text document, via `kotoba-lang/usd`'s
  generic USDA-from-EDN emitter (`usd.core/usda`). No new rendering engine
  — this is a pure EDN -> text-format serialization step.

  Geometry fidelity: `bim`'s `ElementGeometry` (`:brep` / `:axis-sweep` /
  `:mesh-ref` / `:none`, as carried on every `bim/element`) has no bridge
  in `bim` itself to real triangle data (confirmed by reading `bim.cljc`
  in full — its separate `SceneGeom`/`triangles-geom` scene-projection
  system is not derived automatically from `ElementGeometry` by anything
  in `bim`). This namespace computes a real box mesh ONLY for the one case
  simple enough to do honestly without building a BREP tessellator: an
  `:axis-sweep` element with a `:rectangle` profile swept along a straight
  two-point axis (the common architectural case — a wall/beam extruded
  along a run with a rectangular cross-section; thickness is assumed
  horizontal-perpendicular to the axis, height straight up +Z). Every
  other geometry kind (brep, mesh-ref, non-rectangle axis-sweep profiles,
  no-geometry) exports as a plain `Xform` placeholder carrying only a
  `kotobaGeometryKind` string attr — a real limitation, not silently
  faked as a mesh.

  Environment presets (`:scene/atmosphere` `:scene/vegetation`
  `:scene/terrain` `:scene/postfx`) carry no placement/heightfield/
  instance data in M0's scene EDN (vegetation is a named-profile catalog
  with no positions, terrain is FBM generator CONFIG with no realized
  heightfield mesh) — so this namespace records each present domain as a
  `pr-str`'d custom string attr on an `Environment` `Scope` prim rather
  than fabricating geometry the data doesn't support.

  Material fidelity: `bim`'s `material-layer` carries a material NAME
  only, no color/roughness/PBR values — there's no real data to drive
  per-material MaterialX surfaces from. [[default-material-mtlx]] emits
  one neutral-gray `standard_surface` as a companion `.mtlx` document
  (satisfying ADR-2607100100 D5's `usd`+`materialx` pairing with a real,
  tested MaterialX artifact) — this namespace does NOT yet wire a
  verified USD<->MaterialX binding schema into the `.usda` stage itself
  (UsdMtlx `material:binding` conventions are nontrivial enough that
  guessing at them risks a binding that's silently wrong in a viewer;
  left as a real, documented gap rather than a guessed implementation)."
  (:require [kotoba.lang.text :as str]
            [usd.core :as usd]
            [materialx.core :as mx]))

;; ── portable vector math (CLJC: no JVM-only Math interop) ──

(defn- v- [a b] (mapv - a b))
(defn- v+ [a b] (mapv + a b))
(defn- v-scale [a s] (mapv #(* % s) a))

(defn- sqrt [x] #?(:clj (Math/sqrt x) :cljs (js/Math.sqrt x)))

(defn- v-length [[x y z]] (sqrt (+ (* x x) (* y y) (* z z))))

(defn- v-normalize [v]
  (let [l (v-length v)]
    (if (zero? l) [0.0 0.0 0.0] (v-scale v (/ 1.0 l)))))

;; ── geometry: axis-sweep + rectangle profile -> box mesh ──

(defn axis-sweep-rectangle->box-mesh
  "Best-effort box mesh for an `axis-sweep-geometry` whose `:profile` is a
  `rectangle-profile`. `:axis` is read as a straight two-point run
  `[p0 p1]` (extra points beyond the first two are ignored — no curved
  sweeps). `:thickness` extends horizontally perpendicular to the axis,
  `:height` extends straight up (+Z) from the axis. A simplifying
  assumption for the common architectural case, not general BREP sweep
  semantics.

  Returns `{:points [...] :face-vertex-indices [...] :face-vertex-counts [...]}`
  — 8 vertices, 6 quad faces (bottom/top/2 end-caps/2 sides)."
  [{:keys [axis profile]}]
  (let [[p0 p1] axis
        {:keys [thickness height]} profile
        dir (v-normalize (v- p1 p0))
        perp (v-normalize [(- (get dir 1)) (get dir 0) 0.0])
        half (v-scale perp (/ thickness 2.0))
        up [0.0 0.0 height]
        b0l (v- p0 half) b0r (v+ p0 half)
        b1l (v- p1 half) b1r (v+ p1 half)
        t0l (v+ b0l up)  t0r (v+ b0r up)
        t1l (v+ b1l up)  t1r (v+ b1r up)
        points [b0l b0r b1r b1l t0l t0r t1r t1l]
        ;; 0=b0l 1=b0r 2=b1r 3=b1l 4=t0l 5=t0r 6=t1r 7=t1l
        faces [[0 3 2 1]                                  ;; bottom
               [4 5 6 7]                                  ;; top
               [0 1 5 4]                                  ;; end at p0
               [2 3 7 6]                                  ;; end at p1
               [3 0 4 7]                                  ;; left side
               [1 2 6 5]]]                                ;; right side
    {:points points
     :face-vertex-indices (vec (mapcat identity faces))
     :face-vertex-counts (vec (repeat (count faces) 4))}))

(defn rectangle-axis-sweep?
  "True if `geometry` is an `:axis-sweep` element with a `:rectangle`
  profile — the one case [[axis-sweep-rectangle->box-mesh]] can compute a
  real box mesh for. Public: also used by `kotoba.amenominaka.gltf-export`
  (M7) to decide the identical real-mesh-vs-placeholder-node split for
  glTF, keeping the geometry-fidelity rule defined in exactly one place."
  [geometry]
  (and (= :axis-sweep (:kind geometry))
       (= :rectangle (get-in geometry [:profile :kind]))))

(defn element-kind-str
  "`:wall`/`\"wall\"`/nil -> `\"wall\"`/`\"wall\"`/`\"element\"`. Public: shared
  naming convention with `kotoba.amenominaka.gltf-export` (M7)."
  [kind]
  (cond (keyword? kind) (name kind) (some? kind) (str kind) :else "element"))

(defn- element->prim [{:keys [id kind geometry]}]
  (let [prim-name (str (element-kind-str kind) "_" id)]
    (if (rectangle-axis-sweep? geometry)
      (let [{:keys [points face-vertex-indices face-vertex-counts]}
            (axis-sweep-rectangle->box-mesh geometry)]
        [:def "Mesh" prim-name
         [:attr "point3f[]" :points points]
         [:attr "int[]" :faceVertexIndices (into [:array] face-vertex-indices)]
         [:attr "int[]" :faceVertexCounts (into [:array] face-vertex-counts)]])
      [:def "Xform" prim-name
       [:attr "string" :kotobaGeometryKind (element-kind-str (:kind geometry))]])))

(defn- storey->prim [{:keys [id elevation elements]}]
  (let [translate-attrs (when (and elevation (not (zero? elevation)))
                           [[:attr "double3" "xformOp:translate" [0.0 0.0 (double elevation)]]
                            [:attr "uniform token[]" "xformOpOrder" [:array "xformOp:translate"]]])]
    (into [:def "Xform" (str "storey_" id)] (concat translate-attrs (mapv element->prim elements)))))

(defn- building->prim [{:keys [id storeys]}]
  (into [:def "Xform" (str "building_" id)] (mapv storey->prim storeys)))

(defn- site->prim [{:keys [id buildings]}]
  (into [:def "Xform" (str "site_" id)] (mapv building->prim buildings)))

(defn- sanitize-prim-name
  "USD prim names are identifiers, not free text — replace whitespace/
  punctuation runs with `_`. Falls back to `Project` for a blank/nil name."
  [s]
  (let [cleaned (some-> s str/trim (str/replace #"[^A-Za-z0-9_]+" "_"))]
    (if (str/blank? cleaned) "Project" cleaned)))

(defn- environment-prim
  "An `Environment` Scope prim carrying whichever `:scene/atmosphere`
  `:scene/vegetation` `:scene/terrain` `:scene/postfx` are present, each
  as a `pr-str`'d custom string attr — exact data, not a curated/renamed
  view (M0's scene EDN doesn't reliably carry a preset name string across
  all four domains, see ADR-2607100100 M1 notes)."
  [{:scene/keys [atmosphere vegetation terrain postfx]}]
  (into [:def "Scope" "Environment"]
        (remove nil?
                [(when atmosphere [:attr "string" :kotobaAtmosphere (pr-str atmosphere)])
                 (when (seq vegetation) [:attr "string" :kotobaVegetation (pr-str vegetation)])
                 (when terrain [:attr "string" :kotobaTerrain (pr-str terrain)])
                 (when postfx [:attr "string" :kotobaPostfx (pr-str postfx)])])))

(defn- environment-present? [{:scene/keys [atmosphere vegetation terrain postfx]}]
  (boolean (or atmosphere (seq vegetation) terrain postfx)))

(defn scene->usda
  "Export a `kotoba.amenominaka.scene/compose` result (a map with
  `:scene/building`, a `bim/project` map, plus whichever
  `:scene/atmosphere` `:scene/vegetation` `:scene/terrain` `:scene/postfx`
  were requested) to `.usda` ASCII text.

  Throws `ex-info` if `scene` doesn't carry a `bim/project`-shaped
  `:scene/building` (a map with a `:sites` collection) — this namespace
  doesn't re-implement `kotoba.amenominaka.scene/valid-scene?`, it just
  checks the one field it actually reads."
  [scene]
  (let [building (:scene/building scene)]
    (when-not (and (map? building) (sequential? (:sites building)))
      (throw (ex-info "scene->usda: scene's :scene/building must be a bim/project-shaped map (a map with :sites)"
                       {:kotoba.amenominaka.usd-export/error :invalid-scene})))
    (let [project-name (sanitize-prim-name (:name building))
          project-prim (into [:def "Xform" project-name] (mapv site->prim (:sites building)))
          top-prims (cond-> [project-prim]
                      (environment-present? scene) (conj (environment-prim scene)))]
      (apply usd/usda {:defaultPrim project-name :upAxis :Z} top-prims))))

(defn default-material-mtlx
  "A minimal neutral-gray `standard_surface` MaterialX document, via
  `materialx.core`. A companion `.mtlx` asset — see the namespace
  docstring's \"Material fidelity\" note for why this isn't bound into
  the `.usda` stage yet."
  []
  (mx/materialx {}
                [:standard_surface {:name "default_surface" :type "surfaceshader"}
                 [:input {:name "base_color" :type "color3" :value (mx/value [0.6 0.6 0.6])}]
                 [:input {:name "roughness" :type "float" :value "0.6"}]]
                [:surfacematerial {:name "DefaultMaterial" :type "material"}
                 [:input {:name "surfaceshader" :type "surfaceshader" :nodename "default_surface"}]]))
