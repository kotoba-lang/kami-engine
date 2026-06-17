//! # kami-script-runtime
//!
//! Wasmtime host that binds every `kami:engine/*` WIT import to live Rust
//! game-engine state, then drives the compiled Clojure game-script lifecycle:
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

use wasmtime::{Engine, Linker, Module, Store};

use kami_core::actor::components::{Position, Rotation, Velocity};

pub use kami_clj::CljError;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("compile error: {0}")]
    Compile(#[from] CljError),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
    #[error("module `{0}` not loaded")]
    NotLoaded(String),
    #[error("missing export `{0}` in module `{1}`")]
    MissingExport(String, String),
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
    pub draw_queue:  Vec<DrawCommand>,

    // --- Time counters (written by the engine before each tick) ------------
    pub delta_ms:   i64,
    pub elapsed_ms: i64,
    pub tick_n:     i64,

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
    pub pos:  [f32; 3],
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
            entity_by_id:    HashMap::new(),
            keys_down:        HashSet::new(),
            keys_pressed:     HashSet::new(),
            axes:             HashMap::new(),
            pointer_x:        0.0,
            pointer_y:        0.0,
            audio_queue:      Vec::new(),
            draw_queue:       Vec::new(),
            delta_ms:         0,
            elapsed_ms:       0,
            tick_n:           0,
            query_cursors:    HashMap::new(),
            next_query:       1,
            rng:              0x9E37_79B9_7F4A_7C15,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

pub struct KamiScriptRuntime {
    engine:  Engine,
    linker:  Linker<HostState>,
    store:   Store<HostState>,
    modules: HashMap<String, (Module, wasmtime::Instance)>,
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

        Ok(Self { engine, linker, store, modules: HashMap::new() })
    }

    /// Seed the deterministic PRNG (shared-seed co-op / replay). The value is
    /// forced odd/non-zero so xorshift64 never degenerates.
    pub fn set_seed(&mut self, seed: u64) {
        self.store.data_mut().rng = seed | 1;
    }

    // -----------------------------------------------------------------------
    // Input snapshot setters — call these from the engine's input handler
    // -----------------------------------------------------------------------

    /// Mark a key as held (`true`) or released (`false`).
    pub fn set_key_down(&mut self, key: &str, down: bool) {
        if down { self.store.data_mut().keys_down.insert(key.to_string()); }
        else    { self.store.data_mut().keys_down.remove(key); }
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

    // -----------------------------------------------------------------------
    // Module lifecycle
    // -----------------------------------------------------------------------

    /// Compile and load a Clojure source string (GAME_PRELUDE is prepended).
    pub fn load_clj(&mut self, name: &str, src: &str) -> Result<(), RuntimeError> {
        let wasm = kami_clj::compile_str_with_prelude(src)?;
        self.load_wasm(name, &wasm)
    }

    /// Load a pre-compiled WASM core module.
    pub fn load_wasm(&mut self, name: &str, wasm: &[u8]) -> Result<(), RuntimeError> {
        let module   = Module::new(&self.engine, wasm)?;
        let instance = self.linker.instantiate(&mut self.store, &module)?;
        self.modules.insert(name.to_string(), (module, instance));
        Ok(())
    }

    /// Call `init()` on a loaded module (once, right after loading).
    pub fn call_init(&mut self, name: &str) -> Result<(), RuntimeError> {
        let (_, instance) = self.modules.get(name)
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
            s.delta_ms    = dt_ms;
            s.elapsed_ms += dt_ms;
            s.tick_n     += 1;
        }
        let (_, instance) = self.modules.get(name)
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
        let (_, instance) = self.modules.get(name)
            .ok_or_else(|| RuntimeError::NotLoaded(name.to_string()))?;
        let instance = *instance;
        let systems: Vec<String> = instance
            .exports(&mut self.store)
            .filter_map(|e| {
                let n = e.name();
                if n.ends_with("-tick") { Some(n.to_string()) } else { None }
            })
            .collect();
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
        let mut w = world.lock().unwrap();
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
        _payload: &[u8],
    ) -> Result<i32, RuntimeError> {
        // TODO: lower payload into guest memory via cabi_realloc
        let (_, instance) = self.modules.get(name)
            .ok_or_else(|| RuntimeError::NotLoaded(name.to_string()))?;
        let instance = *instance;
        let f = instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut self.store, "on-event")
            .map_err(|_| RuntimeError::MissingExport("on-event".into(), name.to_string()))?;
        Ok(f.call(&mut self.store, (kind, 0, 0))?)
    }

    /// Drain draw commands accumulated during the last tick.
    pub fn drain_draw_queue(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.store.data_mut().draw_queue)
    }

    /// Drain audio-play commands accumulated during the last tick.
    pub fn drain_audio_queue(&mut self) -> Vec<(String, [f32; 3])> {
        std::mem::take(&mut self.store.data_mut().audio_queue)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read a UTF-8 string from guest linear memory via a (ptr, len) pair.
///
/// `get_export` requires `&mut Caller`, so the caller must be taken by mutable ref.
fn read_guest_str(
    caller: &mut wasmtime::Caller<'_, HostState>,
    ptr: i32,
    len: i32,
) -> String {
    if len <= 0 { return String::new(); }
    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
        return String::new();
    };
    let data = mem.data(caller);
    let start = ptr as usize;
    let end   = start + len as usize;
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
    linker.func_wrap(m, "spawn", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> i64 {
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
    })?;

    // despawn(entity: i64)
    linker.func_wrap(m, "despawn", |mut caller: wasmtime::Caller<'_, HostState>, eid: i64| {
        let entity = entity_for_id(caller.data(), eid);
        if let Some(e) = entity {
            let world_arc = caller.data().world.clone();
            let _ = world_arc.lock().unwrap().despawn(e);
            let s = caller.data_mut();
            s.entity_by_id.remove(&(eid as u32));
            s.entity_registry.retain(|_, v| *v != e);
        }
    })?;

    // get-x/y/z(entity: i64) -> f32
    linker.func_wrap(m, "get-x", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Position>(e).ok().map(|p| p.0[0]))
            .unwrap_or(0.0)
    })?;
    linker.func_wrap(m, "get-y", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Position>(e).ok().map(|p| p.0[1]))
            .unwrap_or(0.0)
    })?;
    linker.func_wrap(m, "get-z", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Position>(e).ok().map(|p| p.0[2]))
            .unwrap_or(0.0)
    })?;

    // set-position(entity: i64, x: f32, y: f32, z: f32)
    linker.func_wrap(m, "set-position", |mut caller: wasmtime::Caller<'_, HostState>, eid: i64, x: f32, y: f32, z: f32| {
        let entity = entity_for_id(caller.data(), eid);
        if let Some(e) = entity {
            let world = caller.data_mut().world.clone();
            if let Ok(mut pos) = world.lock().unwrap().get::<&mut Position>(e) {
                pos.0 = [x, y, z];
            }
        }
    })?;

    // get-vx/vy/vz(entity: i64) -> f32
    linker.func_wrap(m, "get-vx", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Velocity>(e).ok().map(|v| v.0[0]))
            .unwrap_or(0.0)
    })?;
    linker.func_wrap(m, "get-vy", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Velocity>(e).ok().map(|v| v.0[1]))
            .unwrap_or(0.0)
    })?;
    linker.func_wrap(m, "get-vz", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Velocity>(e).ok().map(|v| v.0[2]))
            .unwrap_or(0.0)
    })?;

    // set-velocity(entity: i64, vx: f32, vy: f32, vz: f32)
    linker.func_wrap(m, "set-velocity", |mut caller: wasmtime::Caller<'_, HostState>, eid: i64, vx: f32, vy: f32, vz: f32| {
        let entity = entity_for_id(caller.data(), eid);
        if let Some(e) = entity {
            let world = caller.data_mut().world.clone();
            if let Ok(mut vel) = world.lock().unwrap().get::<&mut Velocity>(e) {
                vel.0 = [vx, vy, vz];
            }
        }
    })?;

    // get-rx/ry/rz/rw(entity: i64) -> f32
    linker.func_wrap(m, "get-rx", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Rotation>(e).ok().map(|r| r.0[0]))
            .unwrap_or(0.0)
    })?;
    linker.func_wrap(m, "get-ry", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Rotation>(e).ok().map(|r| r.0[1]))
            .unwrap_or(0.0)
    })?;
    linker.func_wrap(m, "get-rz", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Rotation>(e).ok().map(|r| r.0[2]))
            .unwrap_or(0.0)
    })?;
    linker.func_wrap(m, "get-rw", |caller: wasmtime::Caller<'_, HostState>, eid: i64| -> f32 {
        let world = caller.data().world.clone();
        entity_for_id(caller.data(), eid)
            .and_then(|e| world.lock().unwrap().get::<&Rotation>(e).ok().map(|r| r.0[3]))
            .unwrap_or(1.0) // identity quaternion w = 1
    })?;

    // set-rotation(entity: i64, rx: f32, ry: f32, rz: f32, rw: f32)
    linker.func_wrap(m, "set-rotation", |mut caller: wasmtime::Caller<'_, HostState>, eid: i64, rx: f32, ry: f32, rz: f32, rw: f32| {
        let entity = entity_for_id(caller.data(), eid);
        if let Some(e) = entity {
            let world = caller.data_mut().world.clone();
            if let Ok(mut rot) = world.lock().unwrap().get::<&mut Rotation>(e) {
                rot.0 = [rx, ry, rz, rw];
            }
        }
    })?;

    // ---- ECS queries (survivors core loop) --------------------------------

    // query-begin(tag_ptr, tag_len) -> cursor handle (i64). Snapshots the ids
    // of all entities tagged `tag`; the cursor is drained by query-next.
    linker.func_wrap(m, "query-begin", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> i64 {
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
    })?;

    // query-next(handle) -> next entity-id (i64), or -1 when drained (which
    // also frees the cursor).
    linker.func_wrap(m, "query-next", |mut caller: wasmtime::Caller<'_, HostState>, handle: i64| -> i64 {
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
    })?;

    // count-tagged(tag_ptr, tag_len) -> i64
    linker.func_wrap(m, "count-tagged", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> i64 {
        let tag = read_guest_str(&mut caller, ptr, len);
        let world = caller.data().world.clone();
        let w = world.lock().unwrap();
        let mut q = w.query::<&Tag>();
        q.iter().filter(|(_, t)| t.0 == tag).count() as i64
    })?;

    // nearest(tag_ptr, tag_len, x: f32, y: f32, maxd: f32) -> entity-id, or -1.
    // 2D (x,y) broadphase done host-side so scripts need no f32 math.
    linker.func_wrap(m, "nearest", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32, x: f32, y: f32, maxd: f32| -> i64 {
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
    })?;

    // move-toward(entity, target, speed: f32) — set entity velocity toward
    // target at speed px/s in the XY plane. Host does the normalize×speed math.
    linker.func_wrap(m, "move-toward", |caller: wasmtime::Caller<'_, HostState>, eid: i64, target: i64, speed: f32| {
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
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Random host bindings — host-owned seeded PRNG (deterministic co-op)
// ---------------------------------------------------------------------------

fn bind_random(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/random@1.0.0";
    // int(n) -> uniform i64 in [0, n); 0 if n <= 0. xorshift64 advances the
    // host-owned seed so runs are reproducible (set via KamiScriptRuntime::set_seed).
    linker.func_wrap(m, "int", |mut caller: wasmtime::Caller<'_, HostState>, n: i64| -> i64 {
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
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Physics host bindings  (stub — rapier integration is Phase 3)
// ---------------------------------------------------------------------------

fn bind_physics(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/physics@1.0.0";
    linker.func_wrap(m, "apply-impulse", |_: wasmtime::Caller<'_, HostState>, _eid: i64, _ix: f32, _iy: f32, _iz: f32| {})?;
    linker.func_wrap(m, "apply-force",   |_: wasmtime::Caller<'_, HostState>, _eid: i64, _fx: f32, _fy: f32, _fz: f32| {})?;
    linker.func_wrap(m, "raycast",       |_: wasmtime::Caller<'_, HostState>, _ox: f32, _oy: f32, _oz: f32, _dx: f32, _dy: f32, _dz: f32| -> i64 { 0 })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Input host bindings
// ---------------------------------------------------------------------------

fn bind_input(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/input@1.0.0";

    // key-down?(ptr: i32, len: i32) -> i32  (1 = held, 0 = not)
    linker.func_wrap(m, "key-down", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
        let key = read_guest_str(&mut caller, ptr, len);
        if caller.data().keys_down.contains(&key) { 1 } else { 0 }
    })?;

    // key-pressed?(ptr: i32, len: i32) -> i32  (1 = pressed this frame)
    linker.func_wrap(m, "key-pressed", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
        let key = read_guest_str(&mut caller, ptr, len);
        if caller.data().keys_pressed.contains(&key) { 1 } else { 0 }
    })?;

    // axis(ptr: i32, len: i32) -> f32
    linker.func_wrap(m, "axis", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> f32 {
        let name = read_guest_str(&mut caller, ptr, len);
        caller.data().axes.get(&name).copied().unwrap_or(0.0)
    })?;

    // pointer-x() -> f32
    linker.func_wrap(m, "pointer-x", |caller: wasmtime::Caller<'_, HostState>| -> f32 {
        caller.data().pointer_x
    })?;
    linker.func_wrap(m, "pointer-y", |caller: wasmtime::Caller<'_, HostState>| -> f32 {
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
    linker.func_wrap(m, "draw-mesh", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32, x: f32, y: f32, z: f32| {
        let mesh = read_guest_str(&mut caller, ptr, len);
        caller.data_mut().draw_queue.push(DrawCommand { mesh, pos: [x, y, z] });
    })?;
    linker.func_wrap(m, "spawn-particle", |_: wasmtime::Caller<'_, HostState>, _ptr: i32, _len: i32, _x: f32, _y: f32, _z: f32| {})?;
    linker.func_wrap(m, "draw-line", |_: wasmtime::Caller<'_, HostState>, _x0: f32, _y0: f32, _z0: f32, _x1: f32, _y1: f32, _z1: f32, _color: i64| {})?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Audio host bindings  (queued — kami-audio integration is Phase 3)
// ---------------------------------------------------------------------------

fn bind_audio(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/audio@1.0.0";

    // play(ptr: i32, len: i32)
    linker.func_wrap(m, "play", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| {
        let name = read_guest_str(&mut caller, ptr, len);
        caller.data_mut().audio_queue.push((name, [0.0; 3]));
    })?;
    linker.func_wrap(m, "stop", |_: wasmtime::Caller<'_, HostState>, _ptr: i32, _len: i32| {})?;
    // play-at(ptr: i32, len: i32, x: f32, y: f32, z: f32)
    linker.func_wrap(m, "play-at", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32, x: f32, y: f32, z: f32| {
        let name = read_guest_str(&mut caller, ptr, len);
        caller.data_mut().audio_queue.push((name, [x, y, z]));
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Time host bindings
// ---------------------------------------------------------------------------

fn bind_time(linker: &mut Linker<HostState>) -> Result<(), RuntimeError> {
    let m = "kami:engine/time@1.0.0";
    linker.func_wrap(m, "delta-ms",   |caller: wasmtime::Caller<'_, HostState>| -> i64 { caller.data().delta_ms   })?;
    linker.func_wrap(m, "elapsed-ms", |caller: wasmtime::Caller<'_, HostState>| -> i64 { caller.data().elapsed_ms })?;
    linker.func_wrap(m, "tick",       |caller: wasmtime::Caller<'_, HostState>| -> i64 { caller.data().tick_n     })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Convenience: compile-and-load from a Clojure source string
// ---------------------------------------------------------------------------

/// Compile a Clojure script (with prelude) and immediately call `init()`.
pub fn load_and_init(
    runtime: &mut KamiScriptRuntime,
    name:    &str,
    src:     &str,
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
        assert!(a > 0 && a < 20, "expected a non-trivial spawn count, got {a}");
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
