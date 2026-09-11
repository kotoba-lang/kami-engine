# kami-render

KAMI's backend shader assets and backend-neutral render-style contract.

## Reusable visual profiles

Scene authors select art-direction defaults with `:render/style`. This is not a
replacement for each material's `:model`; mixed surfaces remain valid.

```edn
{:render/style
 {:contract :kotoba.render/style-v1
  :profile :stylized
  :shading {:model :toon-pbr :bands 3 :threshold 0.46 :smoothness 0.06}
  :outline {:mode :screen-space :width-px 1.5
            :color [0.08 0.09 0.12 1.0]
            :depth-threshold 0.1 :normal-threshold 0.2}
  :color-grading {:tone-map :aces :saturation 1.08 :contrast 1.06}}
 :materials
 [{:id :hero :model :mtoon :base [0.8 0.4 0.2]}
  {:id :blade :model :pbr :metallic 0.9 :roughness 0.15}]}
```

Use `kami.render.style/scene-style` to validate the contract and
`pipeline-plan` to derive shader/pass intent. The two built-in profiles are:

- `:stylized`: MToon/skinned-MToon defaults, toon-PBR bands, screen-space
  outline through `kami-postfx`, and a stylized ACES grade.
- `:photoreal`: PBR defaults, no outline, and a neutral ACES grade.

The current executable outline mode is `:screen-space`. `:inverted-hull` is a
reserved contract value and deliberately fails validation until its geometry
pass exists. A renderer must never silently remove a requested visual feature.

Legacy MToon material `:outline 0.02` remains accepted as a per-material
`:outline-hint`; the scene-wide outline pass is controlled only by
`:render/style :outline`.

## GPU execution ABI

`shaders/style_postfx.wgsl` executes outline, ACES tone mapping, saturation,
and contrast in one fullscreen-triangle pass. `postfx-execution` produces its
portable bind-group and draw contract. Group 0 is stable across both profiles:

| Binding | Resource | Exact semantic |
|---:|---|---|
| 0 | `texture_2d<f32>` | resolved, single-sample, linear-HDR scene color |
| 1 | `texture_depth_2d` | WebGPU device depth `[0,1]`, clear value `1` |
| 2 | `texture_2d<f32>` | unit normal encoded `normal * 0.5 + 0.5`; zero means background |
| 3 | filtering sampler | scene-color sampling only |
| 4 | 64-byte uniform | `:kami.render/style-postfx-v1` |

Depth threshold is an absolute difference in device-depth units. Normal
threshold is `1 - dot(normalA, normalB)`. Outline width is rounded to
an integer radius and clamped to 1–8 pixels. The pass outputs display-linear
color to an sRGB target; hosts must not apply a second manual gamma transform.

The host declares `:normal-space :world` or `:view` in the execution resources.
Every pixel in an attachment must use the same space. Edge detection uses only
normal dot products, so either declared space produces the same result under a
rigid camera transform.

Both stylized and photoreal bind all resources. Photoreal sets
`outline-enabled = 0`, avoiding pipeline-layout variants while retaining ACES
and color grading. Upstream MSAA targets must be resolved before this pass.

Run the contract tests with `kbb -M:test` from this directory.

## Stylized character library

`kami.render.character-preset/resolve-character` composes reusable silhouette,
MToon material-role, outline, variation and LOD policies:

```clojure
(resolve-character
 {:family :stylized
  :preset :hero-balanced
  :silhouette-tier :hero
  :palette {:cloth [0.8 0.2 0.1 1.0]}})
```

Built-in presets are `:hero-balanced`, `:combat-readable`, and
`:crowd-efficient`; silhouette tiers are `:hero`, `:gameplay`, and `:crowd`.
Every result has stable `:skin`, `:cloth`, and `:metal` roles using the shared
`:kotoba.render/material-preset-v1` envelope. Each role contains MToon
base/shade, shade shift/tooniness, rim, highlight, metallic/roughness,
role-specific outline participation, deterministic variation, and the selected
silhouette's LOD policy.

The sibling `:photoreal` family and its PBR model are reserved in
`family-boundary`, but deliberately fail resolution until measured photoreal
presets exist. Its semantic roles and resolver envelope are already fixed, so
games will not need different asset slots or role ids when it is implemented.

### Equipment and silhouette detail kit

`kami.render.equipment-kit/resolve-kit` adds portable semantic mesh intents for
helmet, visor, left/right shoulder armour, chest armour, backpack, belt, and a
primary weapon. Parts attach to stable humanoid semantics such as
`:humanoid/head`, `:humanoid/chest`, and `:humanoid/right-hand`; the weapon also
declares `:weapon/grip-primary`.

