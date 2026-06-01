//! funadaiku 船大工 — Nagi 凪 class autonomous **zero-emission** voyage.
//!
//! The autonomy brain (`Autopilot` + `ShipHydro`, the kami-autodrive ship GNC)
//! sails a multi-waypoint coastal course in open water, while a reduced-order
//! **zero-emission powertrain** (wind-assist + solar + hydrogen fuel-cell + LFP
//! battery) decides how much propulsion power is actually available each step —
//! and books the energy split by source. There is **no fossil engine**: when the
//! green budget can't meet the commanded thrust, the throttle is *power-limited*
//! and the ship simply sails slower (honest behaviour, never a fossil top-up).
//!
//! This is the kami-engine counterpart of `20-actors/funadaiku/methods/voyage_energy.py`
//! (the analytic budget) and the operational use of ADR-2606013400 + ADR-2606010600.
//!
//! HONEST: the kami-autodrive `ShipHydro` is a small-vessel surrogate (8 m/s,
//! 2 t), not a 3,000 DWT hull; distances/powers are demo-scaled. The energy
//! *shares* are scale-invariant; the kWh figures are demo-scale. 3-DOF planar,
//! not 6-DOF sea-keeping.
//!
//! ```sh
//! cargo run -p kami-autodrive --example nagi_voyage
//! ```

use glam::Vec2;
use kami_autodrive::{Autopilot, AutopilotConfig, DriveState, Plant, Pose2, ShipHydro, VehicleClass};

/// Reduced-order zero-emission powertrain (G13/N5: no fossil engine).
struct Powertrain {
    // Instantaneous source capacities (demo-scaled kW).
    solar_kw: f32,
    h2_max_kw: f32,
    /// Constant auxiliary / hotel base load the powertrain must always serve.
    hotel_kw: f32,
    drive_eff: f32,
    // LFP battery buffer.
    batt_capacity_kwh: f32,
    batt_soc_kwh: f32,
    // True wind in the world frame (direction it blows TOWARD, rad) + speed (m/s).
    wind_dir: f32,
    wind_speed: f32,
    // Energy ledger (kWh).
    e_wind: f32,
    e_solar: f32,
    e_h2: f32,
    h2_kg: f32,
    power_limited_s: f32,
}

impl Powertrain {
    fn nagi() -> Self {
        Self {
            solar_kw: 1.0,
            h2_max_kw: 90.0,
            hotel_kw: 5.0,
            drive_eff: 0.90,
            batt_capacity_kwh: 5.0,
            batt_soc_kwh: 5.0,
            wind_dir: std::f32::consts::FRAC_PI_2, // wind blowing toward +y (a beam wind on the +x leg)
            wind_speed: 9.0,
            e_wind: 0.0,
            e_solar: 0.0,
            e_h2: 0.0,
            h2_kg: 0.0,
            power_limited_s: 0.0,
        }
    }

    /// Wind-assist thrust fraction (0..~0.35) from the apparent-wind angle off the
    /// bow. A rotor/wing sail makes most thrust on a beam reach (~90°), little
    /// head-on or dead astern. Apparent wind folds in the ship's own motion.
    fn wind_thrust_frac(&self, heading: f32, surge: f32) -> f32 {
        // True wind vector (world).
        let (wsx, wsy) = self.wind_dir.sin_cos();
        let tw = Vec2::new(self.wind_speed * wsy, self.wind_speed * wsx);
        // Ship velocity (world), forward only (surge dominant).
        let (s, c) = heading.sin_cos();
        let vship = Vec2::new(surge * c, surge * s);
        // Apparent wind = true wind - ship velocity.
        let aw = tw - vship;
        let aw_speed = aw.length();
        if aw_speed < 0.1 {
            return 0.0;
        }
        // Angle between apparent wind and the ship's heading.
        let head = Vec2::new(c, s);
        let cos_off = (aw.dot(head) / aw_speed).clamp(-1.0, 1.0);
        let angle = cos_off.acos(); // 0 = head wind, π = following
        // Lift-like curve peaking at beam reach (90°), scaled by wind strength.
        let shape = (angle.sin()).powi(2); // 0 at 0/π, 1 at π/2
        let strength = (aw_speed / 12.0).min(1.0);
        0.35 * shape * strength
    }

