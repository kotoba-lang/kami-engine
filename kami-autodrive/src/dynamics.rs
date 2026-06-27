//! High-fidelity dynamics plants for the non-car classes: hydrodynamic ship,
//! aerodynamic fixed-wing, and rotor-thrust multirotor. Each implements
//! [`Plant`], so the **same `Autopilot`** drives them — only the body physics
//! differs.
//!
//! All plants interpret the normalised [`Command`] uniformly:
//!   * `throttle − brake` = longitudinal force/accel demand (propeller / engine
//!     thrust / forward tilt),
//!   * `steer ∈ [−1, 1]` = normalised turn demand (rudder angle / bank angle /
//!     yaw-rate),
//!
//! so pure-pursuit + PID transfer across classes unchanged.
//!
//! Integration is sub-stepped inside each `step()` for numerical stability at
//! the autopilot's coarse outer `dt`.

use crate::classes::VehicleLimits;
use crate::plant::Plant;
use crate::types::{Command, Pose2};

const G: f32 = 9.81;
const SUBSTEPS: u32 = 8;

/// ISA troposphere air density (kg/m³) at geopotential altitude `h` (m).
fn isa_density(h: f32) -> f32 {
    let h = h.clamp(0.0, 11_000.0);
    1.225 * (1.0 - 2.255_77e-5 * h).powf(4.2559)
}

// ───────────────────────────── Ship (hydrodynamic) ─────────────────────────

/// Surface-vessel 3-DOF maneuvering model (Fossen): body-frame surge `u`, sway
/// `v`, yaw-rate `r` with diagonal added mass, linear + quadratic hydrodynamic
/// damping, and the surge↔yaw Coriolis/centripetal coupling that produces the
/// characteristic outward sway in a turn. Propeller drives surge; the rudder
/// moment scales with `u²` (no steerage at zero speed — physically real).
#[derive(Debug, Clone)]
pub struct ShipHydro {
    pub pose: Pose2,
    /// Body-frame velocities: surge, sway, yaw-rate.
    pub u: f32,
    pub v: f32,
    pub r: f32,
    pub limits: VehicleLimits,
    // Inertia (mass + added mass).
    m11: f32,
    m22: f32,
    m33: f32,
    // Damping: linear, quadratic per axis.
    d11: f32,
    d11q: f32,
    d22: f32,
    d22q: f32,
    d33: f32,
    d33q: f32,
    t_max: f32,
    rudder_k: f32,
    rudder_max: f32,
}

impl ShipHydro {
    pub fn new(pose: Pose2, limits: VehicleLimits) -> Self {
        let m = 2000.0; // small launch, kg
        let m11 = 1.1 * m;
        let m22 = 1.6 * m;
        let lpp = 2.0 * limits.footprint_radius;
        let iz = m * lpp * lpp / 12.0;
        let m33 = 1.2 * iz;
        // Thrust sized so steady surge = max_speed under quadratic drag.
        let t_max = m11 * limits.max_accel;
        let d11q = t_max / (limits.max_speed * limits.max_speed);
        Self {
            pose,
            u: 0.0,
            v: 0.0,
            r: 0.0,
            limits,
            m11,
            m22,
            m33,
            d11: 0.05 * t_max / limits.max_speed,
            d11q,
            d22: 0.3 * m22,
            d22q: 300.0,
            d33: 20_000.0,
            d33q: 5_000.0,
            t_max,
            rudder_k: 180.0,
            rudder_max: limits.max_steer,
        }
    }

    pub fn speed(&self) -> f32 {
        self.u.hypot(self.v)
    }
}

