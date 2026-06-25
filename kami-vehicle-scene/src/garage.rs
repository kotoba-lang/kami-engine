//! garage — EDN authoring surface for `kami-vehicle`'s GARAGE + POWERTRAIN.
//!
//! The data-tier counterpart to the ground loader (`crate`): it turns the
//! canonical `:vehicle/*` EDN (engine torque curves, gearbox ratios, Pacejka
//! tire coefficients, and the 6 garage vehicle `SedanSpec`s) into the real
//! `kami_vehicle` engine structs, re-using the tolerant `kami-scene` accessors
//! the same way `scene.edn` is parsed (missing keys fall back to defaults,
//! namespaced keywords match on `ns/name`, ints coerce to floats).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot physics stays native Rust. The garage specs + powertrain/tire tables are
//! **build-time CONFIG** — read once when a [`kami_vehicle::Vehicle`] is
//! constructed, never touched by the 2 kHz solver — so they are safe to move to
//! EDN. `kami-vehicle`'s compiled-in `VehicleKind::spec`, `TorqueCurve::*`,
//! `Gearbox::manual_6`, and `PacejkaParams::*` remain the parity oracle + the
//! `builtin_*` fallback and are parity-tested against the shipped [`GARAGE_EDN`].
//!
//! ## EDN shape (see `data/garage.edn`)
//!
//! ```edn
//! {:vehicle/engines   {:na-2-0-gasoline {:idle-rpm .. :max-rpm .. :inertia ..
//!                                        :friction .. :torque-curve [[rpm nm] ..]} ..}
//!  :vehicle/gearboxes {:manual-6 {:ratios [..] :final-drive .. :inertia .. :shift-time ..}}
//!  :vehicle/tires     {:road-dry {:b-long .. :c-long .. .. :e-lat ..} :road-wet {..}}
//!  :vehicle/garage    {:sedan {:wheelbase .. :layout :fwd :engine :na-2-0-gasoline ..} ..}}
//! ```

use std::collections::BTreeMap;

use kami_scene::{kw_key, mget, num, EdnValue};
use kami_vehicle::{
    Differential, DrivelineLayout, Engine, Gearbox, PacejkaParams, TorqueCurve, VehicleKind,
};

use crate::Error;

/// The canonical garage + powertrain CONFIG shipped with this crate. This is the
/// source of truth; the compiled-in builders are the parity-tested mirror.
pub const GARAGE_EDN: &str = include_str!("../data/garage.edn");

// ── Engine ──────────────────────────────────────────────────────────────────

/// The EDN-loaded mirror of a [`kami_vehicle::Engine`] preset's tunable fields
/// (the parts that vary between presets — the rest of `Engine` is runtime state).
#[derive(Debug, Clone, PartialEq)]
pub struct EngineSpec {
    /// Idle target RPM.
    pub idle_rpm: f32,
    /// Hard rev limiter.
    pub max_rpm: f32,
    /// Crankshaft moment of inertia (kg·m²).
    pub inertia: f32,
    /// Friction torque magnitude at peak RPM (Nm).
    pub friction: f32,
    /// `(rpm, torque_nm)` sample points, ascending by rpm.
    pub torque_curve: Vec<(f32, f32)>,
}

impl EngineSpec {
    /// Build from a real [`kami_vehicle::Engine`] (the `builtin_*()` oracle).
    pub fn from_engine(e: &Engine) -> Self {
        Self {
            idle_rpm: e.idle_rpm,
            max_rpm: e.max_rpm,
            inertia: e.inertia,
            friction: e.friction,
            torque_curve: e.torque_curve.points.clone(),
        }
    }

    /// Produce the real [`TorqueCurve`] this spec describes.
    pub fn torque_curve(&self) -> TorqueCurve {
        TorqueCurve {
            points: self.torque_curve.clone(),
        }
    }

