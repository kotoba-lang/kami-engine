# ADR-0048: Valve Steam Support — Desktop Packaging + a Steamworks Services Seam (CLJ-facing)

**Date**: 2026-06-27
**Status**: Proposed — Phase 1 (services seam + stub backend + desktop packaging predicate) implemented + tested
**Author**: kami-engine team
**Related**: ADR-0037 (cross-platform packaging — the OS host matrix this extends), ADR-0035 (`kami-engine-clj` Clojure→WASM), ADR-0040 (everything describable is EDN), `kami-script-runtime/src/platform.rs`, `wit/kami-game/world.wit`

---

## Context

A game in this engine is authored **write-once** as EDN data + a Clojure subset, compiled to one
`game.wasm`, and run by a per-platform Rust host (`kami-script-runtime` + `kami-render`). ADR-0037
already ships the **desktop OS hosts** — macOS (Metal) / Linux (Vulkan) / Windows (DX12) — on the
JIT `wasmtime` backend with full wgpu rendering.

"Support Valve Steam" is therefore **not** a new renderer, language, or runtime model. Steam is two
orthogonal things, and the existing stack already supplies the hard one:

1. **A desktop distribution channel.** A Steam build *is* a desktop OS build (the same `wasmtime` +
   wgpu host) wrapped in a Steam depot: a `steam_appid.txt`, the depot file layout, and upload via
   `steamcmd`/SteamPipe. Nothing about the renderer or the guest changes. Steam is **not an OS**, so
   it is not a new `Target` — it rides the three desktop OS targets, each shipping its own depot.

2. **A set of platform services** — achievements, stats, rich presence, cloud saves, the overlay,
   and Steam Input. These are the genuinely new surface. The question this ADR answers is: *how does
   a CLJ/EDN game reach Steamworks without breaking write-once authoring or cross-backend
   determinism?*

The determinism constraint is load-bearing. ADR-0037's whole no-JIT/console story rests on
wasmtime and wasmi producing **bit-identical** runs (the golden-frame test) so lockstep co-op,
replay, and headless CI hold. If Steam state (e.g. "is this achievement already unlocked?") could
flow *into* the i64 sim, a run would diverge between a Steam build and a non-Steam build — breaking
that invariant. So the services seam must be **output-only** from the sim's perspective.

The native binding (`steamworks-rs`) also needs the Steamworks SDK redistributable, a registered
**App ID**, and a running Steam client — none of which exist in CI or on a contributor's machine.
This mirrors ADR-0037's console GPU backend exactly: the honest move is **ship the seam now, gate
the SDK binding** behind a feature, and exercise the seam everywhere with a stub.

---

## Decision

Add Steam along the same two seams it actually is — packaging and services — and keep the
game-facing contract identical.

### 1. Steam is a desktop *distribution predicate*, not a `Target`

