# kotoba-lang/kami-solar-helix-scene

Zero-dep portable `.cljc` — the EDN authoring surface for the solar system's
"helical model": since the Sun itself moves through the Galaxy, a planet's
path in a frame where the Sun is NOT held fixed (e.g. relative to nearby
stars or the Galactic Center) is a helix/corkscrew, not the flat ellipse
seen in the ordinary Sun-fixed heliocentric frame. Both descriptions are
correct simultaneously — this is a change of reference frame (Galilean
addition of the Sun's translational velocity to each planet's orbital
velocity), not a competing physical claim.

## Why this repo exists

Popular 2012-era "vortex" animations of this idea (e.g. DjSadhu's "The
Helical Model") drew the geometry wrong: a tight corkscrew. The real ratio
of the Sun's galactic advance per orbit to that orbit's own circumference
is large — about **7.4x for Earth**, about **16.8x for Jupiter** — so the
true curve is a very gently stretched-out spring, nearly a straight line
with a small sinusoidal wobble, not a tight vortex tube. This repo computes
that ratio from real orbital data instead of asserting it, so the corrected
picture is backed by a number, not just a claim (see
`solar-helix-scene/pitch-to-circumference-ratio`).

## Simplifications (read before consuming positions downstream)

- **Circular, coplanar orbits.** `resources/solar-helix.edn` uses each
  planet's real semi-major axis as a fixed orbital radius and its real
  sidereal period, but ignores eccentricity and each planet's own small
  inclination to the ecliptic. Good enough to illustrate the helix/pitch
  geometry; this is **not an ephemeris** (use VSOP/DE440 for true
  positions).
- **Single-axis tilt for the galactic frame.** A real ecliptic->Galactic
  transform needs the solar apex's full spherical coordinates. This repo
  instead uses one scalar `:ecliptic-tilt-deg` (~60°, the commonly cited
  popular-science rounding for that inclination) and rotates the
  heliocentric orbital plane by that single angle before adding the Sun's
  forward translation. Treat the resulting 3D path as illustrative of the
  **helix shape and pitch**, not as true galactic coordinates.
- **`:sun-speed-km-s` is a rounded CONFIG default (220.0)** — the
  traditional IAU-era Local Standard of Rest circular speed; modern
  Gaia-based fits commonly land ~225-240 km/s depending on the adopted
  R0/rotation curve. Swap the EDN value for a different estimate.

## Public API

- `solar-helix-edn` — the shipped EDN source (literal string constant),
  byte-identical to `resources/solar-helix.edn`.
- `bodies-from-edn` / `galactic-frame-from-edn` — parse `:helix/bodies` /
  `:helix/galactic-frame` from arbitrary EDN source, throwing `ex-info`
  (`:solar-helix-scene/error` of `:not-a-map` / `:no-bodies` /
  `:no-galactic-frame`) on failure.
- `shipped-bodies` / `shipped-galactic-frame` — convenience loaders against
  the shipped `solar-helix-edn`.
- `orbital-speed-km-s` — a body's circular-orbit speed, *derived* from its
  radius and period (`2*pi*r/T`), not transcribed — checked in the test
  suite against published values.
- `heliocentric-position-au` — the flat, Sun-fixed view.
- `galactic-frame-position-au` — the helical, Sun-moving view.
- `pitch-au` / `circumference-au` / `pitch-to-circumference-ratio` — the
  numbers that show the helix is a gentle stretched spring, not a
  corkscrew.
- `helix-tick` — per-frame convenience: both frames for every body at a
  given time, for a renderer to consume directly.
- `all-body-names` — `["sun" "mercury" "venus" "earth" "mars" "jupiter"
  "saturn" "uranus" "neptune"]`.

## Status

New (not a restoration of a deleted Rust crate, unlike most `kami-*-scene`
siblings). Data-tier only, following the same `kotoba-lang/scene`-accessor
EDN-authoring-surface pattern as `kami-atmosphere-scene` / `kami-cam-scene`:
this repo has no renderer of its own. `kami-engine`'s native/WebGPU adapter
line is mid-migration (Rust workspace removed, adapters not yet wired up
for custom 3D scenes as of 2026-07), so there is currently no pixel-output
consumer for this data — it is a correct, reusable physics/data asset for
whichever adapter picks it up.

## Develop

```bash
kbb -M:test
```