    /// Build a real [`Engine`] from this spec (curve + the four scalar params;
    /// runtime fields `omega`/`running` take `Engine::new`'s idle-spun defaults).
    pub fn to_engine(&self) -> Engine {
        let mut e = Engine::new(self.torque_curve());
        e.idle_rpm = self.idle_rpm;
        e.max_rpm = self.max_rpm;
        e.inertia = self.inertia;
        e.friction = self.friction;
        e
    }
}

// ── Gearbox ─────────────────────────────────────────────────────────────────

/// The EDN-loaded mirror of a [`kami_vehicle::Gearbox`] preset's tunable fields.
#[derive(Debug, Clone, PartialEq)]
pub struct GearboxSpec {
    /// Gear ratios (index 0 = reverse, 1 = neutral, then forward gears).
    pub ratios: Vec<f32>,
    /// Final-drive ratio.
    pub final_drive: f32,
    /// Driveline rotational inertia downstream of the gearbox (kg·m²).
    pub inertia: f32,
    /// Shift time constant (s).
    pub shift_time: f32,
}

impl GearboxSpec {
    /// Build from a real [`kami_vehicle::Gearbox`] (the `builtin_*()` oracle).
    pub fn from_gearbox(g: &Gearbox) -> Self {
        Self {
            ratios: g.ratios.clone(),
            final_drive: g.final_drive,
            inertia: g.inertia,
            shift_time: g.shift_time,
        }
    }

    /// Build a real [`Gearbox`] from this spec (the parametric fields; runtime
    /// state `current_gear`/`shift_progress` take `Gearbox::manual_6` defaults).
    pub fn to_gearbox(&self) -> Gearbox {
        let mut g = Gearbox::manual_6();
        g.ratios = self.ratios.clone();
        g.final_drive = self.final_drive;
        g.inertia = self.inertia;
        g.shift_time = self.shift_time;
        g
    }
}

// ── Tire ────────────────────────────────────────────────────────────────────

/// The EDN-loaded mirror of a [`kami_vehicle::PacejkaParams`] preset — all 8
/// long+lat magic-formula coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TireSpec {
    pub b_long: f32,
    pub c_long: f32,
    pub d_long: f32,
    pub e_long: f32,
    pub b_lat: f32,
    pub c_lat: f32,
    pub d_lat: f32,
    pub e_lat: f32,
}

impl TireSpec {
    /// Build from a real [`PacejkaParams`] (the `builtin_*()` oracle).
    pub fn from_pacejka(p: &PacejkaParams) -> Self {
        Self {
            b_long: p.b_long,
            c_long: p.c_long,
            d_long: p.d_long,
            e_long: p.e_long,
            b_lat: p.b_lat,
            c_lat: p.c_lat,
            d_lat: p.d_lat,
            e_lat: p.e_lat,
        }
    }

    /// Produce the real [`PacejkaParams`] this spec describes.
    pub fn to_pacejka(&self) -> PacejkaParams {
        PacejkaParams {
            b_long: self.b_long,
            c_long: self.c_long,
            d_long: self.d_long,
            e_long: self.e_long,
            b_lat: self.b_lat,
            c_lat: self.c_lat,
            d_lat: self.d_lat,
            e_lat: self.e_lat,
        }
    }
}

// ── Garage vehicle spec ──────────────────────────────────────────────────────

/// EDN driveline-layout authoring: `:fwd` / `:rwd` / `{:awd {:front-split ..}}`.
/// Mirror of [`kami_vehicle::DrivelineLayout`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutSpec {
    Fwd,
    Rwd,
    Awd { front_split: f32 },
}

impl LayoutSpec {
    /// Build from a real [`DrivelineLayout`] (the `builtin_*()` oracle).
    pub fn from_layout(l: DrivelineLayout) -> Self {
        match l {
            DrivelineLayout::Fwd => LayoutSpec::Fwd,
            DrivelineLayout::Rwd => LayoutSpec::Rwd,
            DrivelineLayout::Awd { front_split } => LayoutSpec::Awd { front_split },
        }
    }