    /// Given the autopilot's requested throttle and the ship state, return the
    /// throttle the green powertrain can actually sustain, and book the energy
    /// used over `dt` seconds. `t_max` is the plant's max thrust (N).
    fn gate(&mut self, thr_req: f32, t_max: f32, surge: f32, heading: f32, dt: f32) -> f32 {
        let thr_req = thr_req.clamp(0.0, 1.0);
        let thrust_req = thr_req * t_max;
        let wf = self.wind_thrust_frac(heading, surge);
        let thrust_wind = wf * t_max;

        // Electric thrust the drive must still supply (wind offsets the rest).
        let elec_thrust = (thrust_req - thrust_wind).max(0.0);
        // Bus demand = propulsion (thrust · speed / drive_eff) + constant hotel load (W → kW).
        let p_prop_kw = elec_thrust * surge.max(0.0) / self.drive_eff / 1000.0;
        let p_elec_kw = p_prop_kw + self.hotel_kw;

        let h = dt / 3600.0; // hours

        // Dispatch: solar first, then hydrogen, then battery; surplus charges battery.
        let solar = self.solar_kw.min(p_elec_kw);
        let mut residual = p_elec_kw - solar;
        let h2 = self.h2_max_kw.min(residual.max(0.0));
        residual -= h2;
        // Battery discharges to cover the rest, limited by SOC.
        let batt_avail_kw = (self.batt_soc_kwh / h).max(0.0);
        let batt = batt_avail_kw.min(residual.max(0.0));
        residual -= batt;

        // Solar/wind surplus (when demand is low) trickle-charges the battery.
        let surplus = (self.solar_kw - solar).max(0.0);
        let charge = surplus.min((self.batt_capacity_kwh - self.batt_soc_kwh) / h).max(0.0);
        self.batt_soc_kwh = (self.batt_soc_kwh - batt * h + charge * h)
            .clamp(0.0, self.batt_capacity_kwh);

        // Book energy (kWh). Wind energy = its propulsive contribution.
        let p_wind_kw = thrust_wind * surge.max(0.0) / 1000.0;
        self.e_wind += p_wind_kw * h;
        self.e_solar += solar * h;
        self.e_h2 += h2 * h;
        // Green H2 mass: kWh_elec / (LHV 33.33 kWh/kg · FC eff 0.52).
        self.h2_kg += h2 * h / (33.33 * 0.52);

        if residual > 1e-3 {
            // Green budget exhausted — power-limit the throttle (NEVER a fossil top-up).
            self.power_limited_s += dt;
            let suppliable_kw = solar + h2 + batt;
            let elec_thrust_ok = suppliable_kw * 1000.0 * self.drive_eff / surge.max(0.1);
            ((elec_thrust_ok + thrust_wind) / t_max).clamp(0.0, 1.0)
        } else {
            thr_req
        }
    }

    fn total(&self) -> f32 {
        self.e_wind + self.e_solar + self.e_h2
    }
}