```clojure
(resolve-kit {:family :stylized
              :tier :gameplay
              :entity-id :operator/alpha
              :character-preset :combat-readable})
```

The resolved `:meshes` reuse render-IR fields `:id`, `:skin`, and `:material`
while `:mesh-semantic` remains an asset-registry lookup rather than a fake URL.
Hero/gameplay/crowd each select explicit LOD and triangle budgets. Culled parts
remain in `:parts` with `:enabled? false` and a reason, providing auditable LOD
evidence. Variation uses a portable stable seed derived from entity and part
IDs. Every part carries its material role and outline policy.

The photoreal sibling reserves the same part, attachment, mesh and material-role
API but fails resolution until its assets and presets are genuinely available.

### Executor material lowering

`kami.render.character-material/lower-library` converts
`:kotoba.render/material-preset-v1` character envelopes into
`:kotoba.render/portable-material-v1` records. KAMI owns these semantic roles:

`skin`, `cloth`, `metal`, `visor`, `emissive`, `accent`, and `weapon`.

Each output contains base color, toon shade color/shift/tooniness,
metallic/roughness, emissive color/intensity, highlight, rim and outline. Its
`:executor :uniforms` directly matches the existing MToon uniform semantics:
`albedo`, `subsurface-color`, `sss-r0/r1/r2`, `hair-scatter`, and RGB emission.
Consequently each role differs in its complete shader response, not merely its
palette color.

```clojure
(lower-library character
               {:team-palette {:cloth [0.1 0.7 0.2 1.0]
                               :accent [0.95 0.8 0.1 1.0]}})
```

Team palettes may override only `cloth` and `accent`; skin, metal, visor,
emissive and weapon overrides fail validation. The equipment kit automatically
selects only materials referenced by enabled hero/gameplay/crowd parts for its
`:material-registry`. Photoreal uses the same API boundary but is rejected until
its PBR lowering is implemented.

### Actual stylized weapon meshes

`kami.render.weapon-mesh/resolve-weapon` generates an actual indexed rifle,
not a placeholder stick. It reuses `kotoba.render.mesh` primitives and combines
receiver, barrel, stock, grip, magazine, optic, muzzle, and handguard components
into flat `positions/normals/uvs/indices` arrays with contiguous material
ranges. Material roles are `weapon`, `accent`, and `emissive` and resolve through
the executor material library above.

Hero and gameplay retain all eight components with different radial detail;
crowd retains receiver/barrel/stock/magazine as a readable reduced rifle and
records the other components as LOD-culled. Each tier enforces its actual
generated triangle budget.

The result includes right-hand attachment, primary grip, and support-hand grip
semantic sockets, screen-space outline participation, and deterministic stock,
handguard, optic, and muzzle proportions derived from the entity id. The
photoreal family reserves exactly the same API but is rejected until implemented.

### Actual stylized operator body meshes

`kami.render.operator-body-mesh/resolve-operator` replaces the primitive
cuboid body with a complete actual indexed humanoid. Sixteen semantic parts
cover head, neck, torso, pelvis, bilateral upper/lower arms, hands,
upper/lower legs, and boots. Smooth sphere, cylinder, capsule, and scaled
beveled forms reuse the exact-pinned `kotoba.render.mesh` primitives.

Hero/gameplay/crowd generate distinct geometry densities while retaining the
complete anatomy. Results contain positions, unit normals, UVs, indices,
contiguous material ranges, and four-lane joint/weight streams using the shared
`kotoba.render.character` joint order. Extremities use rigid weights; torso,
neck, and limbs use explicit two-bone blends.

Skin, cloth, metal, and accent roles resolve through portable executor
materials and participate in style-v1 outlines. Entity-stable variants adjust
head, shoulder, torso, and boot proportions. Bind-pose attachment semantics
match equipment-kit head/chest/hips/upper-arm/right-hand IDs, so existing
helmet, armour, backpack, and rifle kits attach without translation.

The photoreal sibling reserves the same anatomy, skinning, material-range and
attachment API but is rejected until its real mesh family is implemented.

### Validated operator fitting

`kami.render.operator-fit/resolve-fit` returns the adjusted transforms Royale
should consume for a readable two-hand combat pose. The rifle is yawed and
offset ahead of the torso; primary and support sockets become explicit right
and left hand targets. Shoulder armour moves outside the torso envelope and the
backpack moves behind it with measured clearance.

The result includes conservative body/equipment AABBs and a rifle capsule.
`validate-fit` checks weapon/body, equipment/body, and shoulder/backpack
clearance; grip separation; and projected silhouette occupancy against distinct
hero/gameplay/crowd budgets. It returns numeric clearance, intersections,
occupancy, constraints and error categories, and `resolve-fit` refuses an
invalid result rather than shipping intersecting fallback transforms.

