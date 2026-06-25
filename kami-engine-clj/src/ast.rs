//! Lowering from EDN → typed AST for the Clojure-subset compiler.
//!
//! Extends the kotoba-clj value model with:
//!   - `Expr::Float(f32)` — float literals (compiled to f32.const → i32.reinterpret → i64.extend).
//!   - Game-engine host-import builtins (scene / physics / input / render / audio / time).
//!   - `defsystem` top-level form: sugar for a `(defn name-tick [dt] …)` exported fn.
//!
//! ## Value model
//!
//! All values on the WASM stack are `i64` (same as kotoba-clj).  F32 values
//! are represented as their IEEE-754 bit-pattern zero-extended to i64.
//!
//! Host imports that take f32 parameters receive the low 32 bits of the i64
//! (via `i32.wrap_i64`); the codegen handles this wrapping automatically per
//! the `ParamKind` annotation on each `HostImport`.

use std::sync::atomic::{AtomicU32, Ordering};

use kotoba_edn::{EdnValue, Symbol};

use crate::CljError;

/// Monotonic counter for hygienic temp names (e.g. doseq-entities iterators),
/// so nested expansions never shadow each other.
static GENSYM: AtomicU32 = AtomicU32::new(0);

fn gensym(prefix: &str) -> String {
    format!("__{prefix}{}", GENSYM.fetch_add(1, Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Top-level program
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Program {
    pub defs:      Vec<Def>,
    pub functions: Vec<Function>,
    /// Mutable per-instance state cells (`defatom`) — persist across tick calls.
    pub atoms:     Vec<Atom>,
}

/// A `(defatom name init)` — a mutable i64 cell that lives for the instance's lifetime
/// (a WASM mutable global), letting a game hold lives/score/state without the off-map
/// marker-entity hack. `init` must be an integer constant.
#[derive(Debug, Clone)]
pub struct Atom {
    pub name: String,
    pub init: i64,
}

#[derive(Debug, Clone)]
pub struct Def {
    pub name:  String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name:   String,
    pub params: Vec<String>,
    pub body:   Vec<Expr>,
}

// ---------------------------------------------------------------------------
// Builtins — split into kotoba-core ops and kami-engine host imports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    // ---- arithmetic ---------------------------------------------------------
    Add, Sub, Mul, Div, Mod,
    Inc, Dec, Abs,
    // ---- f32 arithmetic (real float math on the i64-boxed bit-patterns) ------
    // `+f -f *f /f` unbox each operand (i64 bits → f32), do the float op, rebox.
    FAdd, FSub, FMul, FDiv,
    // ---- comparison ---------------------------------------------------------
    Eq, NotEq, Lt, Gt, Le, Ge,
    Zero, Pos, Neg,
    // ---- f32 comparison (sign-correct, unlike I64LtS on f32 bits) -----------
    FLt, FGt, FLe, FGe, FEq,
    // ---- logic --------------------------------------------------------------
    And, Or, Not,
    // ---- string / bytes (kotoba-clj core) -----------------------------------
    StrLen, ByteAt,
    BytesAlloc, ByteAppend, BytesLen, BytesFinish,
    // ---- raw memory ---------------------------------------------------------
    Alloc,
    Load64, Store64,
    Load32, Store32,
    // ---- f32 bit-pattern helpers -------------------------------------------
    /// `(f32->bits x)` — alias for clarity; at the WASM level this is a NOP
    /// because f32 literals already arrive as their bit-pattern i64.
    F32Bits,
    /// `(bits->f32 x)` — extract the low 32 bits of an i64 as f32.  Used when
    /// passing the result of arithmetic back to a host import.
    BitsF32,

    // ---- KAMI scene / ECS --------------------------------------------------
    /// `(spawn-entity kind-str)` → entity-id (i64)
    SpawnEntity,
    /// `(despawn-entity eid)` → void (returns 0)
    DespawnEntity,
    /// `(get-x eid)` → f32 bits (i64)
    GetX, GetY, GetZ,
    /// `(set-position! eid x y z)` — x/y/z are f32 bits
    SetPosition,
    /// `(get-vx eid)`, `(get-vy eid)`, `(get-vz eid)` → f32 bits
    GetVx, GetVy, GetVz,
    /// `(set-velocity! eid vx vy vz)` — vx/vy/vz are f32 bits
    SetVelocity,
    /// `(get-rx eid)` etc. → quaternion component as f32 bits
    GetRx, GetRy, GetRz, GetRw,
    /// `(set-rotation! eid rx ry rz rw)`
    SetRotation,

    // ---- KAMI physics -------------------------------------------------------
    /// `(apply-impulse! eid ix iy iz)` — f32 bits
    ApplyImpulse,
    /// `(apply-force! eid fx fy fz)` — f32 bits
    ApplyForce,
    /// `(raycast ox oy oz dx dy dz)` → entity-id (i64) or 0
    Raycast,

    // ---- KAMI input ---------------------------------------------------------
    /// `(key-down? key-str)` → 1/0
    KeyDown,
    /// `(key-pressed? key-str)` → 1/0 (edge detect)
    KeyPressed,
    /// `(axis name-str)` → f32 bits (i64) in [-1, 1]
    Axis,
    /// `(pointer-x)` / `(pointer-y)` → f32 bits (canvas pixels)
    PointerX, PointerY,

    // ---- KAMI render --------------------------------------------------------
    /// `(draw-mesh! mesh-str x y z)` — x/y/z are f32 bits
    DrawMesh,
    /// `(spawn-particle! preset-str x y z)`
    SpawnParticle,
    /// `(draw-line! x0 y0 z0 x1 y1 z1 color)`
    DrawLine,
    /// `(rt-enable! recipe-str)` — switch the frame to a named ray-tracing
    /// recipe (kami.rt IR); empty string falls back to the raster path.
    RtEnable,

    // ---- KAMI audio ---------------------------------------------------------
    /// `(play-sound name-str)` → void
    PlaySound,
    /// `(stop-sound name-str)` → void
    StopSound,
    /// `(play-sound-at name-str x y z)` — spatial audio
    PlaySoundAt,
    /// `(set-listener! x y z fx fy fz)` — listener pose for binaural mixing
    /// (kami.binaural); pos (x,y,z) + forward (fx,fy,fz) as f32.
    SetListener,

    // ---- KAMI time ----------------------------------------------------------
    /// `(delta-ms)` → i64
    DeltaMs,
    /// `(elapsed-ms)` → i64
    ElapsedMs,
    /// `(tick-n)` → i64 (current tick number)
    TickN,

    // ---- KAMI query / RNG (survivors core loop) ----------------------------
    /// `(rand-int n)` → i64 in [0, n). Host owns the seeded PRNG so co-op runs
    /// stay deterministic (shared-seed) — the guest never holds the seed.
    RandInt,
    /// `(query-begin tag-str)` → iterator handle (i64). Snapshots the entities
    /// tagged `tag` on the host and returns an opaque cursor handle.
    QueryBegin,
    /// `(query-next it)` → next entity-id (i64), or -1 when the cursor is drained.
    QueryNext,
    /// `(count-tagged tag-str)` → i64 count of entities tagged `tag`.
    CountTagged,
    /// `(nearest-tagged tag-str x y maxd)` → nearest entity tagged `tag` within
    /// `maxd` of (x,y) [f32], or -1. Host does the f32 distance (broadphase),
    /// so weapon targeting + bullet/contact collision need no f32 math in clj.
    NearestTagged,
    /// `(move-toward! eid target-eid speed)` — set `eid`'s velocity toward
    /// `target-eid` at `speed` px/s [f32]. Host does the normalize×speed vector
    /// math, so enemy-chase / homing need no in-guest f32 arithmetic (guest
    /// `+`/`-`/`*` are integer ops; this avoids operating on f32 bit-patterns).
    MoveToward,
}

impl Builtin {
    pub fn host_import(self) -> Option<HostImport> {
        use Builtin::*;
        match self {
            SpawnEntity    => Some(HostImport::SceneSpawn),
            DespawnEntity  => Some(HostImport::SceneDespawn),
            GetX           => Some(HostImport::SceneGetX),
            GetY           => Some(HostImport::SceneGetY),
            GetZ           => Some(HostImport::SceneGetZ),
            SetPosition    => Some(HostImport::SceneSetPosition),
            GetVx          => Some(HostImport::SceneGetVx),
            GetVy          => Some(HostImport::SceneGetVy),
            GetVz          => Some(HostImport::SceneGetVz),
            SetVelocity    => Some(HostImport::SceneSetVelocity),
            GetRx          => Some(HostImport::SceneGetRx),
            GetRy          => Some(HostImport::SceneGetRy),
            GetRz          => Some(HostImport::SceneGetRz),
            GetRw          => Some(HostImport::SceneGetRw),
            SetRotation    => Some(HostImport::SceneSetRotation),
            ApplyImpulse   => Some(HostImport::PhysicsApplyImpulse),
            ApplyForce     => Some(HostImport::PhysicsApplyForce),
            Raycast        => Some(HostImport::PhysicsRaycast),
            KeyDown        => Some(HostImport::InputKeyDown),
            KeyPressed     => Some(HostImport::InputKeyPressed),
            Axis           => Some(HostImport::InputAxis),
            PointerX       => Some(HostImport::InputPointerX),
            PointerY       => Some(HostImport::InputPointerY),
            DrawMesh       => Some(HostImport::RenderDrawMesh),
            SpawnParticle  => Some(HostImport::RenderSpawnParticle),
            DrawLine       => Some(HostImport::RenderDrawLine),
            RtEnable       => Some(HostImport::RenderRtEnable),
            PlaySound      => Some(HostImport::AudioPlay),
            StopSound      => Some(HostImport::AudioStop),
            PlaySoundAt    => Some(HostImport::AudioPlayAt),
            SetListener    => Some(HostImport::AudioSetListener),
            DeltaMs        => Some(HostImport::TimeDeltaMs),
            ElapsedMs      => Some(HostImport::TimeElapsedMs),
            TickN          => Some(HostImport::TimeTick),
            RandInt        => Some(HostImport::RandomInt),
            QueryBegin     => Some(HostImport::SceneQueryBegin),
            QueryNext      => Some(HostImport::SceneQueryNext),
            CountTagged    => Some(HostImport::SceneCountTagged),
            NearestTagged  => Some(HostImport::SceneNearest),
            MoveToward     => Some(HostImport::SceneMoveToward),
            _              => None,
        }
    }

    fn from_name(s: &str) -> Option<Builtin> {
        use Builtin::*;
        Some(match s {
            // arithmetic
            "+"          => Add,
            "-"          => Sub,
            "*"          => Mul,
            "/" | "quot" => Div,
            "mod" | "rem"=> Mod,
            "inc"        => Inc,
            "dec"        => Dec,
            "abs"        => Abs,
            // f32 arithmetic (real float math on i64-boxed bit-patterns)
            "+f"         => FAdd,
            "-f"         => FSub,
            "*f"         => FMul,
            "/f"         => FDiv,
            // f32 comparison (sign-correct)
            "<f"         => FLt,
            ">f"         => FGt,
            "<=f"        => FLe,
            ">=f"        => FGe,
            "=f"         => FEq,
            // comparison
            "="          => Eq,
            "!=" | "not="=> NotEq,
            "<"          => Lt,
            ">"          => Gt,
            "<="         => Le,
            ">="         => Ge,
            "zero?"      => Zero,
            "pos?"       => Pos,
            "neg?"       => Neg,
            // logic
            "and"        => And,
            "or"         => Or,
            "not"        => Not,
            // strings / bytes
            "str-len"    => StrLen,
            "byte-at"    => ByteAt,
            "bytes-alloc"=> BytesAlloc,
            "byte-append!"=> ByteAppend,
            "bytes-len"  => BytesLen,
            "bytes-finish"=> BytesFinish,
            // raw memory
            "alloc"      => Alloc,
            "load64"     => Load64,
            "store64!"   => Store64,
            "load32"     => Load32,
            "store32!"   => Store32,
            // f32 helpers
            "f32->bits"  => F32Bits,
            "bits->f32"  => BitsF32,
            // scene / ECS
            "spawn-entity"    => SpawnEntity,
            "despawn-entity"  => DespawnEntity,
            "get-x"           => GetX,
            "get-y"           => GetY,
            "get-z"           => GetZ,
            "set-position!"   => SetPosition,
            "get-vx"          => GetVx,
            "get-vy"          => GetVy,
            "get-vz"          => GetVz,
            "set-velocity!"   => SetVelocity,
            "get-rx"          => GetRx,
            "get-ry"          => GetRy,
            "get-rz"          => GetRz,
            "get-rw"          => GetRw,
            "set-rotation!"   => SetRotation,
            // physics
            "apply-impulse!"  => ApplyImpulse,
            "apply-force!"    => ApplyForce,
            "raycast"         => Raycast,
            // input
            "key-down?"       => KeyDown,
            "key-pressed?"    => KeyPressed,
            "axis"            => Axis,
            "pointer-x"       => PointerX,
            "pointer-y"       => PointerY,
            // render
            "draw-mesh!"      => DrawMesh,
            "spawn-particle!" => SpawnParticle,
            "draw-line!"      => DrawLine,
            "rt-enable!"      => RtEnable,
            // audio
            "play-sound"      => PlaySound,
            "stop-sound"      => StopSound,
            "play-sound-at"   => PlaySoundAt,
            "set-listener!"   => SetListener,
            // time
            "delta-ms"        => DeltaMs,
            "elapsed-ms"      => ElapsedMs,
            "tick-n"          => TickN,
            // query / RNG (survivors)
            "rand-int"        => RandInt,
            "query-begin"     => QueryBegin,
            "query-next"      => QueryNext,
            "count-tagged"    => CountTagged,
            "nearest-tagged"  => NearestTagged,
            "move-toward!"    => MoveToward,
            _                 => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Host imports — each maps to a (module, field) WASM import
// ---------------------------------------------------------------------------

/// Describes the core-WASM type of a parameter as the codegen sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// Plain i64 value (entity IDs, string handles, booleans, integers).
    I64,
    /// f32 bit-pattern stored in an i64; codegen emits `i32.wrap_i64` +
    /// `f32.reinterpret_i32` before the host call.
    F32,
    /// String handle (packed `(offset<<32)|len`) lowered to a `(ptr:i32, len:i32)` pair.
    StringHandle,
}

/// Core WASM return type from the host function, before the codegen lifts it
/// to i64 for the all-i64 guest value model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnKind {
    /// No return value; codegen pushes a `0_i64` placeholder so the stack stays balanced.
    Void,
    /// Returns i32 (entity low half, bool); codegen zero-extends to i64.
    I32,
    /// Returns i64 directly (entity IDs, tick counter).
    I64,
    /// Returns f32; codegen emits `i32.reinterpret_f32` + `i64.extend_i32_u`.
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostImport {
    // scene
    SceneSpawn, SceneDespawn,
    SceneGetX, SceneGetY, SceneGetZ, SceneSetPosition,
    SceneGetVx, SceneGetVy, SceneGetVz, SceneSetVelocity,
    SceneGetRx, SceneGetRy, SceneGetRz, SceneGetRw, SceneSetRotation,
    // physics
    PhysicsApplyImpulse, PhysicsApplyForce, PhysicsRaycast,
    // input
    InputKeyDown, InputKeyPressed, InputAxis, InputPointerX, InputPointerY,
    // render
    RenderDrawMesh, RenderSpawnParticle, RenderDrawLine, RenderRtEnable,
    // audio
    AudioPlay, AudioStop, AudioPlayAt, AudioSetListener,
    // time
    TimeDeltaMs, TimeElapsedMs, TimeTick,
    // query / RNG (survivors)
    RandomInt,
    SceneQueryBegin, SceneQueryNext, SceneCountTagged, SceneNearest, SceneMoveToward,
}

impl HostImport {
    /// WASM import `(module, field)` matching `wit/kami-game/world.wit`.
    pub fn module_field(self) -> (&'static str, &'static str) {
        use HostImport::*;
        match self {
            SceneSpawn        => ("kami:engine/scene@1.0.0",   "spawn"),
            SceneDespawn      => ("kami:engine/scene@1.0.0",   "despawn"),
            SceneGetX         => ("kami:engine/scene@1.0.0",   "get-x"),
            SceneGetY         => ("kami:engine/scene@1.0.0",   "get-y"),
            SceneGetZ         => ("kami:engine/scene@1.0.0",   "get-z"),
            SceneSetPosition  => ("kami:engine/scene@1.0.0",   "set-position"),
            SceneGetVx        => ("kami:engine/scene@1.0.0",   "get-vx"),
            SceneGetVy        => ("kami:engine/scene@1.0.0",   "get-vy"),
            SceneGetVz        => ("kami:engine/scene@1.0.0",   "get-vz"),
            SceneSetVelocity  => ("kami:engine/scene@1.0.0",   "set-velocity"),
            SceneGetRx        => ("kami:engine/scene@1.0.0",   "get-rx"),
            SceneGetRy        => ("kami:engine/scene@1.0.0",   "get-ry"),
            SceneGetRz        => ("kami:engine/scene@1.0.0",   "get-rz"),
            SceneGetRw        => ("kami:engine/scene@1.0.0",   "get-rw"),
            SceneSetRotation  => ("kami:engine/scene@1.0.0",   "set-rotation"),
            PhysicsApplyImpulse=> ("kami:engine/physics@1.0.0","apply-impulse"),
            PhysicsApplyForce => ("kami:engine/physics@1.0.0", "apply-force"),
            PhysicsRaycast    => ("kami:engine/physics@1.0.0", "raycast"),
            InputKeyDown      => ("kami:engine/input@1.0.0",   "key-down"),
            InputKeyPressed   => ("kami:engine/input@1.0.0",   "key-pressed"),
            InputAxis         => ("kami:engine/input@1.0.0",   "axis"),
            InputPointerX     => ("kami:engine/input@1.0.0",   "pointer-x"),
            InputPointerY     => ("kami:engine/input@1.0.0",   "pointer-y"),
            RenderDrawMesh    => ("kami:engine/render@1.0.0",  "draw-mesh"),
            RenderSpawnParticle=> ("kami:engine/render@1.0.0", "spawn-particle"),
            RenderDrawLine    => ("kami:engine/render@1.0.0",  "draw-line"),
            RenderRtEnable    => ("kami:engine/render@1.0.0",  "rt-enable"),
            AudioPlay         => ("kami:engine/audio@1.0.0",   "play"),
            AudioStop         => ("kami:engine/audio@1.0.0",   "stop"),
            AudioPlayAt       => ("kami:engine/audio@1.0.0",   "play-at"),
            AudioSetListener  => ("kami:engine/audio@1.0.0",   "set-listener"),
            TimeDeltaMs       => ("kami:engine/time@1.0.0",    "delta-ms"),
            TimeElapsedMs     => ("kami:engine/time@1.0.0",    "elapsed-ms"),
            TimeTick          => ("kami:engine/time@1.0.0",    "tick"),
            RandomInt         => ("kami:engine/random@1.0.0",  "int"),
            SceneQueryBegin   => ("kami:engine/scene@1.0.0",   "query-begin"),
            SceneQueryNext    => ("kami:engine/scene@1.0.0",   "query-next"),
            SceneCountTagged  => ("kami:engine/scene@1.0.0",   "count-tagged"),
            SceneNearest      => ("kami:engine/scene@1.0.0",   "nearest"),
            SceneMoveToward   => ("kami:engine/scene@1.0.0",   "move-toward"),
        }
    }

    /// Parameter kinds from the guest's perspective (each i64 on the value stack).
    /// `StringHandle` lowers to 2 × i32 (ptr, len) — must be accounted for in
    /// the WASM function type.
    pub fn param_kinds(self) -> &'static [ParamKind] {
        use HostImport::*;
        use ParamKind::*;
        match self {
            SceneSpawn        => &[StringHandle],
            SceneDespawn      => &[I64],
            SceneGetX | SceneGetY | SceneGetZ   => &[I64],
            SceneSetPosition  => &[I64, F32, F32, F32],
            SceneGetVx | SceneGetVy | SceneGetVz => &[I64],
            SceneSetVelocity  => &[I64, F32, F32, F32],
            SceneGetRx | SceneGetRy | SceneGetRz | SceneGetRw => &[I64],
            SceneSetRotation  => &[I64, F32, F32, F32, F32],
            PhysicsApplyImpulse | PhysicsApplyForce => &[I64, F32, F32, F32],
            PhysicsRaycast    => &[F32, F32, F32, F32, F32, F32],
            InputKeyDown | InputKeyPressed => &[StringHandle],
            InputAxis         => &[StringHandle],
            InputPointerX | InputPointerY  => &[],
            RenderDrawMesh    => &[StringHandle, F32, F32, F32],
            RenderSpawnParticle=> &[StringHandle, F32, F32, F32],
            RenderDrawLine    => &[F32, F32, F32, F32, F32, F32, I64],
            RenderRtEnable    => &[StringHandle],
            AudioPlay | AudioStop => &[StringHandle],
            AudioPlayAt       => &[StringHandle, F32, F32, F32],
            AudioSetListener  => &[F32, F32, F32, F32, F32, F32],
            TimeDeltaMs | TimeElapsedMs | TimeTick => &[],
            RandomInt         => &[I64],
            SceneQueryBegin   => &[StringHandle],
            SceneQueryNext    => &[I64],
            SceneCountTagged  => &[StringHandle],
            SceneNearest      => &[StringHandle, F32, F32, F32],
            SceneMoveToward   => &[I64, I64, F32],
        }
    }

    pub fn return_kind(self) -> ReturnKind {
        use HostImport::*;
        use ReturnKind::*;
        match self {
            SceneSpawn                         => I64,
            SceneDespawn                       => Void,
            SceneGetX | SceneGetY | SceneGetZ  => F32,
            SceneSetPosition                   => Void,
            SceneGetVx | SceneGetVy | SceneGetVz => F32,
            SceneSetVelocity                   => Void,
            SceneGetRx | SceneGetRy | SceneGetRz | SceneGetRw => F32,
            SceneSetRotation                   => Void,
            PhysicsApplyImpulse | PhysicsApplyForce => Void,
            PhysicsRaycast                     => I64,
            InputKeyDown | InputKeyPressed     => I32,
            InputAxis                          => F32,
            InputPointerX | InputPointerY      => F32,
            RenderDrawMesh | RenderSpawnParticle | RenderDrawLine | RenderRtEnable => Void,
            AudioPlay | AudioStop | AudioPlayAt | AudioSetListener => Void,
            TimeDeltaMs | TimeElapsedMs | TimeTick => I64,
            RandomInt | SceneQueryBegin | SceneQueryNext
            | SceneCountTagged | SceneNearest => I64,
            SceneMoveToward => Void,
        }
    }
}

// ---------------------------------------------------------------------------
// Expression AST  (superset of kotoba-clj)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal (booleans lower to 1/0).
    Int(i64),
    /// Float literal — compiled to `f32.const` → `i32.reinterpret_f32` → `i64.extend_i32_u`.
    Float(f32),
    /// String literal — stored in a data segment; value is `(offset<<32)|len`.
    Str(Vec<u8>),
    /// Bare symbol — resolves to param, let binding, or def constant.
    Var(String),
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        els:  Box<Expr>,
    },
    Let {
        bindings: Vec<(String, Expr)>,
        body:     Vec<Expr>,
    },
    Do(Vec<Expr>),
    Loop {
        bindings: Vec<(String, Expr)>,
        body:     Vec<Expr>,
    },
    Recur(Vec<Expr>),
    Builtin {
        op:   Builtin,
        args: Vec<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    /// `(atom-val name)` — read a `defatom` cell (a WASM mutable global).
    AtomGet(String),
    /// `(set-atom! name value)` — write a `defatom` cell; evaluates to the new value.
    AtomSet(String, Box<Expr>),
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

pub fn parse_program(src: &str) -> Result<Program, CljError> {
    let forms = kotoba_edn::parse_all(src).map_err(|e| CljError::Read(e.to_string()))?;
    let mut defs      = Vec::new();
    let mut functions = Vec::new();
    let mut atoms     = Vec::new();

    for form in &forms {
        let items = match form {
            EdnValue::List(items) => items,
            other => {
                return Err(CljError::Lower(format!(
                    "top-level form must be a list, found: {other:?}"
                )))
            }
        };
        let head = list_head_symbol(items)?;
        match head.name.as_str() {
            "ns"        => { /* namespace declaration — ignored */ }
            "def"       => defs.push(lower_def(items)?),
            "defn"      => functions.push(lower_defn(items)?),
            "defsystem" => functions.push(lower_defsystem(items)?),
            "defentity" => functions.push(lower_defentity(items)?),
            "defatom"   => atoms.push(lower_defatom(items)?),
            other       => {
                return Err(CljError::Lower(format!(
                    "unsupported top-level form `({other} …)` — expected def/defn/ns/defsystem/defentity/defatom"
                )))
            }
        }
    }
    Ok(Program { defs, functions, atoms })
}

// ---------------------------------------------------------------------------
// Lowering helpers
// ---------------------------------------------------------------------------

fn list_head_symbol(items: &[EdnValue]) -> Result<&Symbol, CljError> {
    match items.first() {
        Some(EdnValue::Symbol(s)) => Ok(s),
        _ => Err(CljError::Lower(
            "list must begin with a symbol in head position".into(),
        )),
    }
}

fn lower_def(items: &[EdnValue]) -> Result<Def, CljError> {
    if items.len() != 3 {
        return Err(CljError::Lower("def takes exactly: (def name value)".into()));
    }
    Ok(Def {
        name:  sym_name(&items[1], "def name")?,
        value: lower_expr(&items[2])?,
    })
}

/// `(defatom name <int>)` — declare a mutable i64 state cell. The initial value must be an
/// integer constant (the WASM global's init expression); a game increments/sets it at runtime.
fn lower_defatom(items: &[EdnValue]) -> Result<Atom, CljError> {
    if items.len() != 3 {
        return Err(CljError::Lower("defatom takes exactly: (defatom name <int-literal>)".into()));
    }
    let name = sym_name(&items[1], "defatom name")?;
    let init = match lower_expr(&items[2])? {
        Expr::Int(v) => v,
        _ => return Err(CljError::Lower(format!(
            "defatom `{name}` initial value must be an integer literal"
        ))),
    };
    Ok(Atom { name, init })
}

fn lower_defn(items: &[EdnValue]) -> Result<Function, CljError> {
    if items.len() < 4 {
        return Err(CljError::Lower("defn requires: (defn name [params…] body…)".into()));
    }
    let name   = sym_name(&items[1], "defn name")?;
    let params = lower_param_vec(&items[2], &name)?;
    let body   = items[3..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    Ok(Function { name, params, body })
}

/// `(defsystem name [dt] body…)` — sugar for a tick handler.
///
/// Exported as `name-tick` and takes a single `dt-ms` parameter (i64).
/// Example:
/// ```clojure
/// (defsystem player-controller [dt]
///   (when (key-down? "ArrowRight")
///     (set-velocity! player (f32 1.0) (f32 0.0) (f32 0.0))))
/// ```
fn lower_defsystem(items: &[EdnValue]) -> Result<Function, CljError> {
    if items.len() < 4 {
        return Err(CljError::Lower(
            "defsystem requires: (defsystem name [dt] body…)".into(),
        ));
    }
    let name = sym_name(&items[1], "defsystem name")?;
    let tick_name = format!("{name}-tick");
    let params = lower_param_vec(&items[2], &name)?;
    let body   = items[3..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    Ok(Function { name: tick_name, params, body })
}

/// `(defentity name [params…] body…)` — entity-template constructor sugar.
///
/// Desugars to a fn that spawns a fresh entity tagged `"name"`, binds it to
/// `self` for the body to initialize, and returns the entity id — the Phase-4
/// prefab DSL, so a whole game's spawn templates live in one guest wasm.
///
/// ```clojure
/// (defentity enemy [x y]
///   (set-position! self x y (f32 0.0)))
/// ;; ≡ (defn enemy [x y]
/// ;;     (let [self (spawn-entity "enemy")]
/// ;;       (set-position! self x y (f32 0.0))
/// ;;       self))
/// ```
fn lower_defentity(items: &[EdnValue]) -> Result<Function, CljError> {
    if items.len() < 4 {
        return Err(CljError::Lower(
            "defentity requires: (defentity name [params…] body…)".into(),
        ));
    }
    let name   = sym_name(&items[1], "defentity name")?;
    let params = lower_param_vec(&items[2], &name)?;
    let mut body = items[3..]
        .iter()
        .map(lower_expr)
        .collect::<Result<Vec<_>, _>>()?;
    // The constructor returns `self` so callers can hold/seed the spawned id.
    body.push(Expr::Var("self".to_string()));
    // `self` = a fresh entity tagged with the template name (the host's
    // `spawn` tags it, so query/count/nearest find it by that tag).
    let spawn = Expr::Builtin {
        op:   Builtin::SpawnEntity,
        args: vec![Expr::Str(name.clone().into_bytes())],
    };
    let let_expr = Expr::Let {
        bindings: vec![("self".to_string(), spawn)],
        body,
    };
    Ok(Function { name, params, body: vec![let_expr] })
}

fn lower_param_vec(v: &EdnValue, ctx: &str) -> Result<Vec<String>, CljError> {
    match v {
        EdnValue::Vector(ps) => ps
            .iter()
            .map(|p| sym_name(p, "parameter"))
            .collect::<Result<Vec<_>, _>>(),
        _ => Err(CljError::Lower(format!(
            "`{ctx}` parameter list must be a vector `[…]`"
        ))),
    }
}

fn lower_expr(v: &EdnValue) -> Result<Expr, CljError> {
    match v {
        EdnValue::Integer(i) => Ok(Expr::Int(*i)),
        EdnValue::Bool(b)    => Ok(Expr::Int(if *b { 1 } else { 0 })),
        EdnValue::Float(f)   => Ok(Expr::Float(f.into_inner() as f32)),
        EdnValue::String(s)  => Ok(Expr::Str(s.clone().into_bytes())),
        EdnValue::Symbol(s)  => Ok(Expr::Var(s.to_qualified())),
        EdnValue::List(items) => lower_call(items),
        other => Err(CljError::Lower(format!(
            "unsupported expression: {other:?}"
        ))),
    }
}

fn lower_call(items: &[EdnValue]) -> Result<Expr, CljError> {
    let head = list_head_symbol(items)?;
    let args = &items[1..];
    match head.name.as_str() {
        "if"    => lower_if(args),
        "when"  => lower_when(args),
        "let"   => lower_let(args),
        "cond"  => lower_cond(args),
        "loop"  => lower_loop(args),
        "recur" => Ok(Expr::Recur(args.iter().map(lower_expr).collect::<Result<_, _>>()?)),
        "do"    => Ok(Expr::Do(args.iter().map(lower_expr).collect::<Result<_, _>>()?)),
        "if-not"   => lower_if_not(args),
        "when-not" => lower_when_not(args),
        "case"     => lower_case(args),
        "->"       => lower_thread(args, true),
        "->>"      => lower_thread(args, false),
        "if-let"   => lower_if_let(args),
        "when-let" => lower_when_let(args),
        "dotimes"  => lower_dotimes(args),
        "as->"     => lower_as_thread(args),
        "cond->"   => lower_cond_thread(args, true),
        "cond->>"  => lower_cond_thread(args, false),
        "doseq-entities" => lower_doseq_entities(args),
        // `(atom-val name)` / `(set-atom! name value)` — read/write a defatom cell. The atom
        // name is a bare symbol (resolved to a global at codegen), not an evaluated argument.
        "atom-val" | "deref" => {
            if args.len() != 1 {
                return Err(CljError::Lower("(atom-val name) takes exactly one argument".into()));
            }
            Ok(Expr::AtomGet(sym_name(&args[0], "atom name")?))
        }
        "set-atom!" => {
            if args.len() != 2 {
                return Err(CljError::Lower("(set-atom! name value) takes exactly two arguments".into()));
            }
            Ok(Expr::AtomSet(sym_name(&args[0], "atom name")?, Box::new(lower_expr(&args[1])?)))
        }
        // `(f32 1.5)` — explicit float literal form for disambiguation
        "f32" => {
            if args.len() != 1 {
                return Err(CljError::Lower("(f32 val) takes exactly one argument".into()));
            }
            match &args[0] {
                EdnValue::Float(f)   => Ok(Expr::Float(f.into_inner() as f32)),
                EdnValue::Integer(i) => Ok(Expr::Float(*i as f32)),
                other => Err(CljError::Lower(format!(
                    "(f32 …) argument must be a number, found {other:?}"
                ))),
            }
        }
        name => {
            let lowered: Vec<Expr> = args.iter().map(lower_expr).collect::<Result<_, _>>()?;
            if let Some(op) = Builtin::from_name(name) {
                check_builtin_arity(op, lowered.len())?;
                Ok(Expr::Builtin { op, args: lowered })
            } else {
                Ok(Expr::Call { name: head.to_qualified(), args: lowered })
            }
        }
    }
}

fn lower_if_not(args: &[EdnValue]) -> Result<Expr, CljError> {
    if args.len() != 3 {
        return Err(CljError::Lower("if-not takes: (if-not cond then else)".into()));
    }
    // (if-not c then else) ≡ (if c else then) — no `not` needed, just swap the branches.
    Ok(Expr::If {
        cond: Box::new(lower_expr(&args[0])?),
        then: Box::new(lower_expr(&args[2])?),
        els:  Box::new(lower_expr(&args[1])?),
    })
}

fn lower_when_not(args: &[EdnValue]) -> Result<Expr, CljError> {
    if args.is_empty() {
        return Err(CljError::Lower("when-not takes: (when-not cond body…)".into()));
    }
    let body = args[1..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    // (when-not c body…) ≡ (if c 0 (do body…))
    Ok(Expr::If {
        cond: Box::new(lower_expr(&args[0])?),
        then: Box::new(Expr::Int(0)),
        els:  Box::new(Expr::Do(body)),
    })
}

fn lower_case(args: &[EdnValue]) -> Result<Expr, CljError> {
    if args.is_empty() {
        return Err(CljError::Lower("case takes: (case expr v1 e1 … [default])".into()));
    }
    let rest = &args[1..];
    let has_default = rest.len() % 2 == 1;
    let mut acc = if has_default {
        lower_expr(rest.last().unwrap())?
    } else {
        Expr::Int(0)
    };
    let pairs = if has_default { &rest[..rest.len() - 1] } else { rest };
    // fold the test/result pairs into nested (if (= expr v) result …); right-to-left so order holds.
    for pair in pairs.chunks_exact(2).rev() {
        let test = Expr::Builtin {
            op: Builtin::Eq,
            args: vec![lower_expr(&args[0])?, lower_expr(&pair[0])?],
        };
        acc = Expr::If {
            cond: Box::new(test),
            then: Box::new(lower_expr(&pair[1])?),
            els:  Box::new(acc),
        };
    }
    Ok(acc)
}

fn thread_into(step: &EdnValue, acc: Expr, first: bool) -> Result<Expr, CljError> {
    match step {
        // a call step (f a b): thread acc in as the first (->) or last (->>) argument.
        EdnValue::List(items) => {
            let head = list_head_symbol(items)?;
            let mut largs: Vec<Expr> =
                items[1..].iter().map(lower_expr).collect::<Result<_, _>>()?;
            if first { largs.insert(0, acc); } else { largs.push(acc); }
            if let Some(op) = Builtin::from_name(head.name.as_str()) {
                check_builtin_arity(op, largs.len())?;
                Ok(Expr::Builtin { op, args: largs })
            } else {
                Ok(Expr::Call { name: head.to_qualified(), args: largs })
            }
        }
        // a bare symbol step f: (-> x f) ≡ (f x)
        EdnValue::Symbol(s) => {
            if let Some(op) = Builtin::from_name(s.name.as_str()) {
                check_builtin_arity(op, 1)?;
                Ok(Expr::Builtin { op, args: vec![acc] })
            } else {
                Ok(Expr::Call { name: s.to_qualified(), args: vec![acc] })
            }
        }
        other => Err(CljError::Lower(format!(
            "-> / ->> step must be a call or symbol, found {other:?}"
        ))),
    }
}

fn lower_thread(args: &[EdnValue], first: bool) -> Result<Expr, CljError> {
    if args.is_empty() {
        return Err(CljError::Lower("-> / ->> takes: (-> x step…)".into()));
    }
    let mut acc = lower_expr(&args[0])?;
    for step in &args[1..] {
        acc = thread_into(step, acc, first)?;
    }
    Ok(acc)
}

fn binding_pair<'a>(args: &'a [EdnValue], form: &str) -> Result<(&'a EdnValue, &'a EdnValue), CljError> {
    match args.first() {
        Some(EdnValue::Vector(v)) if v.len() == 2 => Ok((&v[0], &v[1])),
        _ => Err(CljError::Lower(format!("{form} requires a [name expr] binding vector"))),
    }
}

fn lower_if_let(args: &[EdnValue]) -> Result<Expr, CljError> {
    // (if-let [name expr] then else?) ≡ (let [name expr] (if name then else))
    let (name_v, expr_v) = binding_pair(args, "if-let")?;
    let name = sym_name(name_v, "if-let binding name")?;
    let val = lower_expr(expr_v)?;
    let then = lower_expr(args.get(1)
        .ok_or_else(|| CljError::Lower("if-let needs a then branch".into()))?)?;
    let els = match args.get(2) { Some(e) => lower_expr(e)?, None => Expr::Int(0) };
    Ok(Expr::Let {
        bindings: vec![(name.clone(), val)],
        body: vec![Expr::If {
            cond: Box::new(Expr::Var(name)),
            then: Box::new(then),
            els:  Box::new(els),
        }],
    })
}

fn lower_when_let(args: &[EdnValue]) -> Result<Expr, CljError> {
    // (when-let [name expr] body…) ≡ (let [name expr] (if name (do body…) 0))
    let (name_v, expr_v) = binding_pair(args, "when-let")?;
    let name = sym_name(name_v, "when-let binding name")?;
    let val = lower_expr(expr_v)?;
    let body = args[1..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    Ok(Expr::Let {
        bindings: vec![(name.clone(), val)],
        body: vec![Expr::If {
            cond: Box::new(Expr::Var(name)),
            then: Box::new(Expr::Do(body)),
            els:  Box::new(Expr::Int(0)),
        }],
    })
}

fn lower_dotimes(args: &[EdnValue]) -> Result<Expr, CljError> {
    // (dotimes [i n] body…) — run body n times with i = 0..n-1.
    // ≡ (let [n* n] (loop [i 0] (if (< i n*) (do body… (recur (inc i))) 0)))
    let (i_v, n_v) = binding_pair(args, "dotimes")?;
    let i = sym_name(i_v, "dotimes binding name")?;
    let n = lower_expr(n_v)?;
    let nsym = gensym("n");
    let mut body = args[1..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    body.push(Expr::Recur(vec![Expr::Builtin { op: Builtin::Inc, args: vec![Expr::Var(i.clone())] }]));
    Ok(Expr::Let {
        bindings: vec![(nsym.clone(), n)],
        body: vec![Expr::Loop {
            bindings: vec![(i.clone(), Expr::Int(0))],
            body: vec![Expr::If {
                cond: Box::new(Expr::Builtin { op: Builtin::Lt, args: vec![Expr::Var(i.clone()), Expr::Var(nsym)] }),
                then: Box::new(Expr::Do(body)),
                els:  Box::new(Expr::Int(0)),
            }],
        }],
    })
}

/// Fold a sequence of (name, value) rebindings into NESTED single-binding lets, so each value sees
/// the previous binding of the same name (guaranteed-sequential shadowing) — the substrate for as->
/// and cond->.
fn nest_lets(bindings: Vec<(String, Expr)>, body: Expr) -> Expr {
    let mut acc = body;
    for (name, val) in bindings.into_iter().rev() {
        acc = Expr::Let { bindings: vec![(name, val)], body: vec![acc] };
    }
    acc
}

fn lower_as_thread(args: &[EdnValue]) -> Result<Expr, CljError> {
    // (as-> expr name form…) — rebind `name` through each form, returning the last value.
    if args.len() < 2 {
        return Err(CljError::Lower("as-> takes: (as-> expr name form…)".into()));
    }
    let name = sym_name(&args[1], "as-> binding name")?;
    let mut bindings = vec![(name.clone(), lower_expr(&args[0])?)];
    for form in &args[2..] {
        bindings.push((name.clone(), lower_expr(form)?));
    }
    Ok(nest_lets(bindings, Expr::Var(name)))
}

fn lower_cond_thread(args: &[EdnValue], first: bool) -> Result<Expr, CljError> {
    // (cond-> expr t1 f1 …) — thread expr through f_i only when t_i is truthy (cond->> = thread-last).
    if args.is_empty() {
        return Err(CljError::Lower("cond-> takes: (cond-> expr test form…)".into()));
    }
    let rest = &args[1..];
    if rest.len() % 2 != 0 {
        return Err(CljError::Lower("cond-> requires test/form pairs".into()));
    }
    let name = gensym("ct");
    let mut bindings = vec![(name.clone(), lower_expr(&args[0])?)];
    for pair in rest.chunks_exact(2) {
        let test = lower_expr(&pair[0])?;
        let threaded = thread_into(&pair[1], Expr::Var(name.clone()), first)?;
        bindings.push((name.clone(), Expr::If {
            cond: Box::new(test),
            then: Box::new(threaded),
            els:  Box::new(Expr::Var(name.clone())),
        }));
    }
    Ok(nest_lets(bindings, Expr::Var(name)))
}

fn lower_if(args: &[EdnValue]) -> Result<Expr, CljError> {
    if args.len() != 3 {
        return Err(CljError::Lower("if takes: (if cond then else)".into()));
    }
    Ok(Expr::If {
        cond: Box::new(lower_expr(&args[0])?),
        then: Box::new(lower_expr(&args[1])?),
        els:  Box::new(lower_expr(&args[2])?),
    })
}

fn lower_when(args: &[EdnValue]) -> Result<Expr, CljError> {
    if args.is_empty() {
        return Err(CljError::Lower("when takes: (when cond body…)".into()));
    }
    let cond = lower_expr(&args[0])?;
    let body = args[1..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    Ok(Expr::If {
        cond: Box::new(cond),
        then: Box::new(Expr::Do(body)),
        els:  Box::new(Expr::Int(0)),
    })
}

fn lower_let(args: &[EdnValue]) -> Result<Expr, CljError> {
    let binding_vec = match args.first() {
        Some(EdnValue::Vector(v)) => v,
        _ => return Err(CljError::Lower("let requires a binding vector".into())),
    };
    if binding_vec.len() % 2 != 0 {
        return Err(CljError::Lower("let binding vector must have an even number of forms".into()));
    }
    let mut bindings = Vec::with_capacity(binding_vec.len() / 2);
    let mut it = binding_vec.iter();
    while let (Some(name), Some(val)) = (it.next(), it.next()) {
        bindings.push((sym_name(name, "let binding name")?, lower_expr(val)?));
    }
    let body = args[1..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    if body.is_empty() {
        return Err(CljError::Lower("let requires at least one body expression".into()));
    }
    Ok(Expr::Let { bindings, body })
}

fn lower_cond(args: &[EdnValue]) -> Result<Expr, CljError> {
    if args.len() % 2 != 0 {
        return Err(CljError::Lower("cond requires an even number of test/expr forms".into()));
    }
    let mut acc = Expr::Int(0);
    for pair in args.chunks_exact(2).rev() {
        let (test, expr) = (&pair[0], &pair[1]);
        let then = lower_expr(expr)?;
        if is_else_test(test) {
            acc = then;
        } else {
            acc = Expr::If {
                cond: Box::new(lower_expr(test)?),
                then: Box::new(then),
                els:  Box::new(acc),
            };
        }
    }
    Ok(acc)
}

fn is_else_test(v: &EdnValue) -> bool {
    match v {
        EdnValue::Keyword(k) => k.0.name == "else",
        EdnValue::Bool(true) => true,
        _ => false,
    }
}

fn lower_loop(args: &[EdnValue]) -> Result<Expr, CljError> {
    let binding_vec = match args.first() {
        Some(EdnValue::Vector(v)) => v,
        _ => return Err(CljError::Lower("loop requires a binding vector".into())),
    };
    if binding_vec.len() % 2 != 0 {
        return Err(CljError::Lower("loop binding vector must have an even number of forms".into()));
    }
    let mut bindings = Vec::with_capacity(binding_vec.len() / 2);
    let mut it = binding_vec.iter();
    while let (Some(name), Some(val)) = (it.next(), it.next()) {
        bindings.push((sym_name(name, "loop binding name")?, lower_expr(val)?));
    }
    let body = args[1..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    if body.is_empty() {
        return Err(CljError::Lower("loop requires at least one body expression".into()));
    }
    Ok(Expr::Loop { bindings, body })
}

/// `(doseq-entities [e tag] body…)` — iterate every entity tagged `tag`,
/// binding each entity-id to `e` for the body. The survivors core-loop sugar
/// (enemy AI over all enemies, bullet/contact collision, wave checks). No
/// lambdas needed: it desugars to a host-driven cursor loop —
///
/// ```clojure
/// (let [it (query-begin tag)]
///   (loop [e (query-next it)]
///     (when (not= e -1)
///       body…
///       (recur (query-next it)))))
/// ```
fn lower_doseq_entities(args: &[EdnValue]) -> Result<Expr, CljError> {
    if args.len() < 2 {
        return Err(CljError::Lower(
            "doseq-entities requires: (doseq-entities [e tag] body…)".into(),
        ));
    }
    let binding = match &args[0] {
        EdnValue::Vector(v) if v.len() == 2 => v,
        _ => {
            return Err(CljError::Lower(
                "doseq-entities binding must be `[entity-sym tag]`".into(),
            ))
        }
    };
    let evar = sym_name(&binding[0], "doseq-entities entity binding")?;
    let tag = lower_expr(&binding[1])?;
    let it = gensym("it");

    let next = || Expr::Builtin {
        op: Builtin::QueryNext,
        args: vec![Expr::Var(it.clone())],
    };

    let mut body = args[1..].iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
    if body.is_empty() {
        return Err(CljError::Lower("doseq-entities requires at least one body form".into()));
    }
    body.push(Expr::Recur(vec![next()]));

    let loop_expr = Expr::Loop {
        bindings: vec![(evar.clone(), next())],
        body: vec![Expr::If {
            cond: Box::new(Expr::Builtin {
                op: Builtin::NotEq,
                args: vec![Expr::Var(evar), Expr::Int(-1)],
            }),
            then: Box::new(Expr::Do(body)),
            els: Box::new(Expr::Int(0)),
        }],
    };

    Ok(Expr::Let {
        bindings: vec![(
            it.clone(),
            Expr::Builtin { op: Builtin::QueryBegin, args: vec![tag] },
        )],
        body: vec![loop_expr],
    })
}

fn check_builtin_arity(op: Builtin, n: usize) -> Result<(), CljError> {
    use Builtin::*;
    let ok = match op {
        Not | Inc | Dec | Abs | Zero | Pos | Neg
        | StrLen | BytesAlloc | BytesLen | BytesFinish
        | Alloc | Load64 | Load32
        | F32Bits | BitsF32
        | DespawnEntity
        | GetX | GetY | GetZ | GetVx | GetVy | GetVz
        | GetRx | GetRy | GetRz | GetRw
        | PlaySound | StopSound
        | KeyDown | KeyPressed | Axis
        | RandInt | QueryBegin | QueryNext | CountTagged
        | RtEnable
        | SpawnEntity => n == 1,

        PointerX | PointerY | DeltaMs | ElapsedMs | TickN => n == 0,

        Store64 | Store32 | ByteAt | ByteAppend => n == 2,

        MoveToward => n == 3,

        SetPosition | SetVelocity | ApplyImpulse | ApplyForce
        | DrawMesh | SpawnParticle | PlaySoundAt
        | NearestTagged => n == 4,

        SetRotation => n == 5,
        Raycast     => n == 6,
        SetListener => n == 6,
        DrawLine    => n == 7,

        Sub => n >= 1,
        Add | Mul | And | Or => n >= 1,
        Div | Mod => n == 2,
        Eq | NotEq | Lt | Gt | Le | Ge => n >= 1,
        // f32 arithmetic: `+f`/`*f` fold from one operand; `-f`/`/f` need two.
        FAdd | FMul => n >= 1,
        FSub | FDiv => n >= 2,
        FLt | FGt | FLe | FGe | FEq => n >= 1,
    };
    if ok {
        Ok(())
    } else {
        Err(CljError::Lower(format!(
            "builtin {op:?} called with wrong number of arguments ({n})"
        )))
    }
}

fn sym_name(v: &EdnValue, ctx: &str) -> Result<String, CljError> {
    match v {
        EdnValue::Symbol(s) => Ok(s.to_qualified()),
        other => Err(CljError::Lower(format!(
            "{ctx} must be a symbol, found {other:?}"
        ))),
    }
}
