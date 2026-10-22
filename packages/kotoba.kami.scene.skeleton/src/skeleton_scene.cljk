(ns skeleton-scene
  "KAMI Skeleton Scene — EDN authoring surface for `skeleton`'s default
  humanoid joint-constraint table (the anatomical Euler-angle rotation
  limits, ADR-0040 \"animation: constraints / retarget maps as EDN\"),
  plus EDN-authored animation clips ([[skeleton-scene.clip]]) and MMD
  `.vmd`/`.pmx` motion & mesh import ([[skeleton-scene.vmd]] /
  [[skeleton-scene.pmx]]).

  The data-tier counterpart of `kami-vehicle-scene` / `kami-input-
  scene` for the skeletal-animation system: it turns canonical
  `:skeleton/humanoid-constraints` EDN (an *ordered* table of
  `[joint-name {:min-deg [..] :max-deg [..]}]` pairs) into the real
  `skeleton/joint-constraint` engine struct, the same way the
  hardcoded `skeleton/default-humanoid-constraints` table builds it.
  It re-uses the tolerant `scene` accessors the same way games parse
  `scene.edn` (namespaced keywords match on `ns/name`, malformed
  entries skip).

  Restored from the legacy kami-engine/kami-skeleton-scene Rust crate
  (`src/lib.rs`, `src/clip.rs`, `src/vmd.rs`, `src/pmx.rs`; deleted in
  kotoba-lang/kami-engine PR #82 'Remove Rust workspace from
  kami-engine') as part of the clj-wgsl migration (ADR-2607010930,
  com-junkawasaki/root).

  ## Why this is safe (ADR-0038)

  Hot skeletal evaluation (`skeleton/evaluate`, `evaluate-constrained`,
  `solve-ik-ccd`, `constraint-clamp`) stays pure CLJC in
  `kotoba-lang/skeleton`. The constraint *table* — which bone has
  which Euler-angle limit — is **init-time CONFIG** read once when an
  app builds its constraint index, so it is safe to move to EDN.
  `skeleton` itself stays untouched; the EDN dependency lives only
  here. The compiled-in `skeleton/default-humanoid-constraints`
  remains the [[builtin-humanoid-constraints]] fallback and is
  parity-tested against the shipped EDN ([[humanoid-edn]]).

  ## Degrees in EDN, radians at load

  The Rust source authored each limit in degrees and converted with
  `let d = std::f32::consts::PI / 180.0;` then `<deg> * d` — bit-for-
  bit-parity-tested against the same conversion inlined in
  `skeleton/default-humanoid-constraints`. This CLJC port stores the
  *same* degree literals in EDN (`:min-deg`/`:max-deg`) and converts
  with the *identical* `(/ Math/PI 180.0)` expression
  ([[deg-to-rad]]) `skeleton/default-humanoid-constraints` itself
  uses, so the two are exactly (`=`) equal in Clojure's double
  arithmetic — the CLJC parity contract for [[humanoid-constraints-
  from-edn]] vs [[builtin-humanoid-constraints]].

  ## EDN shape (see `resources/humanoid.edn`)

  ```edn
  {:skeleton/humanoid-constraints
   [[\"head\" {:min-deg [-60.0 -80.0 -40.0] :max-deg [60.0 80.0 40.0]}]
    [\"neck\" {:min-deg [-30.0 -45.0 -30.0] :max-deg [30.0 45.0 30.0]}]
    ... all 13, IN ORDER ...]}
  ```

  The table is an **ordered** vector of `[joint-name {…}]` pairs
  (order matters: it mirrors the `default-humanoid-constraints`
  declaration order). The joint-name is a string that round-trips
  exactly (`\"leftUpperArm\"`); the limits are `:min-deg` / `:max-deg`,
  each a `[x y z]` 3-vector of degrees.

  Depends on `kotoba-lang/scene` (tolerant EDN accessors) and
  `kotoba-lang/skeleton` (`joint-constraint` / `default-humanoid-
  constraints` / the animation data shapes `clip`/`vmd`/`pmx` build).
  Zero additional runtime dependencies beyond those two sibling CLJC
  crates."
  (:require [scene :as scene]
            [skeleton :as skeleton]
            [skeleton-scene.clip :as clip]
            [skeleton-scene.vmd :as vmd]
            [skeleton-scene.pmx :as pmx]))

;; ── Shipped EDN config ───────────────────────────────────────────

(def humanoid-edn
  "The canonical default humanoid joint-constraint table shipped with
  this crate. This is the source of truth; the compiled-in
  `skeleton/default-humanoid-constraints` table is the parity-tested
  mirror. CLJC-portable equivalent of the original's
  `include_str!(\"../data/humanoid.edn\")`. The byte-identical source
  also ships at `resources/humanoid.edn` for provenance/tooling."
  ";; Canonical default humanoid joint-constraint table for kami-skeleton
;; (ADR-0040 data tier: \"animation: constraints / retarget maps as EDN\").
;;
;; This mirrors `kami_skeleton::default_humanoid_constraints()` — the per-bone
;; anatomical Euler-angle rotation limits (VRM humanoid bones, derived from
;; orthopedic range-of-motion references). It is an ORDERED vector of
;; `[joint-name {:min-deg [x y z] :max-deg [x y z]}]` pairs — order matters and
;; is preserved (head → neck → spine → … → rightLowerLeg).
;;
;; Limits are authored in DEGREES (readable, matches the Rust source which writes
;; e.g. `60.0 * d`). The loader multiplies each degree value by
;; `std::f32::consts::PI / 180.0` — the SAME factor the Rust uses — so the f32
;; result is bit-for-bit identical to `default_humanoid_constraints()`. The axis
;; order is [x, y, z]. These are parity-tested == the real Rust (every joint,
;; every min/max [f32;3], exact f32 ==) in tests/humanoid_parity.rs.
;;
;; Joint names round-trip EXACTLY (camelCase preserved, e.g. \"leftUpperArm\").

{:skeleton/humanoid-constraints
 [[\"head\"          {:min-deg [-60.0 -80.0 -40.0]   :max-deg [60.0 80.0 40.0]}]
  [\"neck\"          {:min-deg [-30.0 -45.0 -30.0]   :max-deg [30.0 45.0 30.0]}]
  [\"spine\"         {:min-deg [-30.0 -30.0 -20.0]   :max-deg [30.0 30.0 20.0]}]
  [\"chest\"         {:min-deg [-15.0 -15.0 -10.0]   :max-deg [15.0 15.0 10.0]}]
  [\"hips\"          {:min-deg [-30.0 -30.0 -15.0]   :max-deg [30.0 30.0 15.0]}]
  [\"leftUpperArm\"  {:min-deg [-60.0 -45.0 -30.0]   :max-deg [90.0 90.0 180.0]}]
  [\"rightUpperArm\" {:min-deg [-60.0 -90.0 -180.0]  :max-deg [90.0 45.0 30.0]}]
  [\"leftLowerArm\"  {:min-deg [-5.0 0.0 -5.0]       :max-deg [5.0 145.0 5.0]}]
  [\"rightLowerArm\" {:min-deg [-5.0 -145.0 -5.0]    :max-deg [5.0 0.0 5.0]}]
  [\"leftUpperLeg\"  {:min-deg [-30.0 -45.0 -20.0]   :max-deg [120.0 30.0 45.0]}]
  [\"rightUpperLeg\" {:min-deg [-30.0 -30.0 -45.0]   :max-deg [120.0 45.0 20.0]}]
  [\"leftLowerLeg\"  {:min-deg [-140.0 -5.0 -5.0]    :max-deg [0.0 5.0 5.0]}]
  [\"rightLowerLeg\" {:min-deg [-140.0 -5.0 -5.0]    :max-deg [0.0 5.0 5.0]}]]}\n")

(def deg-to-rad
  "Degrees -> radians factor. The same `(/ Math/PI 180.0)` expression
  `skeleton/default-humanoid-constraints` uses, so multiplying an EDN
  degree literal by this reproduces its `<deg> * d` result exactly (in
  Clojure's double arithmetic)."
  (/ Math/PI 180.0))

(def all-joint-names
  "Joint names in the order they are declared in
  `skeleton/default-humanoid-constraints` (also the order shipped in
  `humanoid-edn`). Iteration source for `builtin`/parity; kept here
  (not in `skeleton`) so the domain namespace stays untouched."
  ["head" "neck" "spine" "chest" "hips"
   "leftUpperArm" "rightUpperArm" "leftLowerArm" "rightLowerArm"
   "leftUpperLeg" "rightUpperLeg" "leftLowerLeg" "rightLowerLeg"])

;; ── Errors ───────────────────────────────────────────────────────
;; Rust's `#[derive(thiserror::Error)]` `Error` enum, ported as
;; `ex-info` constructors distinguished by `:type` in ex-data.

(defn ex-not-a-map
  "The EDN source did not parse to a top-level map."
  []
  (ex-info "humanoid-constraints EDN root is not a map" {:type ::not-a-map}))

(defn ex-no-table
  "The `:skeleton/humanoid-constraints` table was missing or not a
  vector."
  []
  (ex-info "`:skeleton/humanoid-constraints` missing or not a vector" {:type ::no-table}))

;; ── Constraint loading ───────────────────────────────────────────

(defn- deg-vec3->rad
  "Read a `[x y z]` degree 3-vector and convert to radians with the
  SAME arithmetic `skeleton/default-humanoid-constraints` uses
  (`<deg> * d`). `scene/vec3` coerces int<->float and pads short
  vectors with `0.0`."
  [v]
  (mapv #(* % deg-to-rad) (scene/vec3 v)))

(defn constraint-from-map
  "Build one real `joint-constraint` from its EDN map (`{:min-deg [..]
  :max-deg [..]}`). Degrees are read and converted to radians with the
  same multiply `skeleton/default-humanoid-constraints` uses."
  [m]
  (skeleton/joint-constraint (deg-vec3->rad (scene/mget m "min-deg"))
                              (deg-vec3->rad (scene/mget m "max-deg"))))

(defn builtin-humanoid-constraints
  "The compiled-in fallback / parity oracle: the real
  `skeleton/default-humanoid-constraints`. This is what the shipped
  EDN is parity-tested against."
  []
  (skeleton/default-humanoid-constraints))

(defn humanoid-constraints-from-edn
  "Parse the whole `:skeleton/humanoid-constraints` table from EDN
  `src` into an **ordered** vector of `[joint-name joint-constraint]`
  pairs (order preserved from the vector). A pair malformed in *shape*
  (not a `[name {..}]` 2-vector, non-string name, non-map limits) is
  skipped, matching how the rest of the data tier degrades on shape
  errors."
  [src]
  (let [root (or (scene/root-map src) (throw (ex-not-a-map)))
        table (scene/mget root "skeleton/humanoid-constraints")]
    (when-not (vector? table) (throw (ex-no-table)))
    (into []
          (keep (fn [pair]
                  (when (and (vector? pair) (= 2 (count pair)))
                    (let [[name-v limits-v] pair]
                      (when (and (string? name-v) (map? limits-v))
                        [name-v (constraint-from-map limits-v)])))))
          table)))

(defn shipped-humanoid-constraints
  "Convenience: load & rebuild the humanoid constraint table from the
  shipped [[humanoid-edn]]."
  []
  (humanoid-constraints-from-edn humanoid-edn))

(defn limits-eq
  "Compare two `[min max min]`-shaped 3-vectors for equality. Kept for
  parity with the original's `limits_eq` (which compared `[f32;3]`
  arrays because `JointConstraint` derived no `PartialEq`); in CLJC,
  plain `=` already does this, so this is a thin, self-documenting
  alias used by the parity tests."
  [a b]
  (= a b))

;; ── Re-exports (mirroring the original's `pub use`) ──────────────

(def clip-from-edn
  "EDN-authored animation clips (`:tracks` -> `skeleton/animation-
  clip`). See `skeleton-scene.clip`."
  clip/clip-from-edn)

(def mmd-bone-to-humanoid
  "MMD bone name -> VRM humanoid bone name. See `skeleton-scene.vmd`."
  vmd/mmd-bone-to-humanoid)

(def vmd-to-clip
  "MMD `.vmd` motion import (-> `skeleton/animation-clip`). See
  `skeleton-scene.vmd`."
  vmd/vmd-to-clip)

(def pmx-to-model
  "MMD `.pmx` model import (mesh -> `pmx-model`). See
  `skeleton-scene.pmx`."
  pmx/pmx-to-model)

(def pmx-to-skeleton
  "MMD `.pmx` model bones -> `skeleton/skeleton`. See
  `skeleton-scene.pmx`."
  pmx/pmx-to-skeleton)