`kami.render.arm-ik/solve` extends the fit with an analytic two-bone chain for
each arm. It accepts bind shoulder/elbow/hand centers, grip target, outward pole
hint and segment lengths. It returns solved centers, upper/lower centers,
quaternions, 16-value palette delta matrices, continuity endpoints, target
error, elbow angle, reach-clamp and hyperextension evidence. `resolve-fit`
includes these under `:arm-chains :left/:right` and rejects a production chain
whose grip error exceeds 0.025 or whose endpoints are discontinuous.

Unreachable standalone targets clamp inside maximum reach while preserving a
bent elbow and continuous segment endpoints. The photoreal solver uses the same
future boundary but remains explicitly unsupported.

### Actual fitted equipment and grip hands

`kami.render.fitted-equipment-mesh/resolve-meshes` replaces fitted box intents
with actual curved indexed geometry for helmet, visor, chest, shoulders,
backpack, belt, and readable mitten/grip hands. Dome/rim, curved lens,
capsule-plate, beveled shell, rounded pack/roll, and curved belt forms reuse the
shared sphere/cylinder/capsule primitives.

Every authored mesh is normalized to the authoritative AABB exported by
`operator-fit/equipment-layout`; returned `:bounds` and `:fit-volume` therefore
match exactly. The validator and renderer cannot silently disagree about
clearance. Shoulder and backpack bounds/occupancy were reduced before authoring
to preserve the operator silhouette.

Each hero/gameplay/crowd result contains actual positions, unit normals, UVs,
indices, material ranges, executor materials, triangle budget, original fit
transforms, two-bone arm chains, and weapon sockets. Hand centers are the solved
IK hand centers, preserving zero socket error and chain continuity. Photoreal
reserves the same API but remains explicitly unsupported.

The result and every part declare `:space :operator-bind-world`: mesh positions
are already fitted in operator bind-world space. Consumers render those streams
directly and must not apply the retained diagnostic `:transform` a second time.
The `:lod` map records the selected tier and tessellation policy.

### Authored weapon and face forms

`kami.render.weapon-mesh/resolve-weapon` v2 replaces flat receiver intent with
rounded indexed receiver, stock, grip, magazine, handguard, barrel, optic, and
muzzle forms. Primary and support sockets are authored on the grip and
handguard surfaces; the returned local bounds and fit capsule travel with the
same portable socket semantics used by operator-fit. Hero/gameplay/crowd tiers
reduce tessellation while retaining a readable rifle silhouette and semantic
material ranges for weapon, accent, and emissive surfaces.

`kami.render.operator-face-mesh/resolve-face` authors eyes, eyebrows, nose, and
mouth as small curved indexed forms in operator bind-world space. Their union
stays within the head landmark, declares visor compatibility, and carries an
explicit no-torso-mask occupancy result. Fitted hands expose a matching
`:mitten-wrap-contact` at their solved primary/support grip positions. The same
family APIs reserve photoreal realization but reject it until implemented.

### Production character camera

`kami.render.character-camera/resolve-camera` produces a reusable renderer and
Studio camera contract from subject world bounds. Front and left/right
three-quarter orbits target 35% screen-height coverage inside the required
28–42% range, retain head/feet margins, ground and horizon evidence, and return
position, look-at, up, vertical FOV, clipping planes, and viewport.

The deterministic settle searches fixed orbit/distance candidates around camera
collisions and subject occluders. It returns the selected attempt, stable-frame
count, zero final delta, and complete framing evidence; if none is valid it
throws with attempted evidence instead of exporting a bad shot. Selection is
split deliberately: only skinned entities use `:subject-only`, while world
selection is always `:preserve-all`, so framing never erases the environment.
Stylized is implemented; photoreal reserves the same API and remains explicit
future work.

### Camera-safe environment composition

`kami.render.environment-composition/compose` projects all eight corners of
each candidate world AABB through a resolved production character camera. It
selects only props contained by normalized safe-screen bounds, outside a padded
subject projection, grounded within tolerance, and with a visible projected
ground-contact point inside the production lower-frame band (default normalized
screen Y `0.38–0.92`). Public `project-point` and `project-aabb` functions expose
the identical projection contract to renderer and Studio tooling.

`camera-ground-facing` derives an XZ unit direction, yaw, and Y-axis quaternion
from the resolved camera look-at toward its position for +Z-forward props.
Camera-facing vegetation and other asymmetric footprints must use this result
before producing their final world AABB; a hardcoded -Z orientation is valid
only for the front orbit and fails three-quarter shots. `compose` echoes the
same orientation for renderer/Studio evidence.

