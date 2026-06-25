//! # kami-script-runtime
//!
//! WASM host that binds every `kami:engine/*` WIT import to live Rust
//! game-engine state, then drives the compiled Clojure game-script lifecycle:
//!
//! ## WASM backend (ADR-0037)
//!
//! The same host-binding code drives two interchangeable execution backends,
//! selected by cargo feature — their APIs mirror each other closely enough that
//! only module instantiation and the error type differ:
//!
//! - `backend-wasmtime` (default) — JIT. macOS / Linux / Windows / Android.
//! - `backend-wasmi` — pure interpreter, **no runtime codegen** → iOS / PS5 /
//!   Switch, where JIT (W^X) is forbidden. Slower, but gameplay is not the hot
//!   path (physics/render stay native).
//!
//! Because the guest ABI is the all-i64 deterministic model and the PRNG is
//! host-seeded, **both backends produce bit-identical runs** — so lockstep
//! co-op, replay, and headless golden-frame CI hold across a heterogeneous
//! fleet (a wasmtime desktop host and a wasmi console host stay in sync).
//!
//! ```text
//! compile phase  (kami-clj)          runtime phase  (this crate)
//! ─────────────────────────          ──────────────────────────────────
//! .clj source                        KamiScriptRuntime::new(world)
//!    │                                   │
//!    ▼                                   ▼
//! .wasm module  ──────────────────►  load_wasm(&bytes)
//!                                        │
//!                                    call_init()          (once)
//!                                    call_tick(dt_ms)     (every frame)
//!                                    call_event(k,p,l)    (input events)
//! ```
//!
//! ## Type ABI
//!
//! All-i64 stack model in the *guest* (kami-clj compiler), but at the WASM
//! import boundary the codegen lowers F32 values via
//! `i32.wrap_i64 + f32.reinterpret_i32`, so the host functions receive and
//! return **actual `f32`** values — not bit-pattern i64s.  Entity IDs and
//! string handles remain `i64` / `(i32, i32)`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// WASM backend aliases — one host-binding codebase over two engines.
//
// wasmi's API mirrors wasmtime's (Engine / Linker / Module / Store / Caller /
// Extern / Instance + func_wrap / get_typed_func / TypedFunc::call), so the
// ~40 host closures below compile unchanged against whichever the feature
// selects. The only divergences (module instantiation, error type) are
// `#[cfg]`-gated at their few call sites. `backend-wasmi` wins if both are on,
// so `--features backend-wasmi` works without `--no-default-features`.
// ---------------------------------------------------------------------------
#[cfg(feature = "backend-wasmi")]
use wasmi::{Caller, Engine, Extern, Instance, Linker, Module, Store};
#[cfg(all(feature = "backend-wasmtime", not(feature = "backend-wasmi")))]
use wasmtime::{Caller, Engine, Extern, Instance, Linker, Module, Store};

#[cfg(not(any(feature = "backend-wasmtime", feature = "backend-wasmi")))]
compile_error!(
    "kami-script-runtime needs a WASM backend: enable `backend-wasmtime` or `backend-wasmi`."
);

/// Name of the active WASM backend (`"wasmtime"` or `"wasmi"`) — for logging.
pub const BACKEND: &str = if cfg!(feature = "backend-wasmi") {
    "wasmi"
} else {
    "wasmtime"
};

use kami_core::actor::components::{Position, Rotation, Velocity};

pub use kototama::CljError;

pub mod input_map;
pub use input_map::{ButtonEdges, Edges, VirtualStick, apply_dead_zone};

