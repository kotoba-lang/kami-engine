# kami-gameplay

Portable gameplay framework for the KAMI engine: the rules a third-person
shooter needs, as pure `.cljc` over plain maps, with no dependencies.

## Why this exists

The engine could render a battle royale but could not *be* one.

`kami.host`'s ECS entity — the thing a compiled `.clj` game manipulates through
`kami:engine/*` — is exactly this:

```clojure
{:tag "bot" :x 0 :y 0 :z 0 :vx 0 :vy 0 :vz 0}
```

Seven numbers and a string. `get-rx`..`get-rw` return the constants `0 0 0 1`
and `set-rotation` is a no-op, so nothing in the contract can even express which
way an actor is facing. There is no health, no team, no weapon, no ammo.

A game written against that can move point masses around a plane. That is the
whole reason `games/royale` reads as an overhead 2D toy however it is lit: not
the camera distance, not the shader — the contract. Its 50 lines of gameplay
write two raw axes into a velocity, drop bots at one of four hardcoded points,
walk every bot at the player from anywhere on the map, and hand shooting to the
host. There is no aim, no ammo, no reload, no damage, no storm and no state in
which anyone has won.

Meanwhile three EDN tables in `kami-game-scene/data/` describe 25 weapons, an
eight-phase storm and eleven consumables, each with a header saying a Rust
function "stays the builtin fallback AND the parity oracle". That Rust left with
the workspace. The tables have had no runtime and no oracle since.

So: rules with no data on one side, data with no rules on the other. This
package is the join. Unreal's value is not its renderer either — it is the
Gameplay Framework, the Ability System, AI perception and the camera manager.
This is that layer, at this engine's scale and in this workspace's idiom.

## What it provides

| namespace | role | Unreal analogue |
|---|---|---|
| `kami.gameplay.attributes` | named f32 attribute plane with per-attribute defaults and clamps | `UAttributeSet` |
| `kami.gameplay.damage` | one damage pipeline: shield, armour, teams, kill credit, events | `GameplayEffect` |
| `kami.gameplay.weapon` | equip / fire-rate / magazine / reload state machine over the weapon table | weapon component |
| `kami.gameplay.ballistics` | spread cones, capsule hitboxes with a head segment, falloff, projectile travel | trace + damage type |
| `kami.gameplay.aim` | controller pose, over-the-shoulder boom, ADS blend, camera-relative movement | `PlayerCameraManager` + `CharacterMovement` |
| `kami.gameplay.zone` | shrinking safe circle from the storm schedule | game-mode volume |
| `kami.gameplay.perception` | range / cone / occlusion / memory | `AIPerceptionComponent` |
| `kami.gameplay.ai` | five-state bot behaviour driving the same weapons players use | `AIController` |
| `kami.gameplay.loot` | rarity-weighted drops, pickups, consumables with their own caps | inventory |
| `kami.gameplay.match` | entrants, placements, terminal state | `GameMode` / `GameState` |
| `kami.gameplay.world` | the composed deterministic step | the tick |
| `kami.gameplay.rng` | the shared seeded PRNG | — |

Everything is a pure function of `(world, dt, inputs)`. No clock read, no atom,
no global RNG — so a match replays exactly from a seed and an input log, and a
test can assert on a whole match instead of on a frame.

## Run it

```sh
kbb -M:test                      # JVM
npx --yes kbb --backend sci bin/run_tests.cljk     # Node / ClojureScript
npx --yes kbb --backend sci bin/simulate.cljk      # a full 24-entrant match, headless
```

Both suites must pass. That is not belt-and-braces: cross-platform bit-identical
arithmetic is a load-bearing property here, and a suite that ran on one platform
could not observe it.

## Measured

Numbers below were produced by the commands above, not estimated. Re-measure
rather than quoting these; they are here to say what "it works" was taken to
mean, not to be cited later as facts.

**Suite.** 140 tests / 71,355 assertions, identical on both platforms.

**Mutation check.** Sixteen deliberate defects were introduced one at a time and
the suite re-run. Fifteen turned it red. The sixteenth did not, and that was a
real hole rather than an acceptable one: reverting `rng/int` from high-bit to
modulo extraction left every existing assertion green, because a 4-cycle passes
both a distinct-values check and a uniformity check. Two periodicity tests were
added; the mutation now fails four assertions.

**A match.** 24 entrants, the full eight-phase storm, four seeds:

| seed | length | shots | connected | deaths by bullet / storm | winner |
|---|---|---|---|---|---|
| 1 | 501s | 376 | 24.7% | 21 / 2 | bot #22 |
| 7 | 604s | 393 | 24.4% | 21 / 2 | bot #5 |
| 4242 | 461s | 452 | 21.0% | 22 / 1 | bot #7 |
| 20260822 | 280s | 433 | 22.2% | 23 / 0 | the player, 8 kills |

## Two defects this found in the shipped engine

**The browser host's PRNG collapses.** `kami.host` advances its generator with
`(bit-and (+ (* 1103515245 s) 12345) 0x7fffffff)` written directly in
ClojureScript. In JavaScript that product reaches 2.37e18, past
`Number.MAX_SAFE_INTEGER` at 9.0e15, so it loses its low bits before the mask is
applied. From seed 1, the first twenty draws of `(rand-int 4)`:

```
shipped host : 2 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
exact        : 2 3 0 1 2 3 0 1 2 3 0 1 2 3 0 1 2 3 0 1
```

`games/royale` picks one of four spawn points with exactly that call, so every
bot after the first spawns at the same corner and the whole opposition arrives
from one direction in single file. `rng/legacy-host-int` reproduces the defect so
it stays under test; the fix belongs in `kotoba-lang/host`, which this
repository cannot land.

Note the second row too. Even computed exactly, `mod` on an LCG with a
power-of-two modulus has a period of 2^k in its low k bits — a fixed rotation is
barely better than a fixed corner. `rng/int` takes the high bits instead.

**A bad write is silent on one platform and fatal on the other.** `(double nil)`
throws on the JVM and yields NaN in ClojureScript, where every comparison
against NaN is false — so a clamp falls through and stores it, a NaN deadline
never expires, and a NaN health is neither alive nor dead. A missing `:now-ms`
in one call site threw on the JVM and ran green under Node. Both platforms now
refuse non-finite attribute writes. This is the reason the suite is required to
pass on both: the Node run alone was reporting a pass over a live defect.

**Sight cones need something to sweep them.** Bots with a 110-degree view cone
and a fixed spawn facing cannot notice anything that does not walk into it. In
the first full match run, bots dropped on a ring all facing the same way never
acquired the player at all: they stood still for the entire match while he shot
at them from 210 units away, and the storm did most of the killing. Idle bots now
sweep. Storm deaths fell from 8 of 23 to 1 of 23.

## Design notes worth knowing before extending it

- **Attribute reads return declared defaults, not nil or 0.** A spawn that forgot
  to set health cannot be silently born dead.
- **Every write is clamped**, so nothing downstream re-checks a number it did not
  compute.
- **Refused damage is reported, not swallowed** (`:kind :blocked` with a
  `:reason`). A shot that silently does nothing is indistinguishable from a bug
  in the hit detection that produced it.
- **Deaths are detected by comparing alive sets**, not by listening for kill
  events, so a death from a source nobody has written yet is still placed.
- **System order is a value** (`world/system-order`) and asserted by name:
  reload before firing, fire before movement, movement before the storm.
- **Bots use the mechanics players use.** They aim through the same attributes,
  fire through the same rate gate, run dry, reload, and sight their weapons to
  tighten the same cone. A bot cannot be more accurate than the weapon it holds
  allows.
- **Engagement range comes from `:damage-falloff`**, not `:range` — bots fight
  where their weapon still does full damage, which gives each archetype a
  distinct distance out of data that already exists (shotgun 9, SMG 22, pistol
  27, rifle 45, sniper 180).

## What this is not

- **Not deployed.** Nothing here has run in a browser. The live game at
  `isekai.network/gftd/royale` is served from `network-awai/network-isekai`,
  which this workspace cannot reach, and its host is `kotoba-lang/host`, which it
  cannot push to. Adopting this requires work in both.
- **Not a host.** The `kami:engine/*` functions in `wit/kami-interface.edn` are
  declared and implemented here as rules; binding them into a WASM import object
  is the host's job and is not done.
- **No level geometry.** `ballistics/first-hit` takes a `:max-distance` the
  caller shortens to a wall hit, `perception/can-see?` takes an `occluded?`
  predicate, and `world`'s integrator takes a `:collide` function. All three
  seams exist and all three are unwired: without a collision world, shots pass
  through buildings and bots walk through them.
- **No netcode.** Determinism is the precondition for lockstep, not lockstep.