Individual descriptors may override the band with top-level
`:ground-contact-screen-y-range`; this allows a background building base around
Y `0.47` while requiring foreground props at Y `0.58` or lower in frame. Optional
`:screen-extent-range [min max]` validates the larger projected AABB width/height
and rejects unreadably tiny or frame-dominating candidates as
`:screen-extent-outside-range`.

Candidate order is deterministic by descending priority then semantic id. The
result includes selected placements, rejected candidates with reasons, projected
bounds, ground-contact evidence, the subject exclusion rectangle, and the exact
ordering used. Evidence includes the configured ground band and every selected
contact Y; sky/horizon contacts reject as
`:ground-contact-outside-visible-ground-band`. No valid candidate produces an evidence-bearing fail-closed
exception. Composition copies the camera's skinned subject selection but fixes
world selection to `:preserve-all`; it cannot turn environment composition into
a subject-only render. Stylized is implemented while photoreal retains the same
future API boundary and rejects until implemented.

Candidates may declare a `:composition-region`. When policy supplies
`:required-composition-regions`, selection deterministically reserves one safe
candidate per required region before filling remaining capacity by priority.
For density targets, `:required-composition-region-counts` reserves the exact N
per region (for example three left, three right, and one building) before the
same priority fill. Evidence reports required counts, selected counts, and exact
per-region shortages.

`:required-cluster-roles-by-composition-region` adds semantic coverage inside
each region quota. The selector reserves one accepted candidate for every
required role (for example `:vegetation` and `:solid-prop`) before filling the
remaining N by priority. Only candidates that already pass projection, side,
ground-band, extent, and subject-exclusion checks can satisfy a role. Evidence
reports selected and missing role sets per region, and any missing role fails
closed instead of allowing high-priority solids to starve vegetation.
Evidence reports selected region counts and missing regions; missing coverage
fails closed, preventing a dense left or right candidate set from starving the
opposite foreground slot.

Optional candidate `:screen-side :left|:right` is verified against the projected
padded subject exclusion: left candidates must end before the subject's left
edge and right candidates must begin after its right edge. A semantic label that
does not match projection rejects as `:screen-side-mismatch` before region slot
reservation, preventing index/parity labels from spoofing composition balance.

Evidence separates unsafe `:rejected-count` from safe candidates omitted only
by `:maximum-selected` as `:unselected-safe-count` and `:unselected-safe` ids.
Thus selected, rejected, and safe-truncated counts exactly partition the input
candidate count without labeling capacity overflow as a geometry failure.

### Measurable composition gates

Environment composition can reserve actually projected occupancy cells with
`:required-screen-occupancy-cells-by-composition-region` on a configurable
`:screen-occupancy-grid`. It reports exact rectangle-union area globally and per
region (alongside overlap-inflated aggregate area), and gates minimum union area
without allowing stacked props to fake coverage. Attribute diversity policy can
require distinct `:kind` and `:geometry-variant` values per region; deterministic
reservation occurs before count fill and shortages are explicit.

Building candidates may provide final-world `:facade-layer-bounds` entries with
`:id`, `:role`, and `:bounds`. `:facade-readability` measures projected layer
extent, distinct roles, pairwise center separation, and subject overlap. Hero
junction road layers use final-world `:bounds`, `:material-role`, and separate
`:attachment-eligibility`; the road gate measures lower-half layer/role counts,
rectangle-union area, eligible region/anchor semantics, and zero subject overlap.
All thresholds are opt-in policy inputs rather than global style constants, and
all failures remain evidence-bearing and fail closed.

Composition builds one immutable projection context per call and reuses its
camera basis, aspect, FOV tangent, and near plane for every candidate corner,
piece, contact, and facade layer. Eligible candidates are indexed by region
before deterministic cell, role, diversity, and count reservation. Policy may
override `:selection-budget`; defaults fail closed above 512 candidates, 4096
projected pieces, or 128 constraints, with observed counts and limits returned
as evidence. This bounds hostile/editor-generated inputs without weakening any
visual gate or changing lexicographic ordering.

Candidates made of disjoint mask islands may supply top-level
`:bounds-set [{:min [...] :max [...]} ...]` with `:bounds-space :final-world`.
KAMI then projects each tight island independently and uses only those piece
rectangles for safe-screen checks, subject exclusion, occupancy, extent, and
union coverage. A retained coarse `:bounds` is debug/legacy data and cannot
create false subject overlap; candidate identity and deterministic selection
remain singular. Any actual overlapping island still rejects.

Enabled equipment includes the adjusted transform plus its original mesh intent
and attachment semantic, so a consumer applies the result without duplicating
KAMI offsets. Entity-stable micro-variation is applied only inside validated
clearance headroom. Photoreal reserves the same fit/validation boundary but is
explicitly unsupported until its body and equipment volumes are authored.
