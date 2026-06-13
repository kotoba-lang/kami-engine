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
}

#[derive(Debug)]
pub struct DrawCommand {
    pub mesh: String,
    pub pos:  [f32; 3],
}

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

        Ok(Self { engine, linker, store, modules: HashMap::new() })
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
        self.store.data_mut().keys_pressed.clear();
        Ok(())
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
        let entity = world_arc.lock().unwrap().spawn((
            Position([0.0, 0.0, 0.0]),
            Velocity([0.0, 0.0, 0.0]),
            Rotation([0.0, 0.0, 0.0, 1.0]),
        ));
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
