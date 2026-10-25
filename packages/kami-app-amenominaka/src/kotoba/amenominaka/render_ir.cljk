(ns kotoba.amenominaka.render-ir
  "M2 (ADR-2607100100): bridge a `kotoba.amenominaka.scene/compose` result
  into `kotoba-lang/webgpu`'s `kami.webgpu.ir` render-IR — the EDN a REAL,
  already-working browser WebGPU executor (`kami.webgpu/draw!`: camera +
  shadow-mapped directional light + PBR instancing, `.cljs`) consumes.
  This namespace is pure CLJC — it produces render-IR DATA, it does not
  touch WebGPU or the browser itself.

  IMPORTANT CORRECTION to ADR-2607100100 D5's premise: the render-IR/
  executor already in `kotoba-lang/webgpu` is NOT 2D/UI-only (that's
  `dom-gpu`, a different repo) — it's a real 3D camera/shadow/PBR
  instanced renderer, confirmed by reading `kami/webgpu/ir.cljc` in full.
  What was actually missing (closed by this namespace) is the EDN bridge
  from M0's building+environment scene to that IR's shape, not the
  renderer itself.

  Geometry: `ir/instance`'s box is SQUARE-FOOTPRINT only (`:size [w h]`
  = footprint-width × height — confirmed directly from `ir.cljc`'s own
  docstring and `mesh-from-spec`; there is no rectangular/length≠width
  box in the existing IR). Rather than guess at an unverified custom-
  `:geo`-registry extension, this namespace follows the SAME idiom
  `kami.webgpu.ir`'s own docstring already uses for
  `(ir/instance [0 0 0] [...] [2 5]) ;; a building` — each `bim`
  axis-sweep element renders as a coarse massing block (footprint = the
  sweep's run length, height = the profile's height where derivable) —
  a legitimate early-stage architectural massing representation, not
  mesh-accurate wall geometry. Elements with brep/mesh-ref/no-geometry
  carry no positional data in `bim`'s abstract `ElementGeometry`
  (confirmed in M1) and are not rendered — skipped, not faked.

  Vegetation/postfx: M0's scene EDN carries no placement data for
  vegetation and the render-IR has no `:postfx`/`:passes` consumer today
  — neither is bridged into instances; a real, documented gap, not
  fabricated geometry."
  (:require [kami.webgpu.ir :as ir]
            [atmosphere]))

;; ── portable vector math (CLJC) ──

