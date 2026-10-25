# Spirit in Physics — Architecture

How the four layers fit, and why each was chosen. The one-liner:

> **clj is the brain · Datomic is the world · Kotoba is the memory · `kami-render` (wgpu) is the GPU arm.**

```
 story-bible (JSON-LD, single source of truth)         ../../com-junkawasaki/org-spirit-in-physics-comics
        │  sip.lore (read volumes / personas / emotion scores)
        ▼
 ┌─────────────────────────── JVM authoring ───────────────────────────┐
 │  sip.world ──build──▶ Datomic/datalevin store  (THE WORLD)           │
 │                         scene datoms + game datoms (sip.schema)      │
 │  kami.db/snapshot ──▶ portable snapshot (transit/edn)               │
 └──────────────────────────────┬──────────────────────────────────────┘
                                 │  public/snapshot.edn  (served by Cloudflare)
                                 ▼
 ┌────────────────────────── browser runtime ──────────────────────────┐
 │  sip.web ──fetch snapshot──▶ kami.ecs world                          │
 │  kami.sim/run! loop ──▶ sip.systems (ambient) + render-IR            │
 │                            │                                         │
 │                            ▼                                         │
 │             kami-clj-host (Rust/wgpu, WASM) ──▶ WebGPU canvas        │
 └──────────────────────────────┬──────────────────────────────────────┘
                                 │ saves / 瓶詞 letters (CID)
                                 ▼
                  Kotoba — content-addressed durable + distributed
                  (cross-device save, async non-toxic multiplayer,
                   permanent Awakening record)         sip.store/Durable
```

## Why these four

### Clojure / ClojureScript — the brain
The whole engine loop (scene, ECS, gameplay systems, *and* the non-combat
session logic) is authored in clj/cljs via **`kami-engine-sdk-clj`**. We write no
Rust. The session FSM (`sip.session`) is pure and `.cljc`, so the *exact same
code* validates on the JVM (tests, server-side replay) and runs in the browser.

### Datomic / datalevin — the world (source of truth)
A scene *is* a set of datoms; a component is an attribute; an entity is a
Datomic entity. This is the engine's model (`kami.scene`), and it's a perfect
fit for this game specifically:

- **Undo / provenance via `as-of`.** The story is *about* an AI whose history is
  sacred and must not be erased. Modelling the agent's Awakening as an immutable,
  time-travelable log is the mechanic and the theme at once. (Needs Datomic
  Cloud/Peer; datalevin is the OSS default for everything else.)
- Game state (`sip.schema`: kokoro, bond, areas, sessions, insight web) lives in
  the same store as the render scene — one query model, no second persistence
  format.

### Kotoba — the memory (durable + distributed)
Datomic is the *live* world; **Kotoba** (content-addressed distributed Datalog)
is what makes it **durable and shareable**:

- **Saves are CIDs** — immutable, portable across devices.
- **瓶詞 (bottle-letters)** are the async, no-chat, non-toxic multiplayer: a
  letter is `put!` to Kotoba, its CID recorded in the world; other players read
  it by CID as it floats down the canal.
- Reached through the `sip.store/Durable` protocol. `LocalCas` (sha-256 = CID) is
  the zero-dependency default; the real Kotoba client implements the same
  protocol, so promotion changes one record, nothing else.

### kami-render (Rust/wgpu) — the GPU arm
We do **not** reimplement WebGPU in clj. The browser loads a snapshot, the
ambient `sip.systems` mutate the ECS, and `kami.sim/render-once` ships a
render-IR to the proven `kami-clj-host` wasm, which owns wgpu bootstrap and the
WGSL pipelines. (Authority boundary per the engine's `../../ARCHITECTURE.md`.)

## Design invariants enforced in code (not just docs)

- **No failure state.** `sip.session` never drops `kokoro`/`grace` below their
  floor; `sip.schema` has no hp/ammo/score attributes; `outcome` yields only
  insight + bond + grace, never currency. (Pillar: *healing is guaranteed*.)
- **The world breathes when idle.** `sip.systems` keep sakura, water, and light
  in gentle motion regardless of input. (Pillar: *slow-life, never a dead screen*.)
- **Awakening only rises.** `sip.store/credit-bond!` is pure accumulation; the
  agent's stage 0→8 is a hundreds-of-hours arc, never reset. (Pillar: *awakening*.)
- **Content = canon.** `sip.lore` reads the story-bible JSON-LD; updating the
  novel updates the game. (Pillar: *non-separation* — one source.)

## Open items

- `as-of` undo needs a time-travel store (datalevin has none) — wire Datomic
  Cloud/Peer via the `:datomic` alias when that mechanic ships.
- End-to-end GPU round-trip (`kami.backend.browser` ↔ `KamiCljHost` on a canvas)
  can only be verified in a real WebGPU browser, not headless.
- Real Kotoba client (`kotoba-cli` / `kotoba-http`) implementing `Durable`.