impl Plant for ShipHydro {
    fn pose(&self) -> Pose2 {
        self.pose
    }
    fn speed(&self) -> f32 {
        ShipHydro::speed(self)
    }
    fn step(&mut self, mut cmd: Command, dt: f32) {
        cmd.clamp();
        let h = dt / SUBSTEPS as f32;
        let tau_u = (cmd.throttle - cmd.brake) * self.t_max;
        let delta = cmd.steer * self.rudder_max;
        for _ in 0..SUBSTEPS {
            let tau_r = self.rudder_k * delta * self.u * self.u.abs();
            let du = (tau_u - self.d11 * self.u - self.d11q * self.u * self.u.abs()
                + self.m22 * self.v * self.r)
                / self.m11;
            let dv = (-self.d22 * self.v
                - self.d22q * self.v * self.v.abs()
                - self.m11 * self.u * self.r)
                / self.m22;
            let dr = (tau_r - self.d33 * self.r - self.d33q * self.r * self.r.abs()) / self.m33;
            self.u += du * h;
            self.v += dv * h;
            self.r += dr * h;
            self.pose.yaw += self.r * h;
            let (s, c) = self.pose.yaw.sin_cos();
            self.pose.x += (self.u * c - self.v * s) * h;
            self.pose.y += (self.u * s + self.v * c) * h;
        }
    }
}

// ─────────────────────────── Fixed-wing (aerodynamic) ──────────────────────

/// Point-mass fixed-wing aircraft with real aerodynamics: thrust, parasitic +
/// induced drag `C_D = C_D0 + k·C_L²`, lift `L = ½ρV²S·C_L`, ISA air density,
/// `C_Lmax` stall clamp, and bank-to-turn `ψ̇ = g·tanφ / V` from a coordinated
/// banked turn. Holds cruise altitude (γ ≈ 0). Cannot hover — minimum
/// controllable speed is the stall speed, so it overflies rather than stops.
#[derive(Debug, Clone)]
pub struct FixedWing {
    pub pose: Pose2,
    /// True airspeed (m/s).
    pub airspeed: f32,
    /// Cruise altitude (m).
    pub altitude: f32,
    /// True if the demanded turn/level-flight lift exceeds C_Lmax (stall).
    pub stalled: bool,
    pub limits: VehicleLimits,
    mass: f32,
    wing_area: f32,
    cd0: f32,
    k_induced: f32,
    cl_max: f32,
    t_max: f32,
    cd_brake: f32,
    bank_max: f32,
}

impl FixedWing {
    pub fn new(pose: Pose2, altitude: f32, limits: VehicleLimits) -> Self {
        let mass = 1200.0;
        let wing_area = 16.0;
        let t_max = 1.5 * mass * limits.max_accel; // climb/accel authority
        Self {
            pose,
            airspeed: 0.55 * limits.max_speed, // launched at a sane cruise
            altitude,
            stalled: false,
            limits,
            mass,
            wing_area,
            cd0: 0.025,
            k_induced: 0.045,
            cl_max: 1.4,
            t_max,
            cd_brake: 0.08,
            bank_max: limits.max_steer.max(0.6),
        }
    }

    /// Stall speed at the current altitude (m/s).
    pub fn stall_speed(&self) -> f32 {
        let rho = isa_density(self.altitude);
        (2.0 * self.mass * G / (rho * self.wing_area * self.cl_max)).sqrt()
    }
}

impl Plant for FixedWing {
    fn pose(&self) -> Pose2 {
        self.pose
    }
    fn speed(&self) -> f32 {
        self.airspeed
    }
    fn step(&mut self, mut cmd: Command, dt: f32) {
        cmd.clamp();
        let h = dt / SUBSTEPS as f32;
        let rho = isa_density(self.altitude);
        let phi = cmd.steer * self.bank_max;
        let thrust = cmd.throttle * self.t_max;
        for _ in 0..SUBSTEPS {
            let v = self.airspeed.max(1.0);
            let q = 0.5 * rho * v * v * self.wing_area; // dynamic pressure × S
            // Lift needed to hold altitude in a φ-banked turn.
            let cl_need = self.mass * G / (q * phi.cos().max(0.2));
            self.stalled = cl_need > self.cl_max;
            let cl = cl_need.min(self.cl_max);
            let cd = self.cd0 + self.k_induced * cl * cl + cmd.brake * self.cd_brake;
            let drag = q * cd;
            let v_dot = (thrust - drag) / self.mass;
            self.airspeed = (self.airspeed + v_dot * h).max(0.5 * self.stall_speed());
            // Coordinated-turn heading rate (lift-limited if stalled).
            let turn_cl = cl; // available lift caps turn rate
            let load = (turn_cl * q) / (self.mass * G); // load factor n
            let psi_dot = if load > 1.0 {
                G * (load * load - 1.0).sqrt() / self.airspeed * phi.signum()
            } else {
                0.0
            };
            self.pose.yaw += psi_dot * h;
            let (s, c) = self.pose.yaw.sin_cos();
            self.pose.x += self.airspeed * c * h;
            self.pose.y += self.airspeed * s * h;
        }
    }
}

