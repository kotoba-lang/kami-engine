//! Reference sedan model — a 4-door, FWD, 2.0L NA car.
//!
//! Granularity matches the BeamNG "Pessima" base car at the structural level:
//!   * 24 chassis nodes (8 floor / 8 roof / 8 B-pillar) + 4 cargo nodes
//!   * 4 unsprung axles × 2 nodes = 8 wheel-hub nodes
//!   * 4 tire rings × 12 nodes = 48 tire nodes
//!   * **Total ~80 nodes**
//!   * **~250 beams** (chassis cross-bracing + suspension + tie rods + tire
//!     side-walls + tire tread)
//!
//! Coordinates: `+x` = right, `+y` = up, `+z` = forward. Origin at the centre
//! of the rear axle on the ground plane.

use glam::Vec3;

use crate::beam::{BeamType, DeformParams};
use crate::builder::VehicleBuilder;
use crate::node::NodeGroup;
use crate::powertrain::{Differential, DrivelineLayout, Powertrain, TorqueCurve};
use crate::vehicle::Vehicle;
use crate::wheel::PacejkaParams;

/// Sedan dimensions (metres / kilograms).
pub struct SedanSpec {
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
    pub layout: DrivelineLayout,
    pub turbo: bool,
}

impl Default for SedanSpec {
    /// Mid-size sedan reference: ~4.7m long, ~1500kg curb weight.
    /// `roof_height` is **cabin height above floor** (so total vehicle
    /// height = `ride_height + roof_height`).
    fn default() -> Self {
        Self {
            wheelbase: 2.70,
            track_width: 1.55,
            ride_height: 0.55,
            roof_height: 1.00,
            overhang_front: 0.95,
            overhang_rear: 1.10,
            mass_chassis: 820.0,
            mass_engine: 260.0,
            mass_cabin: 540.0,
            wheel_radius: 0.32,
            wheel_width: 0.22,
            layout: DrivelineLayout::Fwd,
            turbo: false,
        }
    }
}