    /// Produce the real [`DrivelineLayout`].
    pub fn to_layout(self) -> DrivelineLayout {
        match self {
            LayoutSpec::Fwd => DrivelineLayout::Fwd,
            LayoutSpec::Rwd => DrivelineLayout::Rwd,
            LayoutSpec::Awd { front_split } => DrivelineLayout::Awd { front_split },
        }
    }
}

/// The EDN-loaded mirror of one garage vehicle: the full [`kami_vehicle`]
/// `SedanSpec` param set plus the per-kind powertrain/tire override ids.
#[derive(Debug, Clone, PartialEq)]
pub struct GarageSpec {
    // ── SedanSpec geometry / mass ──
    pub wheelbase: f32,
    pub track_width: f32,
    pub ride_height: f32,
    pub roof_height: f32,
    pub overhang_front: f32,
    pub overhang_rear: f32,
    pub mass_chassis: f32,
    pub mass_engine: f32,
    pub mass_cabin: f32,
    pub wheel_radius: f32,
    pub wheel_width: f32,
    pub layout: LayoutSpec,
    pub turbo: bool,

    // ── Powertrain / tire override ids (resolve against the other tables) ──
    /// Which `:vehicle/engines` preset this car uses.
    pub engine: String,
    /// Which `:vehicle/gearboxes` preset this car uses.
    pub gearbox: String,
    /// Per-kind gearbox final-drive override (build() sets this).
    pub final_drive: f32,
    /// Differential kind id (e.g. `"open"`).
    pub diff: String,
    /// Which `:vehicle/tires` preset this car's wheels start from.
    pub tire: String,
    /// Per-kind effective rev-limit. When the EDN omits `:max-rpm` the car keeps
    /// the engine preset's own `max_rpm`; build() overrides it for some kinds.
    pub max_rpm: f32,
    /// Per-kind sticky-tire d_long override (sports = 1.20). `None` = use preset.
    pub tire_d_long: Option<f32>,
    /// Per-kind sticky-tire d_lat override. `None` = use preset.
    pub tire_d_lat: Option<f32>,
}

// ── Loaders ──────────────────────────────────────────────────────────────────

/// Resolve the `:vehicle/<table>` sub-map of the root, or [`Error::NoTable`].
fn vehicle_table<'a>(
    root: &'a BTreeMap<EdnValue, EdnValue>,
    table: &'static str,
) -> Result<&'a BTreeMap<EdnValue, EdnValue>, Error> {
    mget(root, &format!("vehicle/{table}"))
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable(table))
}

/// Parse the root map of EDN `src`, or [`Error::NotAMap`].
fn root(src: &str) -> Result<BTreeMap<EdnValue, EdnValue>, Error> {
    kami_scene::root_map(src).ok_or(Error::NotAMap)
}

/// Load the `:vehicle/engines` table → `id -> EngineSpec`.
pub fn engines_from_edn(src: &str) -> Result<BTreeMap<String, EngineSpec>, Error> {
    let root = root(src)?;
    let table = vehicle_table(&root, "engines")?;
    let mut out = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(m) = v.as_map() else { continue };
        out.insert(
            id,
            EngineSpec {
                idle_rpm: num(mget(m, "idle-rpm")),
                max_rpm: num(mget(m, "max-rpm")),
                inertia: num(mget(m, "inertia")),
                friction: num(mget(m, "friction")),
                torque_curve: pairs(mget(m, "torque-curve")),
            },
        );
    }
    Ok(out)
}