// ───────────────────────────── Multirotor (rotor) ──────────────────────────

/// Quadrotor-class plant: an inner attitude loop tilts the thrust vector to
/// translate, with the total thrust held near `m·g/cosθ` for altitude hold.
/// Horizontal motion comes from `g·tanθ` along the heading; quadratic
/// aerodynamic body drag `½ρ C_d A v²` caps top speed; yaw is a rate command.
/// Unlike the fixed-wing it can decelerate to a hover, so it stops at the goal.
#[derive(Debug, Clone)]
pub struct Multirotor {
    pub pose: Pose2,
    /// World-frame horizontal velocity.
    pub vx: f32,
    pub vy: f32,
    /// Current forward tilt (rad), first-order lagged toward the demand.
    pub tilt: f32,
    pub limits: VehicleLimits,
    drag_c: f32, // ½ρ·C_d·A / m  (1/m)
    a_max: f32,
    tilt_max: f32,
    tilt_tau: f32,
    yawrate_max: f32,
    /// Lateral (sideslip) damping gain (1/s): the quad rolls to cancel crab.
    lat_damp: f32,
}

impl Multirotor {
    pub fn new(pose: Pose2, limits: VehicleLimits) -> Self {
        let mass = 1.5;
        let rho = 1.225;
        let cd = 0.5;
        let area = 0.1;
        Self {
            pose,
            vx: 0.0,
            vy: 0.0,
            tilt: 0.0,
            limits,
            drag_c: 0.5 * rho * cd * area / mass,
            a_max: limits.max_accel,
            tilt_max: 0.5,
            tilt_tau: 0.15,
            yawrate_max: 3.0,
            lat_damp: 2.5,
        }
    }

    pub fn speed(&self) -> f32 {
        self.vx.hypot(self.vy)
    }
}

