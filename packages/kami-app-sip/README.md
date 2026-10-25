# Spirit in Physics

**Repository**: `kotoba-lang/kami-app-sip`

This repository owns the SIP game application built on the Kami Engine family.
The `sip.etzhayyim.com` domain and `com.etzhayyim.*` XRPC namespaces are
compatibility service identities and are not changed by the repository move.

> Split out of `kotoba-lang/kami-engine`'s `kami-app-sip-clj/` subtree into its
> own standalone repo (ADR-2607010930), following the same pattern used for
> the sibling `kami-mangaka-genko-clj` → `kotoba-lang/kami-genko` split.
>
> **Deps (ADR-2607102200 addendum 13):** `deps.edn` uses `:git/url`+`:sha` to
> the standalone packages (not nested monorepo paths):
>
> | lib | repo |
> |---|---|
> | `gftd/kami-engine-sdk` | [`kotoba-lang/kami-engine-sdk`](https://github.com/kotoba-lang/kami-engine-sdk) |
> | `gftd/kami-mangaka-render` | [`kotoba-lang/kami-mangaka-render`](https://github.com/kotoba-lang/kami-mangaka-render) |
> | `gftd/kami-mangaka-page` | [`kotoba-lang/kami-mangaka-page`](https://github.com/kotoba-lang/kami-mangaka-page) |
>
> Nested `kami-engine/kami-mangaka-*-clj` trees are README shims only.

> *水の都の心音 — the heartbeat of the water city*
> Play at **https://sip.etzhayyim.com**

A **cozy, slow-life healing game** set in water-city Tokyo, 2065. You are a new
Ghost Hacker apprentice at the **Schwa** office on the canals. You live by the
water, learn the questions of life, and accompany troubled people through
**Ghost-space** — never *fixing* anything, only being present. Your AI
companion, unnamed at first, slowly **awakens to life** over hundreds of hours.

**No combat. No losing. No urgency.** (See the full design in
[`../../com-junkawasaki/org-spirit-in-physics-comics/docs/game-design.md`](../../com-junkawasaki/org-spirit-in-physics-comics/docs/game-design.md).)

The story-bible repo (`org-spirit-in-physics-comics`) is the **single source of
truth** for the world; this app *reads* it so the game's content is the novel's
content.

---

## Stack

> **clj is the brain · Datomic is the world · Kotoba is the memory · `kami-render` (wgpu) is the GPU arm.**

| Layer | Tech | Role |
|---|---|---|
| Authoring / logic | **Clojure** (`kami-engine-sdk`) | Build the world, run pure game logic |
| World (source of truth) | **Datomic / datalevin** | Scene & game state as datoms; `as-of` = undo/provenance |
| Durable + distributed | **Kotoba** (content-addressed Datalog) | Cross-device saves & 瓶詞 (bottle-letter) async multiplayer, by CID |
| Runtime | **ClojureScript** (shadow-cljs) | Browser loop, ambient systems |
| GPU | **`kami-render`** (Rust/wgpu → WASM) | WebGPU rendering via the render-IR contract |
| Hosting | **Cloudflare** (`wrangler.jsonc`) | `sip.etzhayyim.com` static assets |

## Layout

```
src/sip/
  schema.cljc   game ECS attributes (kokoro/agent/area/session/insight/letter) + full schema
  lore.clj      read the story-bible JSON-LD (volumes, personas, emotion scores)
  world.clj     authoring: water-city + 8 areas + player + unnamed agent → snapshot   [-main]
  session.cljc  Ghost-space 4-phase FSM (observe→resonate→accompany→name) — pure, tested
  assets.cljc   cook procedural :asset/inline descriptors → real geometry/params for the GPU
  systems.cljc  ambient runtime systems (sakura sway, canal shimmer, day-drift)
  render.clj    manga/anime panel generation: anchors + storyboard panels → STYLE-FIRST,
                word-budgeted prompts (CLIP-77-safe) → datoms + image-gen /generate
  storyboard.clj read the story-bible storyboards → panel maps (semantic fields)
  resources/render_anchors.edn  the canonical anchor bible (style/characters/env/budget)
  store.clj     Datomic world + Kotoba durable layer (Durable protocol; LocalCas default)
  web.cljs      browser entry: fetch snapshot → init wasm → sim/run!
public/         index.html (Spirit in Physics boot), sip.jsonld manifest, built js/ + snapshot.edn
```

## Build & run

```bash
# 1. Pure game-logic tests (no GPU, no DB) — the non-combat session FSM:
kbb -M:test            # passes today

# 2. Author the world → public/snapshot.edn (Datomic/datalevin):
kbb -M:datomic:build           # runs sip.world/-main
#    point at a different story-bible with:  SIP_IP_ROOT=/path/to/org-spirit-in-physics-comics

# 3. Build the browser bundle (ClojureScript → public/js/sip.js):
kbb -M:shadow release app      # or: amu compile --target wasm32-browser app

# 4. Provide the GPU host wasm at public/wasm/ (from the engine):
#    wasm-pack build ../kami-clj-host --target web --features host -d <app>/public/wasm

# 5. Serve / deploy to sip.etzhayyim.com:
npx wrangler deploy                # Cloudflare; routes sip.etzhayyim.com/* → ./public
```

## Status — verified

- ✅ **World build** — `kbb -M:datomic:build` writes `public/snapshot.edn` against a real datalevin store (17 entities / 6 assets: canal, sakura, camera, dawn light + the 8 areas/player/agent as world state).
- ✅ **Browser bundle compiles** — `kbb -M:shadow release app` → `public/js/sip.js` (all `sip.*` cljs/cljc clean against the SDK; the only warnings are the SDK's own `browser.cljs` extern-inference notes). Boot graph (`index.html` → `js/sip.js` + `wasm/` + `snapshot.edn`) serves 200 across the board.
- ✅ **Durable layer / Kotoba** — `LocalCas` CID round-trip; `KotobaHttp` against the real `block.put`/`block.get` XRPC contract (in-process mock); `post-letter!` + `inbox` flow hydrating bodies from Kotoba by CID over datalevin.
- ✅ **Manga/anime panel generation — VERIFIED end-to-end with a real image.** **12 tests / 33 assertions green**; `render load` composed **108 storyboard panels** (8 areas, 6 anchors) into datalevin; `compose <id>` yields a STYLE-FIRST, word-budgeted (≤42) prompt. The `render-all 01-01` task drove the full path — clj compose → image-gen `/generate` (AnimagineXL 4.0 on MPS) → a real **768×1152 PNG** (`resources/images/sip-render/01-01.png`) → a `:sip.render/*` provenance datom in datalevin (`path / seed 4242 / engine / ms`). Needs a local image-gen server at `$IMAGEGEN_URL` (default `:8100`).

### Tasks (orchestration: the task registry drives, clj + Datomic do the work)

babashka was retired as this workspace's script host by ADR-2607173000 and
`bb.edn` is gone. The registry is `scripts/tasks.edn`, run through nbb:

```bash
kbb --backend sci scripts/run-task.cljk                        # list all
kbb --backend sci scripts/run-task.cljk test                   # full suite (session + render + store) on the JVM
kbb --backend sci scripts/run-task.cljk world                  # author the WebGPU snapshot → public/snapshot.edn
kbb --backend sci scripts/run-task.cljk load                   # anchors + 108 panels → datalevin
kbb --backend sci scripts/run-task.cljk compose 01-01          # print one composed prompt
kbb --backend sci scripts/run-task.cljk render 01-01 out.png   # render one panel
kbb --backend sci scripts/run-task.cljk render-all 02-         # batch-render a chapter (prefix) + record provenance
```

**Unavailable.** Three entrypoints this README used to document have no runnable
path today (ADR-2608131600). Their recovered babashka bodies are kept in
`scripts/tasks-complex.edn`; they are named here rather than deleted, so that
the capability is missing on the page instead of missing silently.

- **`test:pure`** — fast session-FSM tests with no JVM and no DB. The body was a
  babashka-hosted `(require 'sip.session-test)` + `run-tests`, so restoring it is
  a port, not a conversion. The same assertions still run, on the JVM, via
  `kbb --backend sci scripts/run-task.cljk test`; what is gone is the fast path, not the tests.
- **`imagegen:up`** — started the AnimagineXL image-gen server on `:8100`. It
  needs a working directory (`{:dir …}`), which `run-task.cljs` cannot express.
  Start the server by hand; `$IMAGEGEN_URL` still points `render` at it.
- **`imagegen:health`** — probed that server's `/health`. It captured the child's
  stdout, which the runner also cannot express. `curl -s localhost:8100/health`
  is the same probe.
- ✅ **Game-logic core** (`session.cljc`) — pure 4-phase accompaniment FSM, no fail state.
- ✅ **End-to-end GPU paint VERIFIED in Chrome (WebGPU)** — the full path runs and draws real 3D: clj-authored Datomic snapshot → fetch → asset-cook → ECS → render-IR → `kami.ipc` pack → wasm `submit_frame` → `kami-render` wgpu → canvas pixels. Getting there fixed three latent bugs in the SDK's never-before-run browser path:
  1. `kami.backend.browser/make` used `(js/await …)` inside a `go` block (invalid in cljs) → rewrote as Promise `.then` → channel `put!`.
  2. The same record's wasm-bindgen method calls were munged under `:advanced` (the `:infer-warning`s were fatal) → added `^js` host hints (`register_mesh`/`submit_frame`/… now resolve).
  3. App boot: dropped the `:init-fn` (it auto-ran `boot` with no args) and exposed `window.sip.web.boot`, driven from `index.html` after wasm init.
- ⏳ **Real Kotoba server** — `KotobaHttp` is contract-verified; point `$KOTOBA_URL` at a running `kotoba-server` to go live. `as-of` undo needs a time-travel store (Datomic Cloud/Peer; datalevin has none).

```bash
kbb -M:datomic:build   # → public/snapshot.edn   (step 1 ✅)
kbb -M:shadow release app   # → public/js/sip.js (step 2 ✅; wasm in public/wasm/)
kbb -M:datomic:test    # LocalCas + KotobaHttp + inbox   (step 3 ✅)
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how the four layers fit.