/// Load the `:vehicle/gearboxes` table → `id -> GearboxSpec`.
pub fn gearboxes_from_edn(src: &str) -> Result<BTreeMap<String, GearboxSpec>, Error> {
    let root = root(src)?;
    let table = vehicle_table(&root, "gearboxes")?;
    let mut out = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(m) = v.as_map() else { continue };
        out.insert(
            id,
            GearboxSpec {
                ratios: floats(mget(m, "ratios")),
                final_drive: num(mget(m, "final-drive")),
                inertia: num(mget(m, "inertia")),
                shift_time: num(mget(m, "shift-time")),
            },
        );
    }
    Ok(out)
}

/// Load the `:vehicle/tires` table → `id -> TireSpec`.
pub fn tires_from_edn(src: &str) -> Result<BTreeMap<String, TireSpec>, Error> {
    let root = root(src)?;
    let table = vehicle_table(&root, "tires")?;
    let mut out = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(m) = v.as_map() else { continue };
        out.insert(
            id,
            TireSpec {
                b_long: num(mget(m, "b-long")),
                c_long: num(mget(m, "c-long")),
                d_long: num(mget(m, "d-long")),
                e_long: num(mget(m, "e-long")),
                b_lat: num(mget(m, "b-lat")),
                c_lat: num(mget(m, "c-lat")),
                d_lat: num(mget(m, "d-lat")),
                e_lat: num(mget(m, "e-lat")),
            },
        );
    }
    Ok(out)
}

/// Load the `:vehicle/garage` table → `id -> GarageSpec`.
pub fn garage_from_edn(src: &str) -> Result<BTreeMap<String, GarageSpec>, Error> {
    let root = root(src)?;
    let table = vehicle_table(&root, "garage")?;
    let mut out = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(m) = v.as_map() else { continue };

        // `:max-rpm` is optional: when absent the car keeps its engine preset's
        // max_rpm (0.0 sentinel here is resolved by callers that have the engine
        // table — but for round-trip parity we leave it as the authored value).
        let max_rpm = mget(m, "max-rpm").map(|x| num(Some(x))).unwrap_or(0.0);

        out.insert(
            id,
            GarageSpec {
                wheelbase: num(mget(m, "wheelbase")),
                track_width: num(mget(m, "track-width")),
                ride_height: num(mget(m, "ride-height")),
                roof_height: num(mget(m, "roof-height")),
                overhang_front: num(mget(m, "overhang-front")),
                overhang_rear: num(mget(m, "overhang-rear")),
                mass_chassis: num(mget(m, "mass-chassis")),
                mass_engine: num(mget(m, "mass-engine")),
                mass_cabin: num(mget(m, "mass-cabin")),
                wheel_radius: num(mget(m, "wheel-radius")),
                wheel_width: num(mget(m, "wheel-width")),
                layout: layout_from_value(mget(m, "layout")),
                turbo: mget(m, "turbo").and_then(|x| x.as_bool()).unwrap_or(false),
                engine: kw_name(mget(m, "engine")),
                gearbox: kw_name(mget(m, "gearbox")),
                final_drive: num(mget(m, "final-drive")),
                diff: kw_name(mget(m, "diff")),
                tire: kw_name(mget(m, "tire")),
                max_rpm,
                tire_d_long: mget(m, "tire-d-long").map(|x| num(Some(x))),
                tire_d_lat: mget(m, "tire-d-lat").map(|x| num(Some(x))),
            },
        );
    }
    Ok(out)
}

// ── EDN → real Vehicle (the data-driven build path) ──────────────────────────

impl GarageSpec {
    /// Map this garage spec's geometry/mass/layout fields onto the real
    /// [`kami_vehicle`] `SedanSpec` (the input to `sedan()`). Powertrain/tire
    /// override ids are NOT part of `SedanSpec` — they are applied afterward by
    /// [`build_from_spec`].
    pub fn to_sedan_spec(&self) -> kami_vehicle::models::sedan::SedanSpec {
        kami_vehicle::models::sedan::SedanSpec {
            wheelbase: self.wheelbase,
            track_width: self.track_width,
            ride_height: self.ride_height,
            roof_height: self.roof_height,
            overhang_front: self.overhang_front,
            overhang_rear: self.overhang_rear,
            mass_chassis: self.mass_chassis,
            mass_engine: self.mass_engine,
            mass_cabin: self.mass_cabin,
            wheel_radius: self.wheel_radius,
            wheel_width: self.wheel_width,
            layout: self.layout.to_layout(),
            turbo: self.turbo,
        }
    }
}