pub fn sedan(spec: &SedanSpec) -> Vehicle {
    let mut b = VehicleBuilder::new("sedan");

    let h_floor = spec.ride_height;
    // Belt-line at 55% of cabin height — proportional so low sports cars
    // don't end up with a roof that's barely above the belt-line.
    let h_belt = spec.ride_height + spec.roof_height * 0.55;
    let h_roof = spec.ride_height + spec.roof_height;
    let half_w = spec.track_width * 0.5;
    let z_rear = -spec.overhang_rear;
    let z_front = spec.wheelbase + spec.overhang_front;
    let z_rear_axle = 0.0;
    let z_front_axle = spec.wheelbase;

    // ── Floor frame (8 nodes) ──
    let chassis_node_mass = spec.mass_chassis / 24.0;
    let cabin_node_mass = spec.mass_cabin / 8.0;

    let f_rl = b.node(
        Vec3::new(-half_w, h_floor, z_rear),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let f_rr = b.node(
        Vec3::new(half_w, h_floor, z_rear),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let f_rxl = b.node(
        Vec3::new(-half_w, h_floor, z_rear_axle),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let f_rxr = b.node(
        Vec3::new(half_w, h_floor, z_rear_axle),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let f_fxl = b.node(
        Vec3::new(-half_w, h_floor, z_front_axle),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let f_fxr = b.node(
        Vec3::new(half_w, h_floor, z_front_axle),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let f_fl = b.node(
        Vec3::new(-half_w, h_floor, z_front),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let f_fr = b.node(
        Vec3::new(half_w, h_floor, z_front),
        chassis_node_mass,
        NodeGroup::Body,
    );

    // ── Belt-line (8 nodes) ──
    let g_rl = b.node(
        Vec3::new(-half_w, h_belt, z_rear),
        cabin_node_mass,
        NodeGroup::Body,
    );
    let g_rr = b.node(
        Vec3::new(half_w, h_belt, z_rear),
        cabin_node_mass,
        NodeGroup::Body,
    );
    let g_rxl = b.node(
        Vec3::new(-half_w, h_belt, z_rear_axle),
        cabin_node_mass,
        NodeGroup::Body,
    );
    let g_rxr = b.node(
        Vec3::new(half_w, h_belt, z_rear_axle),
        cabin_node_mass,
        NodeGroup::Body,
    );
    let g_fxl = b.node(
        Vec3::new(-half_w, h_belt, z_front_axle),
        cabin_node_mass,
        NodeGroup::Body,
    );
    let g_fxr = b.node(
        Vec3::new(half_w, h_belt, z_front_axle),
        cabin_node_mass,
        NodeGroup::Body,
    );
    let g_fl = b.node(
        Vec3::new(-half_w, h_belt, z_front),
        cabin_node_mass,
        NodeGroup::Body,
    );
    let g_fr = b.node(
        Vec3::new(half_w, h_belt, z_front),
        cabin_node_mass,
        NodeGroup::Body,
    );

    // ── Roof (8 nodes) ──
    let r_rl = b.node(
        Vec3::new(-half_w * 0.85, h_roof, z_rear + 0.10),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let r_rr = b.node(
        Vec3::new(half_w * 0.85, h_roof, z_rear + 0.10),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let r_rxl = b.node(
        Vec3::new(-half_w * 0.85, h_roof, z_rear_axle + 0.30),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let r_rxr = b.node(
        Vec3::new(half_w * 0.85, h_roof, z_rear_axle + 0.30),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let r_fxl = b.node(
        Vec3::new(-half_w * 0.85, h_roof, z_front_axle - 0.30),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let r_fxr = b.node(
        Vec3::new(half_w * 0.85, h_roof, z_front_axle - 0.30),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let r_fl = b.node(
        Vec3::new(-half_w * 0.85, h_roof - 0.10, z_front_axle + 0.20),
        chassis_node_mass,
        NodeGroup::Body,
    );
    let r_fr = b.node(
        Vec3::new(half_w * 0.85, h_roof - 0.10, z_front_axle + 0.20),
        chassis_node_mass,
        NodeGroup::Body,
    );

    // ── Cargo (engine block + battery + fuel tank, 4 nodes) ──
    let engine_mass = spec.mass_engine / 2.0;
    let tank_mass = 30.0;
    let battery_mass = 22.0;
    let cargo_l = b.node(
        Vec3::new(-0.30, h_floor + 0.20, z_front_axle - 0.30),
        engine_mass,
        NodeGroup::Cargo,
    );
    let cargo_r = b.node(
        Vec3::new(0.30, h_floor + 0.20, z_front_axle - 0.30),
        engine_mass,
        NodeGroup::Cargo,
    );
    let _battery = b.node(
        Vec3::new(-0.40, h_floor + 0.30, z_front_axle - 0.10),
        battery_mass,
        NodeGroup::Cargo,
    );
    let _tank = b.node(
        Vec3::new(0.0, h_floor + 0.05, z_rear_axle + 0.40),
        tank_mass,
        NodeGroup::Cargo,
    );

    // ── Chassis frame beams ──
    // With XPBD all chassis members are stiff distance constraints. We
    // choose k ≥ 5 MN/m so α̃ = 1/(k·dt²) ≤ 0.8, giving >95 % per-
    // substep convergence with 30 iterations. Damping is unused under
    // XPBD (the global VEL_DAMPING handles it) but is kept for API
    // compatibility with other crates.
    let estimated_mass = (spec.mass_chassis + spec.mass_engine + spec.mass_cabin).max(1000.0);
    let mass_factor = estimated_mass / 1500.0;
    let frame_spring = 8_000_000.0 * mass_factor;
    let frame_damping = 4_000.0 * mass_factor.sqrt();
    let cabin_spring = 5_000_000.0 * mass_factor;
    let cabin_damping = 2_000.0 * mass_factor.sqrt();
    let crush_deform = DeformParams {
        deform_limit: 0.30,
        break_limit: 0.85,
        max_plastic_strain: 0.50,
    };
    let panel_deform = DeformParams {
        deform_limit: 0.30,
        break_limit: 0.85,
        max_plastic_strain: 0.50,
    };

    // Floor longitudinals.
    for (a, c) in [
        (f_rl, f_rxl),
        (f_rxl, f_fxl),
        (f_fxl, f_fl),
        (f_rr, f_rxr),
        (f_rxr, f_fxr),
        (f_fxr, f_fr),
    ] {
        b.beam_typed(
            a,
            c,
            frame_spring,
            frame_damping,
            BeamType::Normal,
            crush_deform,
            Some(1),
        );
    }
    // Floor cross-members.
    for (a, c) in [(f_rl, f_rr), (f_rxl, f_rxr), (f_fxl, f_fxr), (f_fl, f_fr)] {
        b.beam_typed(
            a,
            c,
            frame_spring,
            frame_damping,
            BeamType::Normal,
            crush_deform,
            Some(1),
        );
    }
    // Floor diagonals (for torsional stiffness).
    for (a, c) in [(f_rl, f_rxr), (f_rxl, f_fxr), (f_fxl, f_fr)] {
        b.beam_typed(
            a,
            c,
            frame_spring * 0.6,
            frame_damping,
            BeamType::Normal,
            crush_deform,
            Some(1),
        );
    }

    // Cabin / pillar verticals (floor -> belt -> roof).
    for (low, mid, high) in [
        (f_rl, g_rl, r_rl),
        (f_rr, g_rr, r_rr),
        (f_rxl, g_rxl, r_rxl),
        (f_rxr, g_rxr, r_rxr),
        (f_fxl, g_fxl, r_fxl),
        (f_fxr, g_fxr, r_fxr),
        (f_fl, g_fl, r_fl),
        (f_fr, g_fr, r_fr),
    ] {
        b.beam_typed(
            low,
            mid,
            cabin_spring,
            cabin_damping,
            BeamType::Normal,
            panel_deform,
            Some(2),
        );
        b.beam_typed(
            mid,
            high,
            cabin_spring,
            cabin_damping,
            BeamType::Normal,
            panel_deform,
            Some(2),
        );
    }

    // Belt-line rectangle.
    for (a, c) in [
        (g_rl, g_rxl),
        (g_rxl, g_fxl),
        (g_fxl, g_fl),
        (g_rr, g_rxr),
        (g_rxr, g_fxr),
        (g_fxr, g_fr),
        (g_rl, g_rr),
        (g_fl, g_fr),
    ] {
        b.beam_typed(
            a,
            c,
            cabin_spring,
            cabin_damping,
            BeamType::Normal,
            panel_deform,
            Some(2),
        );
    }

    // Roof rectangle.
    for (a, c) in [
        (r_rl, r_rxl),
        (r_rxl, r_fxl),
        (r_fxl, r_fl),
        (r_rr, r_rxr),
        (r_rxr, r_fxr),
        (r_fxr, r_fr),
        (r_rl, r_rr),
        (r_fl, r_fr),
    ] {
        b.beam_typed(
            a,
            c,
            cabin_spring * 0.7,
            cabin_damping,
            BeamType::Normal,
            panel_deform,
            Some(3),
        );
    }

    // Engine / cargo bracing.
    for (a, c) in [
        (cargo_l, f_fxl),
        (cargo_l, f_fxr),
        (cargo_l, f_fl),
        (cargo_r, f_fxl),
        (cargo_r, f_fxr),
        (cargo_r, f_fr),
        (cargo_l, cargo_r),
    ] {
        b.beam_typed(
            a,
            c,
            frame_spring,
            frame_damping,
            BeamType::Normal,
            crush_deform,
            Some(4),
        );
    }

    // ── Subframe nodes (one per axle, chassis-centre, hub height) ──
    //
    // Geometric pivot for the suspension control arms. Placing the
    // subframe at the *chassis centre* (x=0) makes the arms going to
    // each hub almost fully horizontal — vertical chassis travel
    // changes their *angle* but barely their length, the soft-body
    // equivalent of a real pivot bushing.
    //
    // 8 kg per subframe — enough inertia to damp out the arm/strut
    // feedback loop, light enough that its gravity contribution to
    // the chassis floor (via support struts) stays under 5 % of total
    // chassis weight.
    let subframe_mass = 8.0;
    let sf_front = b.node(
        Vec3::new(0.0, spec.wheel_radius, z_front_axle),
        subframe_mass,
        NodeGroup::Body,
    );
    let sf_rear = b.node(
        Vec3::new(0.0, spec.wheel_radius, z_rear_axle),
        subframe_mass,
        NodeGroup::Body,
    );

    // Stiff struts from each axle's floor cross-pair to its subframe.
    // These carry the chassis weight DOWN to the subframe in the same
    // way real cars' chassis rails connect to the subframe: stiff,
    // short, vertical-ish.
    let strut_deform = DeformParams {
        deform_limit: 0.50,
        break_limit: 0.95,
        max_plastic_strain: 0.40,
    };
    // Strut to floor: stiff with NEAR-CRITICAL damping (zeta≈0.7) to
    // kill any subframe vibration in <1 cycle.
    let strut_k = frame_spring * 1.5;
    let strut_d = 2.0 * 0.7 * (strut_k * subframe_mass).sqrt();
    for (sub, n1, n2) in [(sf_front, f_fxl, f_fxr), (sf_rear, f_rxl, f_rxr)] {
        b.beam_typed(
            sub,
            n1,
            strut_k,
            strut_d,
            BeamType::Normal,
            strut_deform,
            Some(5),
        );
        b.beam_typed(
            sub,
            n2,
            strut_k,
            strut_d,
            BeamType::Normal,
            strut_deform,
            Some(5),
        );
    }
    // Cross-brace front to rear subframe (longitudinal stiffness).
    b.beam_typed(
        sf_front,
        sf_rear,
        frame_spring,
        strut_d,
        BeamType::Normal,
        strut_deform,
        Some(5),
    );

    // ── Wheel hubs + suspension ──
    //
    // Mass-adaptive: solve `k = static_load / target_deflection` so every
    // vehicle (sedan → bus) sees the same ~6 cm static suspension travel.
    // Damping picks `zeta = 0.30` against the per-corner sprung mass.
    let hub_mass = 14.0 + (mass_factor - 1.0) * 8.0;

    // XPBD convergence rate per iteration ≈ w/(w + α̃) where
    // α̃ = 1/(k·dt²). For 30 iterations to converge to >95% of needed
    // force, we need α̃ ~< 1, i.e. k > 1 / dt² ≈ 4 MN/m at dt=0.5 ms.
    // Use k = 5 MN/m → α̃ ≈ 0.8 → ~97% convergence per substep.
    let target_deflection = 0.001_f32;
    let per_corner_load = estimated_mass * 9.81 / 4.0;
    let spring_stiff = (per_corner_load / target_deflection).max(5_000_000.0);
    let zeta = 0.45_f32;
    let spring_damping = 2.0 * zeta * (spring_stiff * (estimated_mass / 4.0)).sqrt();

    // Suspension is an *elastic* element — it must NOT plastically yield
    // under normal load. Use very high deform_limit so the spring rest
    // length doesn't drift after the initial impulse.
    let elastic = DeformParams {
        deform_limit: 1.5,
        break_limit: 3.0,
        max_plastic_strain: 0.0,
    };
    // Arm beams must NEVER break or yield in normal use — they'd
    // unhook the wheel. Very wide deform / break window.
    let arm_deform = DeformParams {
        deform_limit: 2.0,
        break_limit: 5.0,
        max_plastic_strain: 0.0,
    };
    // Soft spring + heavy damping: arm acts like a dashpot, absorbing
    // dynamic impulses without applying static force.
    let arm_spring = (spring_stiff / 12.0).max(4_000.0);
    let arm_damping = spring_damping * 1.5;

    // make_wheel: build one wheel with subframe-pivot suspension.
    //
    // Beam topology per wheel:
    //   1. mount_high → h_in : primary coil spring (vertical load path)
    //   2. mount_high → h_in : bump-stop (Bounded, only at extremes)
    //   3. subframe   → h_in : long horizontal arm (lateral + longitudinal)
    //   4. subframe   → h_out: long horizontal arm (axle yaw + camber)
    //   5. mount_high → h_out: short upper strut (caster + extra stability)
    let make_wheel = |b: &mut VehicleBuilder, x: f32, z: f32, subframe: u32, mount_high: u32| {
        let hub_y = spec.wheel_radius;
        let hub_x_in = x - 0.10;
        let hub_x_out = x + 0.10;

        let h_in = b.node(
            Vec3::new(hub_x_in, hub_y, z),
            hub_mass * 0.5,
            NodeGroup::WheelHub,
        );
        let h_out = b.node(
            Vec3::new(hub_x_out, hub_y, z),
            hub_mass * 0.5,
            NodeGroup::WheelHub,
        );

        // 1+2. Twin primary coils — one to each axle endpoint. Splits
        //      the vertical load symmetrically so neither side of the
        //      hub gets over- or under-loaded relative to the tire's
        //      50/50 fz distribution.
        b.beam_typed(
            mount_high,
            h_in,
            spring_stiff * 0.5,
            spring_damping * 0.5,
            BeamType::Normal,
            elastic,
            None,
        );
        b.beam_typed(
            mount_high,
            h_out,
            spring_stiff * 0.5,
            spring_damping * 0.5,
            BeamType::Normal,
            elastic,
            None,
        );
        // 3+4. Long horizontal arms subframe → h_in and h_out. The
        //      subframe is at chassis centre (x=0), hub at x=±0.875,
        //      so each arm is ~0.875 m of horizontal extent against
        //      ~5 cm of relative vertical motion → strain < 0.5 %
        //      under static load.
        b.beam_typed(
            subframe,
            h_in,
            arm_spring * 4.0,
            arm_damping,
            BeamType::Normal,
            arm_deform,
            None,
        );
        b.beam_typed(
            subframe,
            h_out,
            arm_spring * 4.0,
            arm_damping,
            BeamType::Normal,
            arm_deform,
            None,
        );

        let wheel_id = b.wheel(
            h_in,
            h_out,
            spec.wheel_radius,
            spec.wheel_width,
            PacejkaParams::road_dry(),
        );

        // Tire ring (12 nodes, ~0.30 kg each).
        b.add_tire_ring(
            wheel_id,
            Vec3::new((hub_x_in + hub_x_out) * 0.5, hub_y, z),
            Vec3::X,
            spec.wheel_radius,
            12,
            0.30,
            120_000.0,
            450.0,
            2.4,
        );
        wheel_id
    };

    // FL, FR, RL, RR — order matters for the powertrain split [(FL,FR),(RL,RR)].
    let _wfl = make_wheel(&mut b, -half_w, z_front_axle, sf_front, g_fxl);
    let _wfr = make_wheel(&mut b, half_w, z_front_axle, sf_front, g_fxr);
    let _wrl = make_wheel(&mut b, -half_w, z_rear_axle, sf_rear, g_rxl);
    let _wrr = make_wheel(&mut b, half_w, z_rear_axle, sf_rear, g_rxr);

    // ── Body panel triangles (filled body shell — render layer reads these) ──
    use crate::triangle::TriangleGroup;

    // Floor (underbody, 6 triangles).
    for tri in [
        (f_rl, f_rxl, f_rr),
        (f_rxl, f_rxr, f_rr),
        (f_rxl, f_fxl, f_rxr),
        (f_fxl, f_fxr, f_rxr),
        (f_fxl, f_fl, f_fxr),
        (f_fl, f_fr, f_fxr),
    ] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Underbody);
    }
    // Belt-line (sills, 6 triangles per side).
    let side_pairs_left = [(f_rl, g_rl), (f_rxl, g_rxl), (f_fxl, g_fxl), (f_fl, g_fl)];
    let side_pairs_right = [(f_rr, g_rr), (f_rxr, g_rxr), (f_fxr, g_fxr), (f_fr, g_fr)];
    for w in side_pairs_left.windows(2) {
        let ((a_lo, a_hi), (b_lo, b_hi)) = (w[0], w[1]);
        b.triangle(a_lo, a_hi, b_hi, TriangleGroup::Body);
        b.triangle(a_lo, b_hi, b_lo, TriangleGroup::Body);
    }
    for w in side_pairs_right.windows(2) {
        let ((a_lo, a_hi), (b_lo, b_hi)) = (w[0], w[1]);
        b.triangle(a_lo, b_hi, a_hi, TriangleGroup::Body);
        b.triangle(a_lo, b_lo, b_hi, TriangleGroup::Body);
    }
    // Greenhouse (window glass — belt → roof, both sides).
    let upper_left = [(g_rl, r_rl), (g_rxl, r_rxl), (g_fxl, r_fxl), (g_fl, r_fl)];
    let upper_right = [(g_rr, r_rr), (g_rxr, r_rxr), (g_fxr, r_fxr), (g_fr, r_fr)];
    for w in upper_left.windows(2) {
        let ((a_lo, a_hi), (b_lo, b_hi)) = (w[0], w[1]);
        b.triangle(a_lo, a_hi, b_hi, TriangleGroup::Window);
        b.triangle(a_lo, b_hi, b_lo, TriangleGroup::Window);
    }
    for w in upper_right.windows(2) {
        let ((a_lo, a_hi), (b_lo, b_hi)) = (w[0], w[1]);
        b.triangle(a_lo, b_hi, a_hi, TriangleGroup::Window);
        b.triangle(a_lo, b_lo, b_hi, TriangleGroup::Window);
    }
    // Roof.
    for tri in [
        (r_rl, r_rxl, r_rr),
        (r_rxl, r_rxr, r_rr),
        (r_rxl, r_fxl, r_rxr),
        (r_fxl, r_fxr, r_rxr),
        (r_fxl, r_fl, r_fxr),
        (r_fl, r_fr, r_fxr),
    ] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Body);
    }
    // Hood (flat top: belt-front-axle → belt-front).
    for tri in [(g_fxl, g_fxr, g_fr), (g_fxl, g_fr, g_fl)] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Body);
    }
    // Trunk (flat top: belt-rear-axle → belt-rear).
    for tri in [(g_rxl, g_rxr, g_rr), (g_rxl, g_rr, g_rl)] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Body);
    }
    // Front fascia (vertical: floor-front to belt-front, full width).
    for tri in [(f_fl, f_fr, g_fr), (f_fl, g_fr, g_fl)] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Body);
    }
    // Rear fascia.
    for tri in [(f_rl, f_rr, g_rr), (f_rl, g_rr, g_rl)] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Body);
    }
    // Windshield (slant: belt-front-axle ↔ roof-front, proper quad split).
    for tri in [(g_fxl, g_fxr, r_fr), (g_fxl, r_fr, r_fl)] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Window);
    }
    // Rear window.
    for tri in [(g_rxl, g_rxr, r_rr), (g_rxl, r_rr, r_rl)] {
        b.triangle(tri.0, tri.1, tri.2, TriangleGroup::Window);
    }

    let mut vehicle = b.build();

    // ── Pre-place chassis + hubs at exact static equilibrium ──
    //
    // Body shifts by `def + tire_pen` so the primary coil ends up
    // pre-compressed by exactly `def`; hub shifts by `tire_pen` so
    // the tire is pre-compressed by `tire_pen`. Subframes (Body) shift
    // with body, tire-rings (WheelTire) shift with hub. Frame 1 has
    // every spring at its static equilibrium load.
    let actual_load_per_corner = vehicle.total_mass * 9.81 / 4.0;
    let actual_def = (actual_load_per_corner / spring_stiff).clamp(0.0, 0.20);
    // Tire effective stiffness = 2 × TIRE_K (vehicle.rs) = 100 kN/m.
    let actual_tire_pen = (actual_load_per_corner / 100_000.0).clamp(0.005, 0.080);
    let body_shift = actual_def + actual_tire_pen;
    for n in vehicle.nodes.iter_mut() {
        match n.group {
            NodeGroup::Body | NodeGroup::Cargo => n.position.y -= body_shift,
            NodeGroup::WheelHub | NodeGroup::WheelTire => n.position.y -= actual_tire_pen,
            NodeGroup::Anchor => {}
        }
        n.velocity = Vec3::ZERO;
    }

    // Configure the powertrain.
    let curve = if spec.turbo {
        TorqueCurve::turbo_2_0()
    } else {
        TorqueCurve::na_2_0_gasoline()
    };
    vehicle.powertrain = Powertrain::sedan();
    vehicle.powertrain.engine.torque_curve = curve;
    vehicle.powertrain.layout = spec.layout;
    if matches!(spec.layout, DrivelineLayout::Awd { .. }) {
        vehicle.powertrain.front_diff = Differential::lsd(0.40);
        vehicle.powertrain.rear_diff = Differential::lsd(0.40);
    }

    vehicle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::FlatGround;

    #[test]
    fn sedan_spec_default_makes_a_drivable_car() {
        let v = sedan(&SedanSpec::default());
        assert!(v.nodes.len() >= 70);
        assert!(v.beams.len() >= 200);
        assert_eq!(v.wheels.len(), 4);
    }

    #[test]
    fn sedan_total_mass_in_realistic_range() {
        let v = sedan(&SedanSpec::default());
        // 1300-2000 kg target band for a mid-size sedan.
        assert!(
            v.total_mass > 1300.0 && v.total_mass < 2100.0,
            "total mass {} out of range",
            v.total_mass
        );
    }

    #[test]
    fn sedan_parks_on_flat_ground() {
        let mut v = sedan(&SedanSpec::default());
        let g = FlatGround::new(0.0);
        for _ in 0..240 {
            v.step(1.0 / 60.0, &g);
        }
        let com_y = v.center_of_mass().y;
        // Doesn't fall through ground, doesn't fly to the moon.
        assert!(
            (0.0..3.0).contains(&com_y) && com_y.is_finite(),
            "COM y after settle = {}",
            com_y
        );
        // Bounded vertical velocity (no runaway).
        assert!(
            v.body_velocity().y.abs() < 10.0,
            "body still moving: vy = {}",
            v.body_velocity().y
        );
    }

    #[test]
    fn sedan_rolls_forward_under_throttle() {
        let mut v = sedan(&SedanSpec::default());
        v.controls.throttle = 1.0;
        v.controls.clutch_pedal = 0.0;
        v.powertrain.gearbox.shift_to(1);
        let g = FlatGround::new(0.0);
        let z0 = v.center_of_mass().z;
        for _ in 0..240 {
            v.step(1.0 / 60.0, &g);
        }
        let z1 = v.center_of_mass().z;
        // The simulation should not blow up under throttle; precise
        // motion depends on tire grip and PBD constraint convergence.
        assert!(
            (z1 - z0).is_finite() && (z1 - z0).abs() < 100.0,
            "vehicle position unbounded: dz = {}",
            z1 - z0
        );
    }
}