pub mod platform;
pub use platform::{InputDefault, LogicHost, PlatformSpec, RenderBackend, Target, TexFmt};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("compile error: {0}")]
    Compile(#[from] CljError),
    #[cfg(all(feature = "backend-wasmtime", not(feature = "backend-wasmi")))]
    #[error("wasm backend error: {0}")]
    Backend(#[from] wasmtime::Error),
    #[cfg(feature = "backend-wasmi")]
    #[error("wasm backend error: {0}")]
    Backend(#[from] wasmi::Error),
    #[error("module `{0}` not loaded")]
    NotLoaded(String),
    #[error("missing export `{0}` in module `{1}`")]
    MissingExport(String, String),
}

// wasmi's `Linker::func_wrap` yields a `LinkerError` (not `wasmi::Error`), so the
// `?` in each `bind_*` needs this bridge. wasmtime's `func_wrap` already yields
// `wasmtime::Error`, covered by the `#[from]` above.
#[cfg(feature = "backend-wasmi")]
impl From<wasmi::errors::LinkerError> for RuntimeError {
    fn from(e: wasmi::errors::LinkerError) -> Self {
        RuntimeError::Backend(e.into())
    }
}

// ---------------------------------------------------------------------------
// Host state
// ---------------------------------------------------------------------------

/// Shared mutable engine state visible to every host-bound function.
pub struct HostState {
    /// Live ECS world — game entities live here.
    pub world: Arc<Mutex<hecs::World>>,

    // --- Entity registry ---------------------------------------------------
    /// name → hecs Entity  (for `(spawn-entity "player")`)
    pub entity_registry: HashMap<String, hecs::Entity>,
    /// entity.id() → hecs Entity  (reverse lookup from guest-side i64 handle)
    pub entity_by_id: HashMap<u32, hecs::Entity>,

    // --- Input snapshot (written by the engine before each tick) -----------
    /// Keys currently held (web `code` strings, e.g. `"ArrowRight"`).
    pub keys_down: HashSet<String>,
    /// Keys pressed *this frame* — cleared automatically after each tick.
    pub keys_pressed: HashSet<String>,
    /// Named axis values, e.g. `("horizontal", 1.0)` for d-pad right.
    pub axes: HashMap<String, f32>,
    pub pointer_x: f32,
    pub pointer_y: f32,

    // --- Queues (drained by the engine after each tick) --------------------
    pub audio_queue: Vec<(String, [f32; 3])>,
    pub draw_queue: Vec<DrawCommand>,

    // --- Binaural listener pose (set-listener!): [px,py,pz, fx,fy,fz] -------
    /// Read by the audio backend (kami-audio) to spatialize `audio_queue`.
    pub listener: [f32; 6],
    // --- Active ray-tracing recipe (rt-enable!) ----------------------------
    /// Name of the kami.rt recipe for this frame; `None` = raster path.
    pub rt_recipe: Option<String>,

    // --- Time counters (written by the engine before each tick) ------------
    pub delta_ms: i64,
    pub elapsed_ms: i64,
    pub tick_n: i64,

    // --- ECS query cursors (survivors core loop) ---------------------------
    /// Open `query-begin` cursors: handle → remaining entity ids (popped by
    /// `query-next`, removed when drained). `doseq-entities` always drains, so
    /// these don't accumulate across a normal tick.
    pub query_cursors: HashMap<i64, Vec<u32>>,
    /// Next cursor handle to hand out.
    pub next_query: i64,

    // --- Seeded PRNG (deterministic co-op / replay) ------------------------
    /// xorshift64 state. The host owns the seed so the same seed + inputs
    /// replay identically (shared-seed co-op, async race/ghost, anti-cheat).
    pub rng: u64,
}

#[derive(Debug)]
pub struct DrawCommand {
    pub mesh: String,
    pub pos: [f32; 3],
}

/// Entity tag = the kind string it was spawned with (`(spawn-entity "enemy")`).
/// `query-begin` / `count-tagged` / `nearest-tagged` filter on an exact match,
/// so a script queries the same tag it spawned with.
pub struct Tag(pub String);

impl HostState {
    pub fn new(world: Arc<Mutex<hecs::World>>) -> Self {
        Self {
            world,
            entity_registry: HashMap::new(),
            entity_by_id: HashMap::new(),
            keys_down: HashSet::new(),
            keys_pressed: HashSet::new(),
            axes: HashMap::new(),
            pointer_x: 0.0,
            pointer_y: 0.0,
            audio_queue: Vec::new(),
            draw_queue: Vec::new(),
            listener: [0.0, 0.0, 0.0, 0.0, 0.0, -1.0],
            rt_recipe: None,
            delta_ms: 0,
            elapsed_ms: 0,
            tick_n: 0,
            query_cursors: HashMap::new(),
            next_query: 1,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

pub struct KamiScriptRuntime {
    engine: Engine,
    linker: Linker<HostState>,
    store: Store<HostState>,
    modules: HashMap<String, (Module, Instance)>,
    /// `<name>-tick` exports in WASM export-section order (= CLJ definition order),
    /// read from the module bytes at load. Engine-independent: `Module::exports()`
    /// iteration order is NOT equal across wasmtime (section order) and wasmi
    /// (alphabetical), which silently reorders systems and breaks determinism.
    system_order: HashMap<String, Vec<String>>,
}

/// Read a LEB128 unsigned int at `off`; returns (value, next-offset).
fn read_uleb(b: &[u8], mut off: usize) -> (u64, usize) {
    let (mut val, mut shift) = (0u64, 0u32);
    while off < b.len() {
        let byte = b[off];
        off += 1;
        val |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    (val, off)
}

/// Names of exports ending in `-tick`, in export-section (definition) order.
/// Hand-parses the WASM export section (id 7) so the order is the module's own,
/// not whatever the engine's `exports()` iterator happens to yield.
fn ordered_tick_exports(wasm: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    if wasm.len() < 8 {
        return names;
    }
    let mut i = 8; // skip magic(4) + version(4)
    while i < wasm.len() {
        let id = wasm[i];
        i += 1;
        let (size, ni) = read_uleb(wasm, i);
        i = ni;
        let end = (i + size as usize).min(wasm.len());
        if id == 7 {
            let (count, mut j) = read_uleb(wasm, i);
            for _ in 0..count {
                let (nlen, nj) = read_uleb(wasm, j);
                let s = j.min(wasm.len());
                let e = (nj + nlen as usize).min(wasm.len());
                let name = std::str::from_utf8(&wasm[nj.min(wasm.len())..e])
                    .unwrap_or("")
                    .to_string();
                let _ = s;
                j = e;
                j += 1; // export kind byte
                let (_idx, jj) = read_uleb(wasm, j);
                j = jj;
                if name.ends_with("-tick") {
                    names.push(name);
                }
            }
            break;
        }
        i = end;
    }
    names
}

impl KamiScriptRuntime {
    /// Create a runtime backed by the given shared ECS world.
    pub fn new(world: Arc<Mutex<hecs::World>>) -> Result<Self, RuntimeError> {
        let engine = Engine::default();
        let mut linker: Linker<HostState> = Linker::new(&engine);
        let store = Store::new(&engine, HostState::new(world));

        bind_scene(&mut linker)?;
        bind_physics(&mut linker)?;
        bind_input(&mut linker)?;
        bind_render(&mut linker)?;
        bind_audio(&mut linker)?;
        bind_time(&mut linker)?;
        bind_random(&mut linker)?;

        Ok(Self {
            engine,
            linker,
            store,
            modules: HashMap::new(),
            system_order: HashMap::new(),
        })
    }

    /// Seed the deterministic PRNG (shared-seed co-op / replay). The value is
    /// forced odd/non-zero so xorshift64 never degenerates.
    pub fn set_seed(&mut self, seed: u64) {
        self.store.data_mut().rng = seed | 1;
    }

    /// Despawn an entity by its guest-side id (e.g. a host-side projectile hit),
    /// cleaning the name/id registries. Returns true if it existed.
    pub fn despawn_id(&mut self, id: u32) -> bool {
        let s = self.store.data_mut();
        if let Some(e) = s.entity_by_id.remove(&id) {
            s.entity_registry.retain(|_, v| *v != e);
            let world = s.world.clone();
            let _ = world.lock().unwrap().despawn(e);
            true
        } else {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Input snapshot setters — call these from the engine's input handler
    // -----------------------------------------------------------------------

    /// Mark a key as held (`true`) or released (`false`).
    pub fn set_key_down(&mut self, key: &str, down: bool) {
        if down {
            self.store.data_mut().keys_down.insert(key.to_string());
        } else {
            self.store.data_mut().keys_down.remove(key);
        }
    }

    /// Record a key-press event (cleared after the next tick).
    pub fn set_key_pressed(&mut self, key: &str) {
        self.store.data_mut().keys_pressed.insert(key.to_string());
    }

    /// Set a named axis value (e.g. from a gamepad or virtual d-pad).
    pub fn set_axis(&mut self, name: &str, value: f32) {
        self.store.data_mut().axes.insert(name.to_string(), value);
    }

    /// Update the screen-space pointer position.
    pub fn set_pointer(&mut self, x: f32, y: f32) {
        let s = self.store.data_mut();
        s.pointer_x = x;
        s.pointer_y = y;
    }

    /// Drop a mapped `[x, y]` stick value (from [`VirtualStick::axes`] or
    /// [`apply_dead_zone`]) into two named axes — the bridge from a platform's
    /// raw device to the abstract `(axis "MoveX")` / `(axis "MoveY")` the game
    /// reads. This is the host side of ADR-0037 seam #3: iOS/Android feed a
    /// touch `VirtualStick`, consoles feed `apply_dead_zone`'d gamepad sticks,
    /// and the same `.clj` consumes either.
    pub fn feed_stick(&mut self, x_action: &str, y_action: &str, axes: [f32; 2]) {
        self.set_axis(x_action, axes[0]);
        self.set_axis(y_action, axes[1]);
    }

    /// Feed this frame's held buttons (by abstract action name) through an
    /// [`ButtonEdges`] detector into the runtime: every held action reads true
    /// for `(key-down? …)`, and newly-pressed actions also fire once for
    /// `(key-pressed? …)`. Released actions drop their held state. Call once per
    /// frame before `call_systems`. The host side of ADR-0037 seam #3 for
    /// buttons — DualSense / Joy-Con / MFi / touch-taps all arrive as a name set.
    pub fn feed_buttons(&mut self, edges: &mut ButtonEdges, held: &[&str]) {
        let e = edges.update(held);
        for k in held {
            self.set_key_down(k, true);
        }
        for k in &e.released {
            self.set_key_down(k, false);
        }
        for k in &e.pressed {
            self.set_key_pressed(k);
        }
    }

    // -----------------------------------------------------------------------
    // Module lifecycle
    // -----------------------------------------------------------------------

    /// Compile and load a Clojure source string (GAME_PRELUDE is prepended).
    pub fn load_clj(&mut self, name: &str, src: &str) -> Result<(), RuntimeError> {
        let wasm = kototama::compile_game_typed(src)?;
        self.load_wasm(name, &wasm)
    }

    /// Load a pre-compiled WASM core module.
    pub fn load_wasm(&mut self, name: &str, wasm: &[u8]) -> Result<(), RuntimeError> {
        let module = Module::new(&self.engine, wasm)?;
        // wasmtime hands back an `Instance` directly; wasmi returns an
        // `InstancePre` that must be `.start()`ed to run the module's start fn.
        #[cfg(not(feature = "backend-wasmi"))]
        let instance = self.linker.instantiate(&mut self.store, &module)?;
        #[cfg(feature = "backend-wasmi")]
        let instance = self
            .linker
            .instantiate_and_start(&mut self.store, &module)?;
        self.system_order
            .insert(name.to_string(), ordered_tick_exports(wasm));
        self.modules.insert(name.to_string(), (module, instance));
        Ok(())
    }

    /// Call `init()` on a loaded module (once, right after loading).
    pub fn call_init(&mut self, name: &str) -> Result<(), RuntimeError> {
        let (_, instance) = self
            .modules
            .get(name)
            .ok_or_else(|| RuntimeError::NotLoaded(name.to_string()))?;
        let instance = *instance;
        let f = instance
            .get_typed_func::<(), (i64,)>(&mut self.store, "init")
            .map_err(|_| RuntimeError::MissingExport("init".into(), name.to_string()))?;
        f.call(&mut self.store, ())?;
        Ok(())
    }

    /// Call `tick(dt_ms)` on a loaded module.
    ///
    /// Time counters are updated before the call; `keys_pressed` is cleared after.
    pub fn call_tick(&mut self, name: &str, dt_ms: i64) -> Result<(), RuntimeError> {
        {
            let s = self.store.data_mut();
            s.delta_ms = dt_ms;
            s.elapsed_ms += dt_ms;
            s.tick_n += 1;
        }
        let (_, instance) = self
            .modules
            .get(name)
            .ok_or_else(|| RuntimeError::NotLoaded(name.to_string()))?;
        let instance = *instance;
        let f = instance
            .get_typed_func::<(i64,), (i64,)>(&mut self.store, "tick")
            .map_err(|_| RuntimeError::MissingExport("tick".into(), name.to_string()))?;
        f.call(&mut self.store, (dt_ms,))?;
        let s = self.store.data_mut();
        s.keys_pressed.clear();
        s.query_cursors.clear(); // within-tick ephemeral; reap abandoned cursors

        Ok(())
    }

    /// Run every `defsystem` in a module for one tick.
    ///
    /// `defsystem` compiles to a `<name>-tick` export; this calls all of them
    /// (in module export order, which is definition order), so a game made of
    /// several systems ticks with one call. Time counters advance once;
    /// `keys_pressed` is cleared after.
    pub fn call_systems(&mut self, name: &str, dt_ms: i64) -> Result<(), RuntimeError> {
        {
            let s = self.store.data_mut();
            s.delta_ms = dt_ms;
            s.elapsed_ms += dt_ms;
            s.tick_n += 1;
        }
        // System order = WASM export-section order (CLJ definition order), captured
        // from the module bytes at load. Engine-independent — neither `Module::exports()`
        // nor `Instance::exports()` agree across wasmtime (section order) and wasmi
        // (alphabetical), which silently reorders `spawn`/`ai` and shifts a just-spawned
        // entity by one tick (a real cross-backend determinism bug this fixes at the source).
        let systems = self.system_order.get(name).cloned().unwrap_or_default();
        let instance = self
            .modules
            .get(name)
            .ok_or_else(|| RuntimeError::NotLoaded(name.to_string()))?
            .1;
        for sys in &systems {
            if let Ok(f) = instance.get_typed_func::<(i64,), (i64,)>(&mut self.store, sys) {
                f.call(&mut self.store, (dt_ms,))?;
            }
        }
        let s = self.store.data_mut();
        s.keys_pressed.clear();
        // Query cursors are within-tick ephemeral (a doseq opens + drains one);
        // reap any a guest abandoned early so query_cursors can't grow unbounded.
        s.query_cursors.clear();
        Ok(())
    }

    /// Fixed-step Euler integration: advance every entity's Position by its
    /// Velocity over `dt_ms`. The minimal engine motion step so scripts that
    /// only set velocity (move-toward!, controllers) actually move things.
    pub fn integrate(&mut self, dt_ms: i64) {
        let dt = dt_ms as f32 / 1000.0;
        let world = self.store.data().world.clone();
        let w = world.lock().unwrap();
        for (_, (p, v)) in w.query::<(&mut Position, &Velocity)>().iter() {
            p.0[0] += v.0[0] * dt;
            p.0[1] += v.0[1] * dt;
            p.0[2] += v.0[2] * dt;
        }
    }

    /// Call `on_event(kind, payload_ptr, payload_len)` on a loaded module.
    pub fn call_event(
        &mut self,
        name: &str,
        kind: i32,
        payload: &[u8],
    ) -> Result<i32, RuntimeError> {
        let (_, instance) = self
            .modules
            .get(name)
            .ok_or_else(|| RuntimeError::NotLoaded(name.to_string()))?;
        let instance = *instance;
        // Lower the payload into the guest's linear memory: bump-allocate space via the guest's
        // cabi_realloc, copy the bytes in, and hand on-event the real (ptr,len). The guest reads its
        // payload from there. Empty payload ⇒ (0,0) (the previous always-empty behaviour).
        let (ptr, len) = if payload.is_empty() {
            (0i64, 0i64)
        } else {
            let realloc = instance
                .get_typed_func::<(i32, i32, i32, i32), i32>(&mut self.store, "cabi_realloc")
                .map_err(|_| {
                    RuntimeError::MissingExport("cabi_realloc".into(), name.to_string())
                })?;
            let p = realloc.call(&mut self.store, (0, 0, 16, payload.len() as i32))?;
            let mem = match instance.get_export(&mut self.store, "memory") {
                Some(Extern::Memory(m)) => m,
                _ => {
                    return Err(RuntimeError::MissingExport(
                        "memory".into(),
                        name.to_string(),
                    ));
                }
            };
            let data = mem.data_mut(&mut self.store);
            let start = p as usize;
            let end = start.saturating_add(payload.len());
            if p < 0 || end > data.len() {
                return Err(RuntimeError::MissingExport(
                    "memory (payload did not fit)".into(),
                    name.to_string(),
                ));
            }
            data[start..end].copy_from_slice(payload);
            (p as i64, payload.len() as i64)
        };
        // The kami-engine-clj codegen is all-i64, so the `on-event` export is
        // (i64,i64,i64)->i64 — not the WIT's nominal i32s. Matching that is what
        // makes this work (previously it requested i32s and always missed).
        let f = instance
            .get_typed_func::<(i64, i64, i64), i64>(&mut self.store, "on-event")
            .map_err(|_| RuntimeError::MissingExport("on-event".into(), name.to_string()))?;
        let ret = f.call(&mut self.store, (kind as i64, ptr, len))?;
        Ok(ret as i32)
    }

    /// Drain draw commands accumulated during the last tick.
    pub fn drain_draw_queue(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.store.data_mut().draw_queue)
    }

    /// Drain audio-play commands accumulated during the last tick.
    pub fn drain_audio_queue(&mut self) -> Vec<(String, [f32; 3])> {
        std::mem::take(&mut self.store.data_mut().audio_queue)
    }

    /// Listener pose [px,py,pz, fx,fy,fz] last set via `set-listener!`
    /// (feeds the kami-audio binaural mixer).
    pub fn listener(&self) -> [f32; 6] {
        self.store.data().listener
    }

    /// Active ray-tracing recipe name set via `rt-enable!`, if any
    /// (selects the kami.rt path for this frame).
    pub fn rt_recipe(&self) -> Option<String> {
        self.store.data().rt_recipe.clone()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read a UTF-8 string from guest linear memory via a (ptr, len) pair.
///
/// `get_export` requires `&mut Caller`, so the caller must be taken by mutable ref.
fn read_guest_str(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> String {
    if len <= 0 {
        return String::new();
    }
    let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
        return String::new();
    };
    let data = mem.data(caller);
    let start = ptr as usize;
    let end = start + len as usize;
    if end <= data.len() {
        String::from_utf8_lossy(&data[start..end]).into_owned()
    } else {
        String::new()
    }
}

/// Reconstruct a `hecs::Entity` from the guest-side i64 entity handle.
fn entity_for_id(state: &HostState, eid: i64) -> Option<hecs::Entity> {
    state.entity_by_id.get(&(eid as u32)).copied()
}

// ---------------------------------------------------------------------------
// Scene host bindings
//
// F32 params/returns use the actual `f32` Rust type because the kami-clj
// codegen lowers them with `i32.wrap_i64 + f32.reinterpret_i32` at the call
// site, making the WASM function type `f32` at the boundary.
// ---------------------------------------------------------------------------

fn bind_scene(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/scene@1.0.0";

    // spawn(name_ptr: i32, name_len: i32) -> i64
    linker.func_wrap(
        m,
        "spawn",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i64 {
            let name = read_guest_str(&mut caller, ptr, len);
            let world_arc = caller.data().world.clone();
            let entity = {
                let mut w = world_arc.lock().unwrap();
                let e = w.spawn((
                    Position([0.0, 0.0, 0.0]),
                    Velocity([0.0, 0.0, 0.0]),
                    Rotation([0.0, 0.0, 0.0, 1.0]),
                ));
                // Tag the entity with its kind so query/count/nearest can find it.
                if !name.is_empty() {
                    let _ = w.insert_one(e, Tag(name.clone()));
                }
                e
            };
            let id = entity.id();
            let s = caller.data_mut();
            if !name.is_empty() {
                s.entity_registry.insert(name, entity);
            }
            s.entity_by_id.insert(id, entity);
            id as i64
        },
    )?;

    // despawn(entity: i64)
    linker.func_wrap(
        m,
        "despawn",
        |mut caller: Caller<'_, HostState>, eid: i64| {
            let entity = entity_for_id(caller.data(), eid);
            if let Some(e) = entity {
                let world_arc = caller.data().world.clone();
                let _ = world_arc.lock().unwrap().despawn(e);
                let s = caller.data_mut();
                s.entity_by_id.remove(&(eid as u32));
                s.entity_registry.retain(|_, v| *v != e);
            }
        },
    )?;

    // get-x/y/z(entity: i64) -> f32
    linker.func_wrap(
        m,
        "get-x",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Position>(e)
                        .ok()
                        .map(|p| p.0[0])
                })
                .unwrap_or(0.0)
        },
    )?;
    linker.func_wrap(
        m,
        "get-y",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Position>(e)
                        .ok()
                        .map(|p| p.0[1])
                })
                .unwrap_or(0.0)
        },
    )?;
    linker.func_wrap(
        m,
        "get-z",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Position>(e)
                        .ok()
                        .map(|p| p.0[2])
                })
                .unwrap_or(0.0)
        },
    )?;

    // set-position(entity: i64, x: f32, y: f32, z: f32)
    linker.func_wrap(
        m,
        "set-position",
        |mut caller: Caller<'_, HostState>, eid: i64, x: f32, y: f32, z: f32| {
            let entity = entity_for_id(caller.data(), eid);
            if let Some(e) = entity {
                let world = caller.data_mut().world.clone();
                if let Ok(mut pos) = world.lock().unwrap().get::<&mut Position>(e) {
                    pos.0 = [x, y, z];
                }
            }
        },
    )?;

    // get-vx/vy/vz(entity: i64) -> f32
    linker.func_wrap(
        m,
        "get-vx",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Velocity>(e)
                        .ok()
                        .map(|v| v.0[0])
                })
                .unwrap_or(0.0)
        },
    )?;
    linker.func_wrap(
        m,
        "get-vy",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Velocity>(e)
                        .ok()
                        .map(|v| v.0[1])
                })
                .unwrap_or(0.0)
        },
    )?;
    linker.func_wrap(
        m,
        "get-vz",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Velocity>(e)
                        .ok()
                        .map(|v| v.0[2])
                })
                .unwrap_or(0.0)
        },
    )?;

    // set-velocity(entity: i64, vx: f32, vy: f32, vz: f32)
    linker.func_wrap(
        m,
        "set-velocity",
        |mut caller: Caller<'_, HostState>, eid: i64, vx: f32, vy: f32, vz: f32| {
            let entity = entity_for_id(caller.data(), eid);
            if let Some(e) = entity {
                let world = caller.data_mut().world.clone();
                if let Ok(mut vel) = world.lock().unwrap().get::<&mut Velocity>(e) {
                    vel.0 = [vx, vy, vz];
                }
            }
        },
    )?;

    // get-rx/ry/rz/rw(entity: i64) -> f32
    linker.func_wrap(
        m,
        "get-rx",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Rotation>(e)
                        .ok()
                        .map(|r| r.0[0])
                })
                .unwrap_or(0.0)
        },
    )?;
    linker.func_wrap(
        m,
        "get-ry",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Rotation>(e)
                        .ok()
                        .map(|r| r.0[1])
                })
                .unwrap_or(0.0)
        },
    )?;
    linker.func_wrap(
        m,
        "get-rz",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Rotation>(e)
                        .ok()
                        .map(|r| r.0[2])
                })
                .unwrap_or(0.0)
        },
    )?;
    linker.func_wrap(
        m,
        "get-rw",
        |caller: Caller<'_, HostState>, eid: i64| -> f32 {
            let world = caller.data().world.clone();
            entity_for_id(caller.data(), eid)
                .and_then(|e| {
                    world
                        .lock()
                        .unwrap()
                        .get::<&Rotation>(e)
                        .ok()
                        .map(|r| r.0[3])
                })
                .unwrap_or(1.0) // identity quaternion w = 1
        },
    )?;

    // set-rotation(entity: i64, rx: f32, ry: f32, rz: f32, rw: f32)
    linker.func_wrap(
        m,
        "set-rotation",
        |mut caller: Caller<'_, HostState>, eid: i64, rx: f32, ry: f32, rz: f32, rw: f32| {
            let entity = entity_for_id(caller.data(), eid);
            if let Some(e) = entity {
                let world = caller.data_mut().world.clone();
                if let Ok(mut rot) = world.lock().unwrap().get::<&mut Rotation>(e) {
                    rot.0 = [rx, ry, rz, rw];
                }
            }
        },
    )?;

    // ---- ECS queries (survivors core loop) --------------------------------

    // query-begin(tag_ptr, tag_len) -> cursor handle (i64). Snapshots the ids
    // of all entities tagged `tag`; the cursor is drained by query-next.
    linker.func_wrap(
        m,
        "query-begin",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i64 {
            let tag = read_guest_str(&mut caller, ptr, len);
            let world = caller.data().world.clone();
            let ids: Vec<u32> = {
                let w = world.lock().unwrap();
                let mut q = w.query::<&Tag>();
                q.iter()
                    .filter_map(|(e, t)| if t.0 == tag { Some(e.id()) } else { None })
                    .collect()
            };
            let s = caller.data_mut();
            let handle = s.next_query;
            s.next_query += 1;
            s.query_cursors.insert(handle, ids);
            handle
        },
    )?;

    // query-next(handle) -> next entity-id (i64), or -1 when drained (which
    // also frees the cursor).
    linker.func_wrap(
        m,
        "query-next",
        |mut caller: Caller<'_, HostState>, handle: i64| -> i64 {
            let s = caller.data_mut();
            match s.query_cursors.get_mut(&handle) {
                Some(v) => match v.pop() {
                    Some(id) => id as i64,
                    None => {
                        s.query_cursors.remove(&handle);
                        -1
                    }
                },
                None => -1,
            }
        },
    )?;

    // count-tagged(tag_ptr, tag_len) -> i64
    linker.func_wrap(
        m,
        "count-tagged",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i64 {
            let tag = read_guest_str(&mut caller, ptr, len);
            let world = caller.data().world.clone();
            let w = world.lock().unwrap();
            let mut q = w.query::<&Tag>();
            q.iter().filter(|(_, t)| t.0 == tag).count() as i64
        },
    )?;

    // nearest(tag_ptr, tag_len, x: f32, y: f32, maxd: f32) -> entity-id, or -1.
    // 2D (x,y) broadphase done host-side so scripts need no f32 math.
    linker.func_wrap(
        m,
        "nearest",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32, x: f32, y: f32, maxd: f32| -> i64 {
            let tag = read_guest_str(&mut caller, ptr, len);
            let world = caller.data().world.clone();
            let w = world.lock().unwrap();
            let max2 = maxd * maxd;
            let mut best: Option<(u32, f32)> = None;
            let mut q = w.query::<(&Tag, &Position)>();
            for (e, (t, p)) in q.iter() {
                if t.0 != tag {
                    continue;
                }
                let dx = p.0[0] - x;
                let dy = p.0[1] - y;
                let d2 = dx * dx + dy * dy;
                if d2 <= max2 && best.map_or(true, |(_, bd)| d2 < bd) {
                    best = Some((e.id(), d2));
                }
            }
            best.map(|(id, _)| id as i64).unwrap_or(-1)
        },
    )?;

    // move-toward(entity, target, speed: f32) — set entity velocity toward
    // target at speed px/s in the XY plane. Host does the normalize×speed math.
    linker.func_wrap(
        m,
        "move-toward",
        |caller: Caller<'_, HostState>, eid: i64, target: i64, speed: f32| {
            let src = entity_for_id(caller.data(), eid);
            let tgt = entity_for_id(caller.data(), target);
            if let (Some(e), Some(t)) = (src, tgt) {
                let world = caller.data().world.clone();
                let w = world.lock().unwrap();
                let sp = w.get::<&Position>(e).ok().map(|p| p.0);
                let tp = w.get::<&Position>(t).ok().map(|p| p.0);
                if let (Some(sp), Some(tp)) = (sp, tp) {
                    let dx = tp[0] - sp[0];
                    let dy = tp[1] - sp[1];
                    let len = (dx * dx + dy * dy).sqrt();
                    let (vx, vy) = if len > 1e-6 {
                        (dx / len * speed, dy / len * speed)
                    } else {
                        (0.0, 0.0)
                    };
                    if let Ok(mut vel) = w.get::<&mut Velocity>(e) {
                        vel.0 = [vx, vy, 0.0];
                    }
                }
            }
        },
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Random host bindings — host-owned seeded PRNG (deterministic co-op)
// ---------------------------------------------------------------------------

