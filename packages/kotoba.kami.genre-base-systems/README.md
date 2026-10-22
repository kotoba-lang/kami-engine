# kami-genre-base-systems

> **Note**: `gftdcojp/isekai-network` (referenced below) is archived — its content moved to `gftdcojp/network-isekai/public/games/gftd/shiro-pico/`.

Base/foundation gameplay systems, one per genre category from a PlayStation
Store-style genre-browse taxonomy — **generic genre archetypes only**, not
tied to any specific commercial title's characters, art, story, or exact
mechanics. Follows the precedent set by `gftdcojp/isekai-network`'s concept 1
(熱痕ランナー): each is a minimal, honest `author.clj`/`logic.clj` pair built
strictly within the kami-clj compiler subset's **proven vocabulary** —
`defsystem`, `defn`, `def`, `let`, `when`/`cond`/`if`, `f32`, `spawn-entity`,
`despawn-entity`, `set-position!`, `set-velocity!`, `get-x`/`get-y`,
`nearest-tagged`, `count-tagged`, `doseq-entities`, `move-toward!`, `axis`,
`tick-n`, `mod`, `rand-int`, `zero?`, `</=/not=`, `defatom`/`set-atom!` —
confirmed working end-to-end via
[`kami-engine-clj`](https://github.com/kotoba-lang/kami-engine/tree/main/kami-engine-clj)
(compiler, consolidated from a short-lived standalone `kotoba-lang/engine`
repo) and `kotoba-lang/kami-script-runtime-rs` (WASM host) this session.

## Honest scope

18 categories were requested. Not all of them are gameplay-mechanic
archetypes — some are business/marketing/production categories with no
distinct core loop of their own. This repo does not force-invent a fake
mechanic to hit 18/18 "real systems":

| Category | Treatment |
|---|---|
| モダンリメイク (modern remake) | N/A — a production category about remaking existing games, not a gameplay archetype |
| 基本プレイ無料 (free-to-play) | N/A — a business model, not a gameplay archetype |
| インディーズ (indie) | N/A — a publishing category, not a gameplay archetype |
| ポストアポカリプス (post-apocalyptic) | a setting/tuning preset (scarcity tuning), not a distinct core loop — layered on another base rather than duplicated |

## Known engine constraints (apply across every system here)

- **Single local input stream only.** No simultaneous local multiplayer
  input exists in the current engine (confirmed this session via
  `isekai-network`'s concept-3 investigation: `kami-input-scene`'s
  `InputMap` schema has no player-index dimension, and no reference game's
  `logic.clj` calls `axis` with a player argument). ローカルマルチプレイヤー
  and 対戦格闘 both use the same honest "1 human + 1 AI" adaptation this
  constraint forces.
- **No terrain/tilemap/pathfinding/vehicle-physics/skeletal-animation
  reachable from kami-clj logic** — those are Rust-substrate crates the
  guest module can't call into. Genres that traditionally lean on them
  (open world terrain, platformer collision, strategy pathfinding, sports
  vehicles) get the best honest gameplay-logic-only approximation using
  only tagged entities + positions, not full genre fidelity.

## Status (updated)

**All 18/18 categories now have real, `kbb -M author.clj`-verified
`author.clj`+`logic.clj` pairs.** 14 are genuine gameplay archetypes with a
distinct core loop; 4 (modern-remake, free-to-play, indie, post-apocalyptic)
are honestly-labelled supporting systems layered on another base, not
fabricated mechanics for categories that aren't gameplay archetypes — see
`design/*.edn` for each category's precise framing.

A maturity pass (ADR-2607031800) then deepened most systems with 1-3
additional `defsystem`s / richer state beyond the bare minimal proving
mechanic — e.g. rhythm gained a combo/streak counter, puzzle gained a
second distinct level, stealth's binary detection became an escalating/
decaying alert level, strategy gained a real win/lose condition, sports had
a genuine dead-code bug fixed (a declared-but-never-incremented score
counter, now backed by a real AI opponent completing the two-goal shape).
`indie` was deliberately left at its original minimal-kernel shape — any
further addition would work against its own stated purpose as the smallest
possible starting point.

A real finding from this pass: the "proven vocabulary" list above was
empirically derived from what this session's reference games happened to
demonstrate, not from the compiler's actual capabilities -- checking
`kotoba-lang/engine`'s own `ast.cljc`/`codegen.cljc` source directly showed
`*` (multiplication), `not`, `do`, `<=`/`>=`, and `and`/`or` are all real,
supported emit targets, wider than the reference-game-grep method alone
would suggest. Some of this repo's code (e.g. `local-multiplayer`'s
precomputed `360` instead of a `*` call) predates that discovery and was
left as-is rather than "simplified" under time pressure — both forms are
equally correct, just differently verbose.

Not yet attempted: WASM compilation (`kotoba-lang/engine`'s `compile-file`)
for any of the 18 systems — a real, valuable follow-up, out of scope for
this pass given this session's established slow-build cost. `author.clj`
verification only confirms the datalevin→`scene.edn` authoring step works,
not that `logic.clj` actually compiles to WASM.

## `platformer` — action-systems layer (2026-07-18)

`design/platformer.edn`'s `:genre/status` moved from `:design-only` to
`:implemented`: `games/platformer/{src,test}` now also carries a second,
decoupled artifact alongside its original kami-clj-subset `logic.clj`
(the honest sequential-waypoint-traversal loop, unchanged) — a pure,
portable `.cljc` **action-systems layer** for a Castlevania-style 2D
side-scroller, built for a different host (`network-isekai` /
`kotoba.kami-host`'s ECS), not for `kami-engine-clj`/WASM:

- `kami-genre-base-systems.platformer.camera` — room-lock camera
  (`clamp-camera`, `step-camera`, `start-room-transition`)
- `kami-genre-base-systems.platformer.weapon` — whip attack FSM
  (`step-whip`, `whip-hitbox`), sub-weapon hearts-gated fire/cooldown
  (`try-fire-sub-weapon`, `step-sub-weapon`), and the item-crash combo
  (`item-crash?`, `try-item-crash`)
- `kami-genre-base-systems.platformer.style` — combo/style-meter rank
  (`register-hit`, `register-damage-taken`, `step-style`, `meter->rank`)
- `kami-genre-base-systems.platformer.boss` — HP-threshold phase/attack
  framework (`step-boss-phase`)
- `kami-genre-base-systems.platformer.stairs` — a decoupled extension
  point (`{:in-zone? ... :resolve ...}`) so a host can plug in
  `physics-2d`'s stair/diagonal-movement work without this repo
  depending on `physics-2d` directly

Run tests: `cd games/platformer && kbb -M:test`. This layer is
**not** exercised by `scripts/verify` (that pipeline is specific to
compiling `logic.clj` through `kami-engine-clj` and executing it via
`kami-host` — a different language subset and a different runtime
entirely; there is no equivalent WASM path for this layer yet).
