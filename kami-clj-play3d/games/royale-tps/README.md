# KAMI Royale TPS

A fork of [`../royale`](../royale) that turns the demo into a third-person
shooter with a match.

The complaint that started it: the live build at `isekai.network/gftd/royale`
"looks overhead, like a 2D game". It does, and the camera is only the visible
half of why.

## What actually made it read as 2D

**The camera.** The shipped scene authors `{:distance 27 :height 15}`. At that
rig a 1.9-unit actor is a handful of pixels at 720p; the file's own comment
records that the previous 70/48 rig lost material and team colour entirely, and
27/15 is the same problem one step smaller. This fork uses a 4.2-unit
over-the-shoulder boom at shoulder height, with a shoulder offset, a collision
pullback and a second sighted rig the engine blends to.

**The movement.** `logic.clj` wrote raw input axes straight into
`set-velocity!`, so "forward" meant world +X regardless of where the camera
pointed. That is a top-down convention, and it survives any camera you put on
it. Movement here goes through `move!`, which resolves stick input against the
player's own yaw. This single call is the largest single difference between the
two files.

**Nothing to look at closely.** A near camera only pays off if there is
something happening at that distance. The parent has no health, no weapon, no
ammo, no reload, no aim, no hit feedback, no storm and no win condition, so
there is nothing a close camera would show you. Roughly half of this fork is
those mechanics.

## What changed

| | `royale` | `royale-tps` |
|---|---|---|
| camera | 27 back / 15 up, fixed | 4.2 back / 1.65 up, over-the-shoulder, + sighted rig |
| movement | raw axes into velocity | camera-relative, diagonals normalised |
| aiming | none — no rotation in the contract | yaw/pitch pose, ADS blend, spread cone |
| shooting | host-side auto-fire that cannot miss | fire rate, magazine, reload, spread, falloff, capsule hitboxes, headshots |
| weapons | none | 25 rows from `battle_royale_weapons.edn` |
| health | none | health, shield, armour, one damage pipeline |
| bots | `move-toward!` the player from 3000 units, through walls | sight cone, hearing, memory, five states, same weapons as the player |
| bot spawns | one of four points via `(rand-int 4)` | evenly spaced ring by index |
| storm | none | eight phases from `battle_royale_storm.edn` |
| match | bots respawn forever; nobody wins | 24 entrants, placements, a winner |
| gameplay source | 50 lines | 158 lines |

The bot-spawn change is worth a note. `(rand-int 4)` on the shipped browser host
returns a constant after the first draw — its PRNG loses precision in JavaScript
— so the parent's four spawn points collapse to one and the entire opposition
arrives from a single direction in single file. `spawn-ring!` needs no draw at
all: the slot is the bot's own index. The underlying defect is documented in
[`../../../kami-gameplay/README.md`](../../../kami-gameplay/README.md).

## Running it

```sh
# the rules, on both platforms
cd ../../../kami-gameplay && clojure -M:test && npx --yes nbb bin/run_tests.cljs

# a full 24-entrant match against this scene, headless
npx --yes nbb bin/simulate.cljs --seed 20260822
```

The harness plays the match with no renderer through the same code a host would
run. A representative result: 280 seconds, 433 shots at a 22.2% hit rate, 23
deaths all by gunfire, five of them headshots, the player first with 8 kills.

## What is not done

**This is not deployed and has not run in a browser.** It needs two things this
repository does not contain:

1. **Host bindings.** `logic.clj` calls 28 host imports. The seven original
   `kami:engine/*` interfaces are implemented in `kotoba-lang/host`; the seven
   new ones (`attributes`, `combat`, `weapon`, `aim`, `zone`, `perception`,
   `match`) are declared in `wit/kami-interface.edn` and implemented as rules in
   `kami-gameplay/`, but nothing binds them into a WASM import object yet.
2. **Publication.** The live site is served from `network-awai/network-isekai`,
   a repository this workspace cannot reach.

**No level collision.** Shots pass through buildings and bots walk through them.
The seams are there and unwired — see the kami-gameplay README.

`clojure -M scripts/guest_lint.cljk` from the repository root checks this file
against the declared guest vocabulary: no call to a name the guest does not
have, and no host import at the wrong arity. It found ten invented names in the
first draft of this file, including four pieces of floating-point arithmetic the
guest subset cannot do.