fn main() {
    let dt = 0.5_f32;
    let start = Pose2::new(0.0, 0.0, 0.0);
    let limits = VehicleClass::Ship.limits();
    let t_max = limits.m11_thrust(); // see helper below

    let mut ship = ShipHydro::new(start, limits);
    // Slightly widen the arrival circle (the proven ship test captures at the
    // default ~6 m; 12 m gives clean multi-waypoint hand-off without overshoot).
    let mut cfg = AutopilotConfig::for_class(VehicleClass::Ship);
    cfg.goal_tol = limits.footprint_radius * 2.0;
    let mut ap = Autopilot::new(cfg, start);
    let mut pt = Powertrain::nagi();

    // Coastal course as a chain of short on-grid legs (kami-autodrive navigates
    // at perception-grid scale, ~tens of metres). The heading changes between
    // legs so the apparent-wind angle — and the wind-assist share — varies.
    let waypoints = [
        Vec2::new(80.0, 40.0),
        Vec2::new(170.0, 80.0),
        Vec2::new(270.0, 80.0),
        Vec2::new(360.0, 40.0),
        Vec2::new(450.0, 40.0),
    ];
    let mut leg = 0usize;
    ap.set_goal(waypoints[leg]);

    println!("# funadaiku Nagi 凪 — autonomous zero-emission voyage (demo-scale)");
    println!("# t   x      y      yaw    speed  thr_cmd thr_green  batt%  leg");
    let mut arrived = false;
    for step in 0..6000 {
        let pose = ship.pose();
        if ap.state == DriveState::Arrived {
            leg += 1;
            if leg >= waypoints.len() {
                arrived = true;
                println!("ARRIVED final at step {step}: ({:.0}, {:.0})", pose.x, pose.y);
                break;
            }
            ap.set_goal(waypoints[leg]);
        }
        // No obstacles in open water — empty lidar return set.
        let cmd = ap.step(pose, ship.speed(), &[], pose, dt);
        let thr_green = pt.gate(cmd.throttle, t_max, ship.u, pose.yaw, dt);
        let mut gated = cmd;
        gated.throttle = thr_green;
        ship.step(gated, dt);

        if step % 60 == 0 {
            println!(
                "{:5.0} {:6.0} {:6.0} {:6.2} {:5.2}  {:5.2}   {:5.2}   {:4.0}%  {}",
                step as f32 * dt,
                pose.x,
                pose.y,
                pose.yaw,
                ship.speed(),
                cmd.throttle,
                thr_green,
                pt.batt_soc_kwh / pt.batt_capacity_kwh * 100.0,
                leg,
            );
        }
    }

    let tot = pt.total().max(1e-6);
    println!("\n# Energy split over the voyage (zero-emission, G13):");
    println!("  wind-assist : {:6.1}%  ({:.2} kWh)", pt.e_wind / tot * 100.0, pt.e_wind);
    println!("  solar       : {:6.1}%  ({:.2} kWh)", pt.e_solar / tot * 100.0, pt.e_solar);
    println!("  hydrogen FC : {:6.1}%  ({:.2} kWh)", pt.e_h2 / tot * 100.0, pt.e_h2);
    println!("  fossil      : {:6.1}%  (0.00 kWh) — none (G13/N5)", 0.0);
    println!("  green-H2 consumed : {:.2} kg", pt.h2_kg);
    println!("  battery SOC final : {:.0}%", pt.batt_soc_kwh / pt.batt_capacity_kwh * 100.0);
    println!("  power-limited time: {:.0} s (green-only throttle clamp; never fossil)", pt.power_limited_s);

    // Self-check (this example doubles as a smoke test).
    assert!(arrived, "Nagi failed to reach the final waypoint");
    assert!(pt.e_wind > 0.0, "wind-assist contributed nothing — check apparent-wind model");
    assert!(pt.e_h2 > 0.0, "hydrogen fuel cell never dispatched");
    println!("\nOK: autonomous arrival under a zero-emission powertrain (fossil = 0).");
}

/// Tiny extension trait so the example can recover the plant's max thrust for
/// the powertrain bookkeeping without changing the crate's public surface.
trait MaxThrust {
    fn m11_thrust(&self) -> f32;
}
impl MaxThrust for kami_autodrive::VehicleLimits {
    fn m11_thrust(&self) -> f32 {
        // Mirrors ShipHydro::new: t_max = m11 · max_accel, m11 = 1.1·m, m = 2000 kg.
        let m = 2000.0_f32;
        1.1 * m * self.max_accel
    }
}