/// Build a real [`kami_vehicle::Vehicle`] from a [`GarageSpec`] + the resolved
/// powertrain/tire tables — the DATA-DRIVEN counterpart of
/// `kami_vehicle::models::garage::build(kind)`.
///
/// This replicates `build()` faithfully but sources every per-kind override from
/// the EDN tables instead of inline Rust literals:
///   1. `sedan(&spec.to_sedan_spec())` → base soft-body car,
///   2. `v.name = name`, `v.enable_rigid_chassis()`,
///   3. engine torque-curve from `engines[&spec.engine]`,
///   4. effective `max_rpm` from `spec.max_rpm` (when authored, i.e. non-zero),
///   5. gearbox `final_drive` from `spec.final_drive`,
///   6. tire from `tires[&spec.tire]` with `spec.tire_d_long`/`tire_d_lat`
///      overrides applied to every wheel when `Some`.
///
/// Overrides are applied to match `build()`'s observable result: the engine
/// curve is always re-set from the named preset (sedan's `na-2-0-gasoline`
/// preset == `sedan()`'s own default curve, so it is a no-op for the sedan), the
/// gearbox `final_drive` is always set from the spec (sedan's EDN value == the
/// `manual_6` default 4.10, a no-op), `max_rpm` is set only when the EDN authors
/// it (sedan/suv omit it → keep the preset), and the sticky-tire `d_long`/`d_lat`
/// are applied only when authored (sports only) — exactly mirroring `build()`.
///
/// Returns [`Error::NoTable`] if the engine or tire id is missing from its table.
pub fn build_from_spec(
    name: &str,
    spec: &GarageSpec,
    engines: &BTreeMap<String, EngineSpec>,
    _gearboxes: &BTreeMap<String, GearboxSpec>,
    tires: &BTreeMap<String, TireSpec>,
) -> Result<kami_vehicle::Vehicle, Error> {
    let mut v = kami_vehicle::models::sedan::sedan(&spec.to_sedan_spec());
    v.name = name.to_string();
    v.enable_rigid_chassis();

    // ── Engine: torque-curve from the named preset, max_rpm from the spec ──
    let engine = engines
        .get(&spec.engine)
        .ok_or(Error::NoTable("engines"))?;
    v.powertrain.engine.torque_curve = engine.torque_curve();
    // The EDN authors `:max-rpm` only when build() overrides the preset's own
    // limit (hatchback/sports/pickup/bus); 0.0 = "keep the engine preset".
    if spec.max_rpm != 0.0 {
        v.powertrain.engine.max_rpm = spec.max_rpm;
    } else {
        v.powertrain.engine.max_rpm = engine.max_rpm;
    }

    // ── Gearbox final-drive override (always authored per kind) ──
    v.powertrain.gearbox.final_drive = spec.final_drive;

    // ── Tire: base preset + per-kind sticky d_long/d_lat overrides ──
    // build() re-sets the tire to road_dry for Sports then sticks d_long/d_lat;
    // for non-sports cars sedan()'s wheels already carry road_dry, so re-setting
    // from the (identical) preset is a no-op. To be faithful AND robust, only
    // re-apply the base preset when a sticky override is present (sports) — that
    // is the sole case build() touches the wheels.
    if spec.tire_d_long.is_some() || spec.tire_d_lat.is_some() {
        let tire = tires.get(&spec.tire).ok_or(Error::NoTable("tires"))?;
        let base = tire.to_pacejka();
        for w in v.wheels.iter_mut() {
            w.tire = base;
            if let Some(d) = spec.tire_d_long {
                w.tire.d_long = d;
            }
            if let Some(d) = spec.tire_d_lat {
                w.tire.d_lat = d;
            }
        }
    }

    Ok(v)
}