impl Plant for Multirotor {
    fn pose(&self) -> Pose2 {
        self.pose
    }
    fn speed(&self) -> f32 {
        Multirotor::speed(self)
    }
    fn step(&mut self, mut cmd: Command, dt: f32) {
        cmd.clamp();
        let h = dt / SUBSTEPS as f32;
        let a_dem = (cmd.throttle - cmd.brake) * self.a_max;
        let tilt_des = (a_dem / G).atan().clamp(-self.tilt_max, self.tilt_max);
        let yaw_dot = cmd.steer * self.yawrate_max;
        for _ in 0..SUBSTEPS {
            // First-order attitude response.
            let alpha = (h / self.tilt_tau).min(1.0);
            self.tilt += (tilt_des - self.tilt) * alpha;
            self.pose.yaw += yaw_dot * h;
            // Forward accel from tilted thrust (altitude-hold ⇒ |T|≈mg/cosθ).
            let a_fwd = G * self.tilt.tan();
            let (s, c) = self.pose.yaw.sin_cos();
            let mut ax = a_fwd * c;
            let mut ay = a_fwd * s;
            // Roll to cancel body-lateral (sideslip) velocity — a real quad
            // tilts sideways to stop crabbing, so it tracks heading instead of
            // orbiting.
            let v_lat = -self.vx * s + self.vy * c;
            let a_lat = -self.lat_damp * v_lat;
            ax += a_lat * -s;
            ay += a_lat * c;
            // Quadratic aerodynamic drag opposing velocity.
            let spd = self.vx.hypot(self.vy);
            ax -= self.drag_c * spd * self.vx;
            ay -= self.drag_c * spd * self.vy;
            self.vx += ax * h;
            self.vy += ay * h;
            self.pose.x += self.vx * h;
            self.pose.y += self.vy * h;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::VehicleClass;

    fn full_throttle() -> Command {
        Command {
            throttle: 1.0,
            brake: 0.0,
            steer: 0.0,
            handbrake: 0.0,
            reverse: false,
        }
    }

    #[test]
    fn isa_density_decreases_with_altitude() {
        assert!((isa_density(0.0) - 1.225).abs() < 1e-3);
        assert!(isa_density(2000.0) < isa_density(0.0));
        assert!(isa_density(8000.0) < isa_density(2000.0));
    }

    #[test]
    fn ship_surge_reaches_steady_state_near_max_speed() {
        let limits = VehicleClass::Ship.limits();
        let mut ship = ShipHydro::new(Pose2::new(0.0, 0.0, 0.0), limits);
        for _ in 0..1200 {
            ship.step(full_throttle(), 0.1); // 120 s
        }
        // Quadratic drag balances thrust just below max_speed; no overshoot.
        assert!(
            ship.u > 0.85 * limits.max_speed,
            "surge {} < target",
            ship.u
        );
        assert!(ship.u <= limits.max_speed + 0.1, "overshoot {}", ship.u);
    }

    #[test]
    fn ship_rudder_has_no_authority_at_rest() {
        // Rudder moment ∝ u²: a stationary ship cannot yaw (physically real).
        let mut ship = ShipHydro::new(Pose2::new(0.0, 0.0, 0.0), VehicleClass::Ship.limits());
        let hard_over = Command {
            throttle: 0.0,
            brake: 0.0,
            steer: 1.0,
            handbrake: 0.0,
            reverse: false,
        };
        for _ in 0..50 {
            ship.step(hard_over, 0.1);
        }
        assert!(
            ship.r.abs() < 1e-3 && ship.pose.yaw.abs() < 1e-3,
            "yawed at rest: r={}",
            ship.r
        );
    }

    #[test]
    fn fixed_wing_stall_speed_matches_formula() {
        let plane = FixedWing::new(
            Pose2::new(0.0, 0.0, 0.0),
            0.0,
            VehicleClass::Aircraft.limits(),
        );
        let rho = isa_density(0.0);
        let expected = (2.0 * 1200.0 * G / (rho * 16.0 * 1.4)).sqrt();
        assert!(
            (plane.stall_speed() - expected).abs() < 0.1,
            "{}",
            plane.stall_speed()
        );
    }

    #[test]
    fn fixed_wing_holds_airspeed_above_stall_under_thrust() {
        let mut plane = FixedWing::new(
            Pose2::new(0.0, 0.0, 0.0),
            500.0,
            VehicleClass::Aircraft.limits(),
        );
        let stall = plane.stall_speed();
        for _ in 0..400 {
            plane.step(full_throttle(), 1.0 / 30.0);
        }
        assert!(
            plane.airspeed > stall,
            "airspeed {} dropped below stall {}",
            plane.airspeed,
            stall
        );
    }

    #[test]
    fn multirotor_tilts_forward_and_translates_along_heading() {
        let mut d = Multirotor::new(Pose2::new(0.0, 0.0, 0.0), VehicleClass::Drone.limits());
        for _ in 0..100 {
            d.step(full_throttle(), 1.0 / 50.0); // 2 s
        }
        assert!(d.tilt > 0.05, "should tilt to translate, tilt={}", d.tilt);
        assert!(
            d.pose.x > 1.0 && d.pose.y.abs() < 0.5,
            "moves +x along heading: {:?}",
            d.pose
        );
    }

    #[test]
    fn multirotor_holds_position_under_zero_command() {
        let mut d = Multirotor::new(Pose2::new(4.0, -2.0, 0.3), VehicleClass::Drone.limits());
        for _ in 0..100 {
            d.step(Command::coast(), 1.0 / 50.0);
        }
        assert!(
            d.speed() < 0.2,
            "should not drift when idle, v={}",
            d.speed()
        );
    }
}