`kami-script-runtime::platform` gains `Target::steam_distributable(self) -> bool`, true for exactly
`Mac | Linux | Windows`. A Steam build reuses that OS target's `PlatformSpec` unchanged
(`jit_allowed`, `LogicHost::Wasmtime`, the OS's wgpu backend, BCn textures) and layers on the Steam
depot + `steam_appid.txt` + the services backend. Mobile/console/web are excluded: no-JIT consoles
ship through their own first-party stores, and web is the browser path. A unit test pins the
predicate and asserts every Steam target keeps the desktop host contract (JIT + wasmtime), so it
can't silently regress when the matrix changes.

### 2. A Steamworks services seam as an OUTPUT-ONLY effects sink (CLJ-facing)

The guest reaches Steam through one new WIT interface, modelled **exactly like `kami:engine/audio`**
— fire-and-forget, nothing read back:

```wit
interface steam {                              // wit/kami-game/world.wit
    unlock-achievement: func(id-ptr: s32, id-len: s32);
    set-stat:           func(name-ptr: s32, name-len: s32, value: s64);
    set-rich-presence:  func(key-ptr: s32, key-len: s32, val-ptr: s32, val-len: s32);
}
```

CLJ game logic calls three builtins (compiled by `kami-engine-clj`, no codegen change — they ride
the generic host-import path):

```clojure
(steam-unlock! "FIRST_BOSS")                 ; achievement by Steamworks API name
(steam-set-stat! "bosses" 1)                 ; integer stat, absolute value
(steam-rich-presence! "status" "in_combat")  ; rich-presence key/value
```

Because the interface emits but never returns sim-affecting state, **a game runs bit-identically
with or without Steam connected** — the wasmtime↔wasmi golden-frame parity is untouched, and a
replay recorded on a Steam build re-runs on a non-Steam build. This is the single most important
constraint and the reason the seam is output-only by construction, not by convention.

### 3. Host plumbing mirrors the audio queue

`kami-script-runtime` buffers each call as a `steam::SteamEvent`
(`UnlockAchievement | SetStat | SetRichPresence`) in `HostState::steam_queue`, exactly as
`audio_queue` works. After a tick the engine calls `drain_steam_queue()` and forwards the batch to a
`steam::SteamBackend::apply`:

| Backend | Linked | Steamworks? | Use |
|---|---|---|---|
| `StubSteam` (default) | everywhere | no — log + no-op | CI, web, non-Steam desktop, headless golden-frame |
| `steamworks-rs` impl | `steam-sdk` feature (off) | yes | an actual Steam desktop build — **not in this scaffold** |

The trait is infallible at the seam (platform telemetry must never break gameplay); a real impl
swallows/logs its own errors and can override `apply` to coalesce (e.g. one `StoreStats` flush/frame).

### 4. Achievement / stat catalog is EDN (ADR-0040), baked at package time

Achievement and stat **ids** are authored as data alongside `scene.edn`, e.g. `steam.edn`:

```clojure
{:steam/app-id 480
 :steam/achievements
 [{:id "FIRST_BOSS" :name "Slayer"     :desc "Defeat the first boss"}
  {:id "NO_HIT"     :name "Untouchable":desc "Clear a stage hitless"}]
 :steam/stats
 [{:id "bosses" :type :int :default 0}]}
```

The guest only ever *names* these strings; the catalog is consumed by the `bb kami` packaging step
(Clojure side) to generate the Steamworks config and validate that every `steam-unlock!`/`set-stat`
id the game references exists in the catalog. (Catalog→VDF generation and the lint are follow-up
packaging work; this ADR fixes the data shape.)

---

## Architecture

```
        write-once guest (unchanged authoring)
        ┌───────────────────────────────────────┐
        │ logic.clj   (steam-unlock! / set-stat! │
        │              / rich-presence!)         │
        │ steam.edn   (achievement/stat catalog) │
        └───────────────┬───────────────────────┘
        kami-engine-clj  │ compiles steam builtins → import kami:engine/steam
                         ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ kami-script-runtime host  (Mac / Linux / Windows desktop)    │
   │   bind_steam → HostState.steam_queue → drain_steam_queue()   │
   │                          │                                   │
   │                          ▼  SteamBackend::apply              │
   │            ┌── StubSteam (default, log+noop) ──┐             │
   │            └── steamworks-rs  [feature steam-sdk, gated] ──┐ │
   └────────────────────────────────────────────────────────┼──┘
   packaging: Target::steam_distributable() = Mac|Linux|Windows
              + steam_appid.txt + depot layout  (bb kami package … --dist steam)
```

Everything above the `SteamBackend` line is portable and deterministic; only the `steamworks-rs`
binding is platform/SDK-proprietary and lives behind a feature — the same seam discipline ADR-0037
used for the console GPU backend.

---

## Consequences

**Gained**
- Steam support with **zero change to the renderer, the runtime model, or write-once authoring**. A
  CLJ/EDN game emits achievements/stats/presence with three builtins; the desktop host it already
  ships on grows a Steam depot.
- Determinism is preserved *by construction*: the seam is output-only, so Steam and non-Steam builds
  produce identical sim runs — replay, lockstep co-op, and golden-frame CI all still hold.
- The seam is exercised on **every** target via `StubSteam` (CI/web/non-Steam desktop), so the
  `kami:engine/steam` imports always resolve and the path can't bit-rot before the SDK lands.

**Costs / risks**
- The real `steamworks-rs` binding is **out of this scaffold's scope** — it needs the Steamworks SDK
  redistributable, an App ID, and a running Steam client, so it ships behind the `steam-sdk` feature
  and is wired only on an actual Steam desktop build. "Steam support" here = "every layer portable
  and deterministic except the SDK binding," stated precisely (cf. ADR-0037's console wording).
- Output-only is a deliberate limitation: a game cannot *query* Steam (owned DLC, unlocked-state,
  cloud-read) from the deterministic sim. Such reads, if ever needed, must enter as host-side input
  *before* a tick (like the input snapshot), never mid-sim — a future ADR if a game requires it.
- Cloud saves and the overlay are not part of the per-tick services seam; they are host/shell-level
  concerns (save = snapshot serialization; overlay = Steam client) handled by the desktop shell, not
  the guest.

**Phased rollout**
1. ✅ **Services seam + stub + packaging predicate** (this ADR): `kami:engine/steam` WIT interface;
   `kami-engine-clj` builtins (`steam-unlock!` / `steam-set-stat!` / `steam-rich-presence!`) →
   import on the generic host-import path (no codegen change); `kami-script-runtime` `bind_steam` +
   `steam_queue` + `drain_steam_queue` + `steam::{SteamEvent, SteamBackend, StubSteam}`;
   `Target::steam_distributable`. Tests: clj compiles + imports the interface; runtime fills/drains
   the queue end-to-end; backend fan-out + stub-is-noop; platform predicate invariants.
2. **EDN catalog tooling** — `bb kami` consumes `steam.edn` to generate Steamworks config and lint
   that every referenced id exists (the ADR-0040 author-time guard).
3. **`steam-sdk` backend** — `steamworks-rs` impl of `SteamBackend` behind the feature; `bb kami
   package <os> --dist steam` lays out the depot + `steam_appid.txt`. Needs the SDK + an App ID, so
   validated only on a real Steam build (the ADR-0037 console-seam pattern).

---

## Alternatives Considered

1. **Make `Steam` a new `Target` in the platform matrix.** Rejected: Steam is not an OS — it spans
   Metal/Vulkan/DX12 and ships a per-OS depot. A `Target::Steam` would need a render backend it
   can't have. A `steam_distributable()` predicate over the desktop targets models reality without
   forking the matrix.

2. **Let the guest query Steam state (two-way interface).** Rejected: any read that influences the
   sim diverges Steam vs non-Steam runs, breaking the cross-backend determinism ADR-0037 depends on.
   Output-only keeps the golden-frame guarantee. Reads, if ever required, enter as pre-tick host
   input, not as a guest call.

3. **Link `steamworks-rs` directly into the host now.** Rejected: the SDK redistributable + App ID +
   Steam client aren't present in CI or for contributors, so the host wouldn't build/test portably.
   A stub backend linked everywhere + a gated SDK feature keeps the seam green and the SDK optional.

4. **Re-author Steam integration per game in Rust (`kami-app-{game}`).** Rejected: violates
   write-once CLJ/EDN authoring and duplicates the integration per title. The seam belongs in the
   host, named by the guest.

---

## References

- ADR-0037 — cross-platform packaging (the desktop OS host matrix + the seam-now/SDK-later pattern)
- ADR-0035 — `kami-engine-clj` Clojure→WASM (the generic host-import path the builtins ride)
- ADR-0040 — everything describable is EDN (the `steam.edn` catalog rationale)
- `wit/kami-game/world.wit` — `interface steam` (output-only services)
- `kami-script-runtime/src/steam.rs` — `SteamEvent` / `SteamBackend` / `StubSteam`
- `kami-script-runtime/src/platform.rs` — `Target::steam_distributable`
- `kami-engine-clj/src/ast.rs` — `SteamUnlock` / `SteamSetStat` / `SteamRichPresence` builtins