/// Build a real [`kami_vehicle::Vehicle`] from the shipped [`GARAGE_EDN`] tables,
/// resolving `kind_id` (hyphen/underscore tolerant) → [`GarageSpec`] →
/// [`build_from_spec`]. This is the data-driven equivalent of
/// `kami_vehicle::build_vehicle(VehicleKind::from_id(kind_id))`.
pub fn build_from_edn(kind_id: &str) -> Result<kami_vehicle::Vehicle, Error> {
    let id = kind_id.replace('_', "-");
    let garage = garage_from_edn(GARAGE_EDN)?;
    let engines = engines_from_edn(GARAGE_EDN)?;
    let gearboxes = gearboxes_from_edn(GARAGE_EDN)?;
    let tires = tires_from_edn(GARAGE_EDN)?;

    // garage table is keyed on the bare keyword name (`sedan`, `hatchback`, …).
    // Accept both the hyphen and underscore forms of the kind id.
    let (key, spec) = garage
        .get(&id)
        .map(|s| (id.clone(), s))
        .or_else(|| garage.get(kind_id).map(|s| (kind_id.to_string(), s)))
        .ok_or(Error::MapNotFound(kind_id.to_string()))?;

    build_from_spec(&key, spec, &engines, &gearboxes, &tires)
}

// ── builtin oracles (the hardcoded Rust source of truth) ─────────────────────

impl GarageSpec {
    /// The compiled-in mirror for one [`VehicleKind`], assembled from the real
    /// Rust source of truth: `VehicleKind::spec()` for the SedanSpec geometry,
    /// and `build_vehicle(kind)` for the per-kind powertrain/tire OVERRIDES that
    /// only `garage::build()` applies (effective `max_rpm`, `final_drive`,
    /// sticky tire `d_long`/`d_lat`). This is what the EDN is parity-tested
    /// against.
    pub fn builtin(kind: VehicleKind) -> GarageSpec {
        let spec = kind.spec();
        // build() applies the powertrain/tire overrides; the built Vehicle is
        // the authoritative oracle for the effective override values.
        let v = kami_vehicle::build_vehicle(kind);
        let tire0 = v.wheels[0].tire;
        let base_dry = PacejkaParams::road_dry();

        // Detect a per-kind sticky-tire override by comparing against road_dry.
        let tire_d_long = (tire0.d_long != base_dry.d_long).then_some(tire0.d_long);
        let tire_d_lat = (tire0.d_lat != base_dry.d_lat).then_some(tire0.d_lat);

        // The EDN authors `:max-rpm` ONLY when build() overrides the engine
        // preset's own max_rpm; otherwise it carries the 0.0 "use preset"
        // sentinel. Mirror that here so the builtin oracle round-trips the EDN
        // authoring convention exactly: effective max_rpm differs from the
        // engine preset → record the override, else the 0.0 sentinel.
        let preset_max_rpm = builtin_engine(engine_id_for(kind))
            .map(|e| e.max_rpm)
            .unwrap_or(0.0);
        let effective_max_rpm = v.powertrain.engine.max_rpm;
        let max_rpm = if effective_max_rpm != preset_max_rpm {
            effective_max_rpm
        } else {
            0.0
        };

        GarageSpec {
            wheelbase: spec.wheelbase,
            track_width: spec.track_width,
            ride_height: spec.ride_height,
            roof_height: spec.roof_height,
            overhang_front: spec.overhang_front,
            overhang_rear: spec.overhang_rear,
            mass_chassis: spec.mass_chassis,
            mass_engine: spec.mass_engine,
            mass_cabin: spec.mass_cabin,
            wheel_radius: spec.wheel_radius,
            wheel_width: spec.wheel_width,
            layout: LayoutSpec::from_layout(spec.layout),
            turbo: spec.turbo,
            engine: engine_id_for(kind).to_string(),
            gearbox: "manual-6".to_string(),
            final_drive: v.powertrain.gearbox.final_drive,
            diff: "open".to_string(),
            tire: "road-dry".to_string(),
            max_rpm,
            tire_d_long,
            tire_d_lat,
        }
    }
}