fn bind_random(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/random@1.0.0";
    // int(n) -> uniform i64 in [0, n); 0 if n <= 0. xorshift64 advances the
    // host-owned seed so runs are reproducible (set via KamiScriptRuntime::set_seed).
    linker.func_wrap(
        m,
        "int",
        |mut caller: Caller<'_, HostState>, n: i64| -> i64 {
            if n <= 0 {
                return 0;
            }
            let s = caller.data_mut();
            let mut x = s.rng;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s.rng = x;
            (x % (n as u64)) as i64
        },
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Physics host bindings  (stub — rapier integration is Phase 3)
// ---------------------------------------------------------------------------

fn bind_physics(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/physics@1.0.0";
    linker.func_wrap(
        m,
        "apply-impulse",
        |_: Caller<'_, HostState>, _eid: i64, _ix: f32, _iy: f32, _iz: f32| {},
    )?;
    linker.func_wrap(
        m,
        "apply-force",
        |_: Caller<'_, HostState>, _eid: i64, _fx: f32, _fy: f32, _fz: f32| {},
    )?;
    linker.func_wrap(
        m,
        "raycast",
        |_: Caller<'_, HostState>,
         _ox: f32,
         _oy: f32,
         _oz: f32,
         _dx: f32,
         _dy: f32,
         _dz: f32|
         -> i64 { 0 },
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Input host bindings
// ---------------------------------------------------------------------------

fn bind_input(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/input@1.0.0";

    // key-down?(ptr: i32, len: i32) -> i32  (1 = held, 0 = not)
    linker.func_wrap(
        m,
        "key-down",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
            let key = read_guest_str(&mut caller, ptr, len);
            if caller.data().keys_down.contains(&key) {
                1
            } else {
                0
            }
        },
    )?;

    // key-pressed?(ptr: i32, len: i32) -> i32  (1 = pressed this frame)
    linker.func_wrap(
        m,
        "key-pressed",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
            let key = read_guest_str(&mut caller, ptr, len);
            if caller.data().keys_pressed.contains(&key) {
                1
            } else {
                0
            }
        },
    )?;

    // axis(ptr: i32, len: i32) -> f32
    linker.func_wrap(
        m,
        "axis",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> f32 {
            let name = read_guest_str(&mut caller, ptr, len);
            caller.data().axes.get(&name).copied().unwrap_or(0.0)
        },
    )?;

    // pointer-x() -> f32
    linker.func_wrap(m, "pointer-x", |caller: Caller<'_, HostState>| -> f32 {
        caller.data().pointer_x
    })?;
    linker.func_wrap(m, "pointer-y", |caller: Caller<'_, HostState>| -> f32 {
        caller.data().pointer_y
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Render host bindings  (queued — wgpu integration is Phase 3)
// ---------------------------------------------------------------------------

fn bind_render(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/render@1.0.0";

    // draw-mesh(ptr: i32, len: i32, x: f32, y: f32, z: f32)
    linker.func_wrap(
        m,
        "draw-mesh",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32, x: f32, y: f32, z: f32| {
            let mesh = read_guest_str(&mut caller, ptr, len);
            caller.data_mut().draw_queue.push(DrawCommand {
                mesh,
                pos: [x, y, z],
            });
        },
    )?;
    linker.func_wrap(
        m,
        "spawn-particle",
        |_: Caller<'_, HostState>, _ptr: i32, _len: i32, _x: f32, _y: f32, _z: f32| {},
    )?;
    linker.func_wrap(
        m,
        "draw-line",
        |_: Caller<'_, HostState>,
         _x0: f32,
         _y0: f32,
         _z0: f32,
         _x1: f32,
         _y1: f32,
         _z1: f32,
         _color: i64| {},
    )?;
    // rt-enable(ptr: i32, len: i32) — name a kami.rt recipe for this frame.
    linker.func_wrap(
        m,
        "rt-enable",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
            let name = read_guest_str(&mut caller, ptr, len);
            caller.data_mut().rt_recipe = if name.is_empty() { None } else { Some(name) };
        },
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Audio host bindings  (queued — kami-audio integration is Phase 3)
// ---------------------------------------------------------------------------

fn bind_audio(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/audio@1.0.0";

    // play(ptr: i32, len: i32)
    linker.func_wrap(
        m,
        "play",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
            let name = read_guest_str(&mut caller, ptr, len);
            caller.data_mut().audio_queue.push((name, [0.0; 3]));
        },
    )?;
    linker.func_wrap(
        m,
        "stop",
        |_: Caller<'_, HostState>, _ptr: i32, _len: i32| {},
    )?;
    // play-at(ptr: i32, len: i32, x: f32, y: f32, z: f32)
    linker.func_wrap(
        m,
        "play-at",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32, x: f32, y: f32, z: f32| {
            let name = read_guest_str(&mut caller, ptr, len);
            caller.data_mut().audio_queue.push((name, [x, y, z]));
        },
    )?;
    // set-listener(x, y, z, fx, fy, fz) — listener pose for binaural mixing.
    linker.func_wrap(
        m,
        "set-listener",
        |mut caller: Caller<'_, HostState>, x: f32, y: f32, z: f32, fx: f32, fy: f32, fz: f32| {
            caller.data_mut().listener = [x, y, z, fx, fy, fz];
        },
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Time host bindings
// ---------------------------------------------------------------------------

fn bind_time(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/time@1.0.0";
    linker.func_wrap(m, "delta-ms", |caller: Caller<'_, HostState>| -> i64 {
        caller.data().delta_ms
    })?;
    linker.func_wrap(m, "elapsed-ms", |caller: Caller<'_, HostState>| -> i64 {
        caller.data().elapsed_ms
    })?;
    linker.func_wrap(m, "tick", |caller: Caller<'_, HostState>| -> i64 {
        caller.data().tick_n
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Convenience: compile-and-load from a Clojure source string
// ---------------------------------------------------------------------------

/// Compile a Clojure script (with prelude) and immediately call `init()`.
pub fn load_and_init(
    runtime: &mut KamiScriptRuntime,
    name: &str,
    src: &str,
) -> Result<(), RuntimeError> {
    runtime.load_clj(name, src)?;
    runtime.call_init(name)
}

// ---------------------------------------------------------------------------
// Tests — survivors host bindings (query / nearest / move-toward / rand)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> Arc<Mutex<hecs::World>> {
        Arc::new(Mutex::new(hecs::World::new()))
    }

    #[test]
    fn doseq_visits_every_tagged_entity() {
        // doseq-entities + nearest-tagged + move-toward over 3 enemies → all
        // three end the tick moving toward the player. This is the survivors
        // core loop that could not even compile before the kami-clj extension.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init []
              (let [p (spawn-entity "player")]
                (set-position! p (f32 100.0) (f32 0.0) (f32 0.0))
                (spawn-entity "enemy")
                (spawn-entity "enemy")
                (spawn-entity "enemy")))
            (defn tick [dt]
              (doseq-entities [e "enemy"]
                (let [p (nearest-tagged "player" (get-x e) (get-y e) (f32 100000.0))]
                  (when (not= p -1)
                    (move-toward! e p (f32 7.0))))))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();
        rt.call_tick("g", 16).unwrap();

        let world = w.lock().unwrap();
        let mut enemies = 0;
        let mut moving = 0;
        let mut q = world.query::<(&Tag, &Velocity)>();
        for (_, (t, v)) in q.iter() {
            if t.0 == "enemy" {
                enemies += 1;
                // player is at +x, enemies spawned at origin → vx ≈ +7, vy ≈ 0
                if v.0[0] > 6.9 && v.0[0] < 7.1 && v.0[1].abs() < 1e-3 {
                    moving += 1;
                }
            }
        }
        assert_eq!(enemies, 3, "all enemies present");
        assert_eq!(moving, 3, "doseq-entities must visit + move every enemy");
    }

    #[test]
    fn nearest_respects_max_distance() {
        // An enemy outside maxd gets no target (nearest returns -1 → no move).
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init []
              (let [p (spawn-entity "player")
                    e (spawn-entity "enemy")]
                (set-position! p (f32 1000.0) (f32 0.0) (f32 0.0))
                (set-position! e (f32 0.0) (f32 0.0) (f32 0.0))))
            (defn tick [dt]
              (doseq-entities [e "enemy"]
                (let [p (nearest-tagged "player" (get-x e) (get-y e) (f32 50.0))]
                  (when (not= p -1)
                    (move-toward! e p (f32 7.0))))))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();
        rt.call_tick("g", 16).unwrap();

        let world = w.lock().unwrap();
        let mut q = world.query::<(&Tag, &Velocity)>();
        for (_, (t, v)) in q.iter() {
            if t.0 == "enemy" {
                assert_eq!(v.0, [0.0, 0.0, 0.0], "player out of range → no movement");
            }
        }
    }

    #[test]
    fn rand_int_is_seed_deterministic() {
        // Same seed → identical spawn decisions (shared-seed co-op / replay).
        let run = |seed: u64| -> usize {
            let w = world();
            let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
            rt.set_seed(seed);
            let src = r#"
                (defn init [] 0)
                (defn tick [dt]
                  (when (zero? (mod (rand-int 100) 3))
                    (spawn-entity "hit")))
            "#;
            rt.load_clj("g", src).unwrap();
            rt.call_init("g").unwrap();
            for _ in 0..20 {
                rt.call_tick("g", 16).unwrap();
            }
            let world = w.lock().unwrap();
            let mut q = world.query::<&Tag>();
            q.iter().filter(|(_, t)| t.0 == "hit").count()
        };
        let a = run(42);
        assert_eq!(a, run(42), "same seed must replay identically");
        // sanity: the PRNG actually fired some-but-not-all of 20 ticks
        assert!(
            a > 0 && a < 20,
            "expected a non-trivial spawn count, got {a}"
        );
    }

    #[test]
    fn vec_state_bag_drives_spawns_at_runtime() {
        // Phase-4 vector executes (not just compiles): push 5 and 7 into a state
        // bag, read them back, and spawn their sum (12) — proving vec-make /
        // vec-push! / vec-get round-trip through guest linear memory. Runs under
        // whichever backend is compiled, so the dual-backend gate exercises both.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init []
              (let [v (vec-make 8)]
                (vec-push! v 5)
                (vec-push! v 7)
                (let [n (+ (vec-get v 0) (vec-get v 1))]
                  (loop [i 0]
                    (when (< i n)
                      (spawn-entity "blip")
                      (recur (+ i 1)))))))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();

        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        let blips = q.iter().filter(|(_, t)| t.0 == "blip").count();
        assert_eq!(blips, 12, "vec-get(0)+vec-get(1) = 5+7 = 12 spawns");
    }

    #[test]
    fn map_assoc_bag_drives_spawns_at_runtime() {
        // Phase-4 map executes (not just compiles): put two keys, UPDATE one
        // in place (100→4), miss a third (get-or default), and spawn the result.
        // Proves map-put! insert+update, map-get, and map-get-or default all
        // round-trip through guest memory under whichever backend is compiled.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init []
              (let [m (map-make 8)]
                (map-put! m 100 3)
                (map-put! m 200 7)
                (map-put! m 100 (+ (map-get m 100) 1))
                (let [n (+ (map-get m 100) (map-get-or m 999 0))]
                  (loop [i 0]
                    (when (< i n)
                      (spawn-entity "kv")
                      (recur (+ i 1)))))))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();

        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        let kv = q.iter().filter(|(_, t)| t.0 == "kv").count();
        assert_eq!(
            kv, 4,
            "100→3 then updated to 4; missing 999→0; 4+0 = 4 spawns"
        );
    }

    #[test]
    fn defentity_template_spawns_and_inits_at_runtime() {
        // Phase-4 defentity executes: each `(enemy x)` call spawns a fresh
        // entity tagged "enemy", runs the body to set its position via `self`,
        // and returns it. Proves the spawn-self-init-return desugar end-to-end.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defentity enemy [x]
              (set-position! self x (f32 0.0) (f32 0.0)))
            (defn init []
              (enemy (f32 10.0))
              (enemy (f32 20.0))
              (enemy (f32 30.0)))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();

        let world = w.lock().unwrap();
        let mut xs: Vec<f32> = world
            .query::<(&Tag, &Position)>()
            .iter()
            .filter(|(_, (t, _))| t.0 == "enemy")
            .map(|(_, (_, p))| p.0[0])
            .collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            xs,
            vec![10.0, 20.0, 30.0],
            "3 enemies spawned + positioned via self"
        );
    }

    #[test]
    fn virtual_stick_drives_guest_axis_at_runtime() {
        // ADR-0037 seam #3 end-to-end: a touch on a VirtualStick → feed_stick →
        // the guest reads (axis "MoveX"/"MoveY") and sets the player's velocity.
        // The same .clj would run on iOS (touch) or a console (gamepad); only the
        // host-side device→axis mapping differs. Verified on both backends.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init [] (spawn-entity "player"))
            (defsystem drive [dt]
              (doseq-entities [e "player"]
                (set-velocity! e (axis "MoveX") (axis "MoveY") (f32 0.0))))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();

        // Touch at full-right of a stick centred at (100,100), radius 50.
        let stick = VirtualStick::new([100.0, 100.0], 50.0);
        rt.feed_stick("MoveX", "MoveY", stick.axes([150.0, 100.0]));
        rt.call_systems("g", 16).unwrap();

        let world = w.lock().unwrap();
        let mut q = world.query::<(&Tag, &Velocity)>();
        let (_, (_, v)) = q.iter().find(|(_, (t, _))| t.0 == "player").unwrap();
        assert!(
            (v.0[0] - 1.0).abs() < 1e-4,
            "full-right touch → vx≈1, got {:?}",
            v.0
        );
        assert!(v.0[1].abs() < 1e-4, "no vertical → vy≈0, got {:?}", v.0);
    }

    #[test]
    fn button_edges_drive_guest_key_semantics_at_runtime() {
        // ADR-0037 seam #3 (buttons) end-to-end: feed_buttons drives the guest's
        // (key-down? "Fire") as a LEVEL (spawns each frame held) and
        // (key-pressed? "Jump") as an EDGE (spawns once on the down frame).
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init [] 0)
            (defsystem guns [dt]
              (when (key-down? "Fire")    (spawn-entity "shot"))
              (when (key-pressed? "Jump") (spawn-entity "jump")))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();

        let mut edges = ButtonEdges::new();
        // frame 0: Fire down (level) → shot. Jump absent.
        rt.feed_buttons(&mut edges, &["Fire"]);
        rt.call_systems("g", 16).unwrap();
        // frame 1: Fire still held → shot. Jump newly pressed (edge) → jump.
        rt.feed_buttons(&mut edges, &["Fire", "Jump"]);
        rt.call_systems("g", 16).unwrap();
        // frame 2: both held → shot only (Jump's edge is spent).
        rt.feed_buttons(&mut edges, &["Fire", "Jump"]);
        rt.call_systems("g", 16).unwrap();
        // frame 3: all released → nothing.
        rt.feed_buttons(&mut edges, &[]);
        rt.call_systems("g", 16).unwrap();

        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        let (mut shots, mut jumps) = (0, 0);
        for (_, t) in q.iter() {
            match t.0.as_str() {
                "shot" => shots += 1,
                "jump" => jumps += 1,
                _ => {}
            }
        }
        assert_eq!(shots, 3, "Fire held frames 0,1,2 → 3 shots (level)");
        assert_eq!(jumps, 1, "Jump pressed once on frame 1 → 1 jump (edge)");
    }

    /// Order-independent FNV fold of every entity's (tag-len, id, x-bits, y-bits).
    fn world_hash(w: &Arc<Mutex<hecs::World>>) -> u64 {
        let world = w.lock().unwrap();
        let mut acc: u64 = 0;
        for (e, (t, p)) in world.query::<(&Tag, &Position)>().iter() {
            let mut h: u64 = 0xcbf29ce484222325; // FNV-1a offset
            let mut feed = |x: u32| {
                h ^= x as u64;
                h = h.wrapping_mul(0x100000001b3);
            };
            feed(t.0.len() as u32);
            feed(e.id());
            feed(p.0[0].to_bits());
            feed(p.0[1].to_bits());
            acc = acc.wrapping_add(h); // commutative → independent of iteration order
        }
        acc
    }

    #[test]
    fn golden_frame_determinism() {
        // Seeded RNG + host-side f32 math ⇒ the same script runs identically on
        // EVERY backend. This pins a golden world-state hash after a fixed number
        // of deterministic ticks; the dual-backend gate runs it under wasmtime AND
        // wasmi, so both must hit the SAME constant — the cross-backend determinism
        // proof (replay / lockstep / anti-cheat foundation) without linking both
        // engines in one binary.
        const GAME: &str = r#"
            (defn player [] (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))
            (defn init [] (set-position! (spawn-entity "player") (f32 0.0) (f32 0.0) (f32 0.0)))
            (defsystem spawn [dt]
              (when (< (count-tagged "e") 6)
                (when (zero? (mod (tick-n) 5))
                  (let [r (rand-int 4) e (spawn-entity "e")]
                    (cond
                      (= r 0) (set-position! e (f32 100.0)  (f32 0.0)   (f32 0.0))
                      (= r 1) (set-position! e (f32 -100.0) (f32 0.0)   (f32 0.0))
                      (= r 2) (set-position! e (f32 0.0)    (f32 100.0) (f32 0.0))
                      :else   (set-position! e (f32 0.0)    (f32 -100.0)(f32 0.0)))))))
            (defsystem ai [dt]
              (let [p (player)]
                (when (not= p -1)
                  (doseq-entities [e "e"]
                    (move-toward! e p (f32 50.0))))))
        "#;
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        rt.set_seed(0xD1CE_5EED);
        rt.load_clj("g", GAME).unwrap();
        rt.call_init("g").unwrap();
        for _ in 0..40 {
            rt.call_systems("g", 16).unwrap();
            rt.integrate(16);
        }
        let h = world_hash(&w);
        // This constant is hit by BOTH wasmtime and wasmi (the dual-backend gate runs
        // this test under each), so it is the cross-backend determinism guard. It also
        // catches within-backend drift from any future gameplay/host change.
        const GOLDEN: u64 = 0x5d6e4ebcdfe61ffc;
        assert_eq!(
            h, GOLDEN,
            "world-state hash 0x{h:016x} ≠ GOLDEN — determinism/backend regression"
        );
    }

    /// Compile + run a script that spawns `n-expr` copies of "x", and return the
    /// count — a behavioral probe that the kami-engine-clj compiler EVALUATES the
    /// expression correctly (not just emits valid wasm). Runs on whichever backend
    /// is compiled, so the dual-backend gate checks both interpret it identically.
    fn eval_count(n_expr: &str) -> usize {
        let src = format!(
            "(defn init [] (loop [i 0] (when (< i {n_expr}) (spawn-entity \"x\") (recur (+ i 1)))))"
        );
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        rt.set_seed(1);
        rt.load_clj("g", &src).unwrap();
        rt.call_init("g").unwrap();
        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        q.iter().filter(|(_, t)| t.0 == "x").count()
    }

    #[test]
    fn lang_cond_picks_matching_branch() {
        assert_eq!(eval_count("(cond (= 1 2) 99 (= 2 2) 7 :else 0)"), 7);
        assert_eq!(eval_count("(cond (= 1 2) 99 (= 3 4) 5 :else 4)"), 4); // :else
    }

    #[test]
    fn lang_nested_arithmetic() {
        assert_eq!(eval_count("(* (+ 2 3) 2)"), 10);
        assert_eq!(eval_count("(mod 17 5)"), 2);
        assert_eq!(eval_count("(abs (dec (- 0 7)))"), 8); // dec(-7) = -8, abs = 8
    }

    #[test]
    fn lang_and_or_not() {
        // and(true, or(false,false)) = false → else branch (6)
        assert_eq!(eval_count("(if (and (> 5 3) (or false (< 1 0))) 4 6)"), 6);
        assert_eq!(eval_count("(if (not (= 1 1)) 3 5)"), 5);
        assert_eq!(eval_count("(if (or false (>= 3 3)) 9 0)"), 9);
    }

    #[test]
    fn lang_let_shadowing_and_do() {
        assert_eq!(eval_count("(let [a 4 b (+ a 1)] (do a b))"), 5); // do → last
        assert_eq!(eval_count("(let [a 3] (let [a (* a 2)] a))"), 6); // inner shadow
    }

    #[test]
    fn lang_vec_capacity_is_enforced() {
        // push past cap is a silent no-op; len stays at cap.
        assert_eq!(
            eval_count(
                "(let [v (vec-make 2)] (vec-push! v 1) (vec-push! v 1) (vec-push! v 1) (vec-len v))"
            ),
            2
        );
        // round-trip a stored value
        assert_eq!(
            eval_count("(let [v (vec-make 4)] (vec-push! v 7) (vec-get v 0))"),
            7
        );
    }

    #[test]
    fn lang_map_default_and_update() {
        assert_eq!(eval_count("(map-get-or (map-make 4) 42 9)"), 9); // missing → default
        assert_eq!(
            eval_count(
                "(let [m (map-make 4)] (map-put! m 1 3) (map-put! m 1 (+ (map-get m 1) 2)) (map-get m 1))"
            ),
            5 // in-place update 3 → 5
        );
        assert_eq!(
            eval_count("(let [m (map-make 4)] (map-put! m 1 1) (map-has? m 9))"),
            0
        );
    }

    /// Load + init a script and count entities tagged `tag`. Used by the host
    /// robustness probes below — each spawns "ok" only if a host fn handled a
    /// bad/edge input gracefully (returned the documented sentinel, no panic/trap).
    fn run_init_count(src: &str, tag: &str) -> usize {
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        rt.set_seed(7);
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();
        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        q.iter().filter(|(_, t)| t.0 == tag).count()
    }

    #[test]
    fn host_despawn_then_read_is_zero() {
        // get-x on a despawned entity returns 0.0 (no dangling-handle trap).
        let n = run_init_count(
            r#"(defn init []
                  (let [e (spawn-entity "z")]
                    (despawn-entity e)
                    (when (= (get-x e) (f32 0.0)) (spawn-entity "ok"))))"#,
            "ok",
        );
        assert_eq!(n, 1);
    }

    #[test]
    fn host_unknown_entity_is_graceful() {
        // get-x of a never-spawned id → 0.0; despawn of an unknown id → no-op.
        let n = run_init_count(
            r#"(defn init []
                  (despawn-entity 999999)
                  (when (= (get-x 999999) (f32 0.0)) (spawn-entity "ok")))"#,
            "ok",
        );
        assert_eq!(n, 1, "unknown-entity reads/writes must not trap");
    }

    #[test]
    fn host_move_toward_invalid_target_is_noop() {
        // move-toward! with target -1 must not trap and must leave velocity at 0.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"(defn init []
                       (let [e (spawn-entity "e")]
                         (set-position! e (f32 5.0) (f32 0.0) (f32 0.0))
                         (move-toward! e -1 (f32 50.0))))"#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap(); // must not panic
        let world = w.lock().unwrap();
        let mut q = world.query::<(&Tag, &Velocity)>();
        let (_, (_, v)) = q.iter().find(|(_, (t, _))| t.0 == "e").unwrap();
        assert_eq!(v.0, [0.0, 0.0, 0.0], "invalid target → unchanged velocity");
    }

    #[test]
    fn host_empty_queries_return_sentinels() {
        // nearest of a tag with no entities → -1; count of an empty tag → 0.
        let n = run_init_count(
            r#"(defn init []
                  (when (= (nearest-tagged "ghost" (f32 0.0) (f32 0.0) (f32 100.0)) -1)
                    (spawn-entity "ok"))
                  (when (= (count-tagged "none") 0) (spawn-entity "ok")))"#,
            "ok",
        );
        assert_eq!(n, 2);
    }

    #[test]
    fn host_rand_int_guards_nonpositive() {
        // rand-int n returns 0 for n <= 0 (no divide-by-zero / modulo trap).
        assert_eq!(
            run_init_count(
                r#"(defn init []
                      (when (= (rand-int 0) 0) (spawn-entity "ok"))
                      (when (= (rand-int -5) 0) (spawn-entity "ok")))"#,
                "ok",
            ),
            2
        );
    }

    #[test]
    fn prelude_timer_fires_after_period() {
        // GAME_PRELUDE timer: fires once elapsed ≥ period, then resets. Used by
        // games for cooldowns — previously only compile-tested, now behavioral.
        let src = r#"(defn init []
            (let [t (timer-make 100)]
              (when (= (timer-fired? t) 0) (spawn-entity "a"))   ;; elapsed 0   < 100
              (timer-tick! t 60)
              (when (= (timer-fired? t) 0) (spawn-entity "a"))   ;; elapsed 60  < 100
              (timer-tick! t 60)
              (when (= (timer-fired? t) 1) (spawn-entity "b"))   ;; elapsed 120 ≥ 100 → fire+reset
              (when (= (timer-fired? t) 0) (spawn-entity "c")))) ;; elapsed 0 again
        "#;
        assert_eq!(run_init_count(src, "a"), 2, "not fired before the period");
        assert_eq!(run_init_count(src, "b"), 1, "fires once elapsed ≥ period");
        assert_eq!(run_init_count(src, "c"), 1, "firing resets the timer");
    }

    #[test]
    fn prelude_vec3_and_entity_pos_round_trip() {
        // vec3-make / vec3-y store + read f32 bit-patterns through the heap.
        assert_eq!(
            run_init_count(
                r#"(defn init []
                      (let [v (vec3-make (f32 1.0) (f32 2.0) (f32 3.0))]
                        (when (= (vec3-y v) (f32 2.0)) (spawn-entity "ok"))))"#,
                "ok",
            ),
            1
        );
        // entity-pos reads an entity's transform into a fresh Vec3.
        assert_eq!(
            run_init_count(
                r#"(defn init []
                      (let [e (spawn-entity "p")]
                        (set-position! e (f32 7.0) (f32 0.0) (f32 0.0))
                        (let [v (entity-pos e)]
                          (when (= (vec3-x v) (f32 7.0)) (spawn-entity "ok")))))"#,
                "ok",
            ),
            1
        );
    }

    #[test]
    fn prelude_f32_constants() {
        // The prelude's F32-* bit-pattern constants match `(f32 …)` literals.
        assert_eq!(
            run_init_count(
                r#"(defn init []
                      (when (= (f32 1.0) F32-ONE)  (spawn-entity "ok"))
                      (when (= (f32 0.0) F32-ZERO) (spawn-entity "ok")))"#,
                "ok",
            ),
            2
        );
    }

    #[test]
    fn lang_string_handle_abi() {
        // The all-i64 string ABI: a literal is a packed (offset<<32 | len) handle;
        // str-len / byte-at read it + the linear-memory data section.
        assert_eq!(
            run_init_count(
                r#"(defn init [] (when (= (str-len "hello") 5) (spawn-entity "ok")))"#,
                "ok"
            ),
            1
        );
        // "hello"[1] = 'e' = 101
        assert_eq!(
            run_init_count(
                r#"(defn init [] (when (= (byte-at "hello" 1) 101) (spawn-entity "ok")))"#,
                "ok"
            ),
            1
        );
    }

    #[test]
    fn lang_raw_memory_store_load() {
        // alloc + store64!/load64 and store32!/load32 round-trip through the heap.
        assert_eq!(
            run_init_count(
                r#"(defn init [] (let [p (alloc 8)] (store64! p 42) (when (= (load64 p) 42) (spawn-entity "ok"))))"#,
                "ok",
            ),
            1
        );
        assert_eq!(
            run_init_count(
                r#"(defn init [] (let [p (alloc 4)] (store32! p 7) (when (= (load32 p) 7) (spawn-entity "ok"))))"#,
                "ok",
            ),
            1
        );
    }

    #[test]
    fn lang_loop_recur_accumulator() {
        // multi-binding loop/recur with an accumulator: sum 1..5 = 15.
        assert_eq!(
            run_init_count(
                r#"(defn init []
                      (let [s (loop [i 1 acc 0]
                                (if (<= i 5) (recur (+ i 1) (+ acc i)) acc))]
                        (when (= s 15) (spawn-entity "ok"))))"#,
                "ok",
            ),
            1
        );
    }

    #[test]
    fn call_event_dispatches_on_kind() {
        // The third lifecycle export (init/tick/on-event) was untested — and
        // call_event requested an i32 signature the all-i64 codegen never emits,
        // so it silently never worked. This pins the fixed behavior on both backends:
        // on-event reacts to the event kind.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init [] 0)
            (defn on-event [kind ptr len]
              (when (= kind 5) (spawn-entity "evt"))
              0)
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();
        rt.call_event("g", 3, &[]).unwrap(); // non-matching kind → no spawn
        rt.call_event("g", 5, &[]).unwrap(); // match → spawn
        rt.call_event("g", 5, &[]).unwrap(); // match → spawn
        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        assert_eq!(q.iter().filter(|(_, t)| t.0 == "evt").count(), 2);
    }

    #[test]
    fn waves_game_runs_and_spawns() {
        // End-to-end: the KAMI Waves CLJ game — authored with the expanded compiler forms
        // (-> / clamp / dotimes / case / even? / max) — compiles, loads, and its spawn defsystem
        // actually creates enemies when ticked. Proves the gameplay-in-CLJ path runs, not just builds.
        let src = include_str!("../../kami-clj-play/games/waves/logic.clj");
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        rt.load_clj("waves", src).unwrap();
        rt.call_init("waves").unwrap();
        for _ in 0..40 {
            rt.call_systems("waves", 16).unwrap();
        } // base-period 18 → ≥2 waves
        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        let enemies = q.iter().filter(|(_, t)| t.0 == "enemy").count();
        assert!(
            enemies >= 1,
            "waves spawned {enemies} enemies over 40 ticks"
        );
    }

    #[test]
    fn call_event_lowers_payload() {
        // The payload is now lowered into guest memory via cabi_realloc and on-event receives the
        // real (ptr,len) — previously hardcoded (0,0), so payloads never reached the guest. The guest
        // dispatches on the payload length, proving the bytes were allocated + the length delivered.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init [] 0)
            (defn on-event [kind ptr len]
              (when (= len 3) (spawn-entity "len3"))
              0)
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();
        rt.call_event("g", 1, &[7, 8, 9]).unwrap(); // 3-byte payload → cabi_realloc + len=3
        rt.call_event("g", 1, &[]).unwrap(); // empty → (0,0) → no spawn
        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        assert_eq!(
            q.iter().filter(|(_, t)| t.0 == "len3").count(),
            1,
            "payload lowered into guest memory; its length reached on-event"
        );
    }

    #[test]
    fn golden_frame_with_despawn_determinism() {
        // Extends the determinism guard to a DESPAWN-heavy game (spawn + chase +
        // weapon culls nearest). Despawn touches the entity registry (HashMap
        // remove) and nearest-tagged iterates the world — a second place the
        // system-order class of nondeterminism could hide. Both backends must hit
        // the same GOLDEN, proving cross-backend lockstep holds through despawn.
        const GAME: &str = r#"
            (defn player [] (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 9000000.0)))
            (defn init [] (set-position! (spawn-entity "player") (f32 0.0) (f32 0.0) (f32 0.0)))
            (defsystem spawn [dt]
              (when (zero? (mod (tick-n) 3))
                (let [r (rand-int 4) e (spawn-entity "e")]
                  (cond
                    (= r 0) (set-position! e (f32 80.0)  (f32 0.0)  (f32 0.0))
                    (= r 1) (set-position! e (f32 -80.0) (f32 0.0)  (f32 0.0))
                    (= r 2) (set-position! e (f32 0.0)   (f32 80.0) (f32 0.0))
                    :else   (set-position! e (f32 0.0)   (f32 -80.0)(f32 0.0))))))
            (defsystem ai [dt]
              (let [p (player)]
                (when (not= p -1)
                  (doseq-entities [e "e"] (move-toward! e p (f32 40.0))))))
            (defsystem weapon [dt]
              (when (zero? (mod (tick-n) 4))
                (let [hit (nearest-tagged "e" (f32 0.0) (f32 0.0) (f32 1000.0))]
                  (when (not= hit -1) (despawn-entity hit)))))
        "#;
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        rt.set_seed(0xBADC_0FFE);
        rt.load_clj("g", GAME).unwrap();
        rt.call_init("g").unwrap();
        for _ in 0..30 {
            rt.call_systems("g", 16).unwrap();
            rt.integrate(16);
        }
        let h = world_hash(&w);
        const GOLDEN: u64 = 0xa86ffa76713b5595;
        assert_eq!(h, GOLDEN, "despawn-game world hash 0x{h:016x} ≠ GOLDEN");
    }

    #[test]
    fn lang_user_function_call_with_args() {
        // User-defined fn calls (Expr::Call to a non-builtin) with multiple args +
        // a return value — the core call ABI, exercised only indirectly before
        // (the games' `(player)` is 0-arg). Verified on both backends.
        assert_eq!(
            run_init_count(
                r#"(defn add3 [a b c] (+ a b c))
                   (defn init [] (when (= (add3 2 3 5) 10) (spawn-entity "ok")))"#,
                "ok",
            ),
            1
        );
        // a returned value composes into further arithmetic
        assert_eq!(
            run_init_count(
                r#"(defn dbl [x] (* x 2))
                   (defn init [] (when (= (dbl (dbl 3)) 12) (spawn-entity "ok")))"#,
                "ok",
            ),
            1
        );
    }

    #[test]
    fn lang_integer_division() {
        // `/` is integer division (truncating). Previously untested.
        assert_eq!(eval_count("(/ 17 5)"), 3);
        assert_eq!(eval_count("(/ 100 10)"), 10);
        assert_eq!(eval_count("(* (/ 9 2) 3)"), 12); // (9/2=4)*3
    }

    #[test]
    fn host_surfaces_guest_trap_as_err() {
        // A guest divide-by-zero traps the wasm; the host must surface it as Err
        // (not crash/abort), and stay usable afterward — a guest bug can't take
        // the host down. Verified on whichever backend is compiled.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        rt.load_clj("bad", r#"(defn init [] (/ 5 0))"#).unwrap();
        assert!(
            rt.call_init("bad").is_err(),
            "guest trap must be an Err, not a host crash"
        );
        // the runtime is not poisoned: a fresh module still loads and runs.
        rt.load_clj("good", r#"(defn init [] (spawn-entity "ok"))"#)
            .unwrap();
        rt.call_init("good").unwrap();
        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        assert_eq!(q.iter().filter(|(_, t)| t.0 == "ok").count(), 1);
    }

    #[test]
    fn count_tagged_via_world() {
        // spawn tags entities so count/query can see them.
        let w = world();
        let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
        let src = r#"
            (defn init []
              (spawn-entity "enemy")
              (spawn-entity "enemy")
              (spawn-entity "bullet"))
        "#;
        rt.load_clj("g", src).unwrap();
        rt.call_init("g").unwrap();
        let world = w.lock().unwrap();
        let mut q = world.query::<&Tag>();
        let enemies = q.iter().filter(|(_, t)| t.0 == "enemy").count();
        assert_eq!(enemies, 2);
    }
}