(defn- sqrt [x] #?(:clj (Math/sqrt x) :cljs (js/Math.sqrt x)))
(defn- v- [a b] (mapv - a b))
(defn- v-length [[x y z]] (sqrt (+ (* x x) (* y y) (* z z))))
(defn- axis-midpoint [[p0 p1]] (mapv #(/ (+ %1 %2) 2.0) p0 p1))
(defn- axis-length [[p0 p1]] (v-length (v- p1 p0)))

(def default-element-color
  "The exact colour `kami.webgpu.ir`'s own docstring uses for its
  `;; a building` massing-block example — reused here for continuity
  since `bim` carries no material colour data to derive one from
  (confirmed in M1)."
  [0.62 0.60 0.66])

(defn- profile-height
  "The profile's vertical extent where derivable; a fixed 1.0 default for
  profile kinds with no directly comparable dimension (`:polygon`)."
  [{:keys [kind height diameter]}]
  (case kind
    :rectangle height
    :i-shape height
    :circle diameter
    1.0))

(defn element->instance
  "An `ir/instance` massing block for an `:axis-sweep` element, or nil for
  any other `ElementGeometry` kind (brep/mesh-ref/no-geometry carry no
  positional data in `bim` to place an instance at)."
  [{:keys [geometry]}]
  (when (= :axis-sweep (:kind geometry))
    (let [{:keys [axis profile]} geometry
          [mx my mz] (axis-midpoint axis)]
      (ir/instance [mx my mz] default-element-color
                   [(axis-length axis) (profile-height profile)]))))

(defn- storey->instances [{:keys [elevation elements]}]
  (keep (fn [el] (some-> (element->instance el)
                          (update-in [:pos 1] + (or elevation 0.0))))
        elements))

(defn- building->instances [{:keys [storeys]}] (mapcat storey->instances storeys))
(defn- site->instances [{:keys [buildings]}] (mapcat building->instances buildings))

(defn building->instances-all
  "Every `axis-sweep` element across a `bim/project`'s full Site->
  Building->Storey hierarchy, as `ir/instance` massing blocks (storey
  `:elevation` applied as an additive Y offset — the same convention M1's
  USD export uses for the storey Xform's translate)."
  [{:keys [sites]}]
  (vec (mapcat site->instances sites)))

(defn atmosphere->sky
  "An `ir/sky` map derived from a `:scene/atmosphere` (`atmosphere-scene`
  preset) map's `:day-night` sub-map, via the real `atmosphere/day-night-
  to-uniform` — fog-colour stands in for `ir/sky`'s `:horizon` (the
  SkyUniform has no separate ambient-sky field). Returns nil if `weather`
  has no `:day-night` (defensive — every `atmosphere-scene` shipped
  preset has one)."
  [weather]
  (when-let [day-night (:day-night weather)]
    (let [{:keys [sun-dir sun-color fog-color]} (atmosphere/day-night-to-uniform day-night)]
      (ir/sky fog-color sun-dir sun-color))))

(def default-sky
  "A reasonable default sky for a building-only scene composed with no
  `:weather` requested — not derived from any preset."
  (ir/sky [0.55 0.70 0.90] [0.3 0.7 0.6] [1.0 0.98 0.9]))

(def ground-plane-size
  "Footprint/height of the terrain ground-plane proxy instance — a flat
  slab, not a real heightfield mesh (`terrain-scene` provides FBM
  generator CONFIG, never a realized heightfield — confirmed in M1)."
  [200.0 0.2])

(defn terrain->ground-instance
  "A flat ground-plane `ir/instance` tinted by the biome's `[:palette
  :base]` first (grass/ground) layer colour — a real, renderable colour
  from the data, not a fabricated heightfield. Returns nil if `biome` has
  no base palette."
  [biome]
  (when-let [ground-color (first (get-in biome [:palette :base]))]
    (let [[_ h] ground-plane-size]
      (ir/instance [0.0 (- h) 0.0] ground-color ground-plane-size))))

(defn- instances-centroid-xz [instances]
  (if (seq instances)
    (let [n (count instances)]
      [(/ (reduce + (map (comp first :pos) instances)) n)
       (/ (reduce + (map #(nth (:pos %) 2) instances)) n)])
    [0.0 0.0]))

(defn scene->render-ir
  "Bridge a `kotoba.amenominaka.scene/compose` result to a
  `kami.webgpu.ir` render-IR frame — sky + camera + one massing-block
  instance per `axis-sweep` building element, plus a ground-plane
  instance if `:scene/terrain` was requested. Camera framing orbits the
  building instances' XZ centroid (`ir/rig->camera` with the default rig)
  — the ground plane is excluded from the centroid so it can't skew
  framing toward the world origin.

  Throws `ex-info` if `scene`'s `:scene/building` isn't a `bim/project`-
  shaped map (a map with `:sites`)."
  [scene]
  (let [building (:scene/building scene)]
    (when-not (and (map? building) (sequential? (:sites building)))
      (throw (ex-info "scene->render-ir: scene's :scene/building must be a bim/project-shaped map (a map with :sites)"
                       {:kotoba.amenominaka.render-ir/error :invalid-scene})))
    (let [building-instances (building->instances-all building)
          ground-instance (some-> (:scene/terrain scene) terrain->ground-instance)
          instances (cond-> building-instances ground-instance (conj ground-instance))
          sky-map (or (some-> (:scene/atmosphere scene) atmosphere->sky) default-sky)
          [cx cz] (instances-centroid-xz building-instances)
          {:keys [eye target]} (ir/rig->camera {} [cx cz])]
      (ir/render-ir sky-map instances eye target))))