/// The EDN engine-preset id each kind resolves to (matches `garage.rs::build`).
fn engine_id_for(kind: VehicleKind) -> &'static str {
    match kind {
        VehicleKind::Sedan | VehicleKind::Hatchback => "na-2-0-gasoline",
        VehicleKind::Suv | VehicleKind::Sports => "turbo-2-0",
        VehicleKind::Pickup => "pickup-v6",
        VehicleKind::Bus => "bus-diesel",
    }
}

/// All six [`VehicleKind`]s — the iteration source for parity tests.
pub const ALL_VEHICLE_KINDS: [VehicleKind; 6] = [
    VehicleKind::Sedan,
    VehicleKind::Hatchback,
    VehicleKind::Suv,
    VehicleKind::Sports,
    VehicleKind::Pickup,
    VehicleKind::Bus,
];

/// The compiled-in engine preset for one EDN id — the `builtin_*()` oracle. The
/// effective `max_rpm` per kind is `build_vehicle(kind).powertrain.engine`; the
/// raw curve/idle/inertia/friction live on these constructors.
pub fn builtin_engine(id: &str) -> Option<EngineSpec> {
    // The four scalar engine params (idle/inertia/friction, and the *preset*
    // max_rpm) come from `Engine::new`; only the curve varies per id, plus the
    // two presets that build() rev-limits.
    let base = |curve: TorqueCurve| Engine::new(curve);
    match id {
        "na-2-0-gasoline" => Some(EngineSpec::from_engine(&base(TorqueCurve::na_2_0_gasoline()))),
        "turbo-2-0" => Some(EngineSpec::from_engine(&base(TorqueCurve::turbo_2_0()))),
        "pickup-v6" => {
            // build() override: max_rpm 6000 + the inline pickup curve.
            let mut e = base(TorqueCurve {
                points: vec![
                    (800.0, 280.0),
                    (1500.0, 380.0),
                    (2500.0, 480.0),
                    (3500.0, 470.0),
                    (4500.0, 380.0),
                    (5500.0, 250.0),
                    (6000.0, 0.0),
                ],
            });
            e.max_rpm = 6000.0;
            Some(EngineSpec::from_engine(&e))
        }
        "bus-diesel" => {
            // build() override: max_rpm 3600 + the inline diesel curve.
            let mut e = base(TorqueCurve {
                points: vec![
                    (600.0, 600.0),
                    (1200.0, 1100.0),
                    (1800.0, 1200.0),
                    (2400.0, 1100.0),
                    (3000.0, 800.0),
                    (3600.0, 0.0),
                ],
            });
            e.max_rpm = 3600.0;
            Some(EngineSpec::from_engine(&e))
        }
        _ => None,
    }
}

/// The compiled-in `manual-6` gearbox preset — the `builtin_*()` oracle.
pub fn builtin_gearbox(id: &str) -> Option<GearboxSpec> {
    match id {
        "manual-6" => Some(GearboxSpec::from_gearbox(&Gearbox::manual_6())),
        _ => None,
    }
}

/// The compiled-in tire preset — the `builtin_*()` oracle.
pub fn builtin_tire(id: &str) -> Option<TireSpec> {
    match id {
        "road-dry" => Some(TireSpec::from_pacejka(&PacejkaParams::road_dry())),
        "road-wet" => Some(TireSpec::from_pacejka(&PacejkaParams::road_wet())),
        _ => None,
    }
}

/// Convenience: load the engines table from the shipped [`GARAGE_EDN`].
pub fn shipped_engines() -> Result<BTreeMap<String, EngineSpec>, Error> {
    engines_from_edn(GARAGE_EDN)
}

/// Convenience: load the garage table from the shipped [`GARAGE_EDN`].
pub fn shipped_garage() -> Result<BTreeMap<String, GarageSpec>, Error> {
    garage_from_edn(GARAGE_EDN)
}

// ── small EDN helpers ────────────────────────────────────────────────────────

/// Read a keyword VALUE's bare/qualified name (e.g. `:road-dry` → `"road-dry"`).
fn kw_name(v: Option<&EdnValue>) -> String {
    v.and_then(kw_key).unwrap_or_default()
}

/// Read a flat numeric vector `[a b c ..]` as `Vec<f32>` (ints coerce).
fn floats(v: Option<&EdnValue>) -> Vec<f32> {
    v.and_then(|x| x.as_vector())
        .map(|s| s.iter().map(|x| num(Some(x))).collect())
        .unwrap_or_default()
}

/// Read a vector of `[a b]` pairs `[[rpm nm] ..]` as `Vec<(f32, f32)>`.
fn pairs(v: Option<&EdnValue>) -> Vec<(f32, f32)> {
    v.and_then(|x| x.as_vector())
        .map(|s| {
            s.iter()
                .filter_map(|p| {
                    let inner = p.as_vector()?;
                    Some((
                        num(inner.first()),
                        num(inner.get(1)),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Read a driveline `:layout` VALUE: `:fwd` / `:rwd` / `{:awd {:front-split ..}}`.
fn layout_from_value(v: Option<&EdnValue>) -> LayoutSpec {
    let Some(v) = v else { return LayoutSpec::Fwd };
    if let Some(kw) = kw_key(v) {
        return match kw.as_str() {
            "rwd" => LayoutSpec::Rwd,
            "awd" => LayoutSpec::Awd { front_split: 0.0 },
            _ => LayoutSpec::Fwd,
        };
    }
    // Map form `{:awd {:front-split ..}}`.
    if let Some(m) = v.as_map() {
        if let Some(awd) = mget(m, "awd").and_then(|x| x.as_map()) {
            return LayoutSpec::Awd {
                front_split: num(mget(awd, "front-split")),
            };
        }
    }
    LayoutSpec::Fwd
}

// Re-export the diff resolver so callers can turn the `:diff` id into a real
// `Differential`. Currently only `open` is authored in the garage; the LSD diffs
// are applied by `sedan()` itself for AWD layouts (not a per-kind override).
/// Resolve a `:diff` id to a real [`Differential`] (`"open"` → open; default open).
pub fn differential_from_id(id: &str) -> Differential {
    match id {
        "open" => Differential::open(),
        _ => Differential::open(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engines_table_loads_all_four() {
        let m = engines_from_edn(GARAGE_EDN).expect("engines parse");
        assert_eq!(m.len(), 4);
        assert!(m.contains_key("na-2-0-gasoline"));
        assert!(m.contains_key("bus-diesel"));
    }

    #[test]
    fn layout_value_forms_parse() {
        assert_eq!(layout_from_value(None), LayoutSpec::Fwd);
        let m = kami_scene::root_map("{:l :rwd}").unwrap();
        assert_eq!(layout_from_value(mget(&m, "l")), LayoutSpec::Rwd);
        let m = kami_scene::root_map("{:l {:awd {:front-split 0.45}}}").unwrap();
        assert_eq!(
            layout_from_value(mget(&m, "l")),
            LayoutSpec::Awd { front_split: 0.45 }
        );
    }

    #[test]
    fn missing_table_is_an_error() {
        let err = engines_from_edn("{:other 1}").unwrap_err();
        assert!(matches!(err, Error::NoTable("engines")));
    }
}
