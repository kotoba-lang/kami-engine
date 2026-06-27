//! Powertrain — Engine + Clutch + Gearbox + Differential + Driveline.
//!
//! Granularity: each link is a 1-DoF rotational element (a single angular
//! velocity + inertia) connected by torque transfer rules. This is the same
//! granularity as BeamNG's "Generic Vehicle Powertrain v2" and is sufficient
//! to reproduce realistic engine braking, clutch slip, gear-shift shock,
//! limited-slip behaviour, and torque distribution.
//!
//! The driveline maps an output shaft torque to per-wheel `drive_torque`
//! values. Wheels in turn feed back their angular velocity through the
//! differential to define the engine-shaft load.

use serde::{Deserialize, Serialize};

/// Sampled torque curve `RPM -> Nm`. Values between samples are linearly
/// interpolated. Below the first sample we hold the first value, above the
/// last sample we drop to zero (rev-cut).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorqueCurve {
    /// (rpm, torque_nm) pairs, sorted by rpm ascending.
    pub points: Vec<(f32, f32)>,
}

impl TorqueCurve {
    /// 2.0L NA gasoline reference curve — peak ~200 Nm @ 4500 RPM.
    pub fn na_2_0_gasoline() -> Self {
        Self {
            points: vec![
                (800.0, 130.0),
                (1500.0, 160.0),
                (2500.0, 185.0),
                (3500.0, 200.0),
                (4500.0, 200.0),
                (5500.0, 185.0),
                (6500.0, 150.0),
                (7000.0, 0.0),
            ],
        }
    }

    /// Turbo 2.0L reference curve — peak ~380 Nm @ 3000 RPM.
    pub fn turbo_2_0() -> Self {
        Self {
            points: vec![
                (800.0, 150.0),
                (1500.0, 280.0),
                (2500.0, 370.0),
                (3000.0, 380.0),
                (4500.0, 370.0),
                (5500.0, 320.0),
                (6500.0, 220.0),
                (7000.0, 0.0),
            ],
        }
    }

    pub fn lookup(&self, rpm: f32) -> f32 {
        if self.points.is_empty() {
            return 0.0;
        }
        let first = self.points[0];
        let last = *self.points.last().unwrap();
        if rpm <= first.0 {
            return first.1;
        }
        if rpm >= last.0 {
            return last.1;
        }
        for w in self.points.windows(2) {
            let (r0, t0) = w[0];
            let (r1, t1) = w[1];
            if rpm <= r1 {
                let f = (rpm - r0) / (r1 - r0);
                return t0 + (t1 - t0) * f;
            }
        }
        0.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Engine {
    pub torque_curve: TorqueCurve,
    /// Idle target RPM.
    pub idle_rpm: f32,
    /// Hard rev limiter.
    pub max_rpm: f32,
    /// Crankshaft moment of inertia (kg·m²).
    pub inertia: f32,
    /// Friction torque magnitude at peak RPM (engine braking, Nm). Scaled
    /// linearly with RPM internally.
    pub friction: f32,
    /// Current crankshaft angular velocity (rad/s).
    pub omega: f32,
    /// Whether the engine is running.
    pub running: bool,
}

impl Engine {
    pub fn new(curve: TorqueCurve) -> Self {
        Self {
            torque_curve: curve,
            idle_rpm: 850.0,
            max_rpm: 7000.0,
            inertia: 0.18,
            friction: 35.0,
            omega: rpm_to_rad(850.0),
            running: true,
        }
    }

    pub fn rpm(&self) -> f32 {
        rad_to_rpm(self.omega)
    }

    /// Net torque produced by the engine *on its own shaft* (before the
    /// clutch). Throttle is in `[0, 1]`; idle controller adds the rest.
    pub fn net_torque(&self, throttle: f32) -> f32 {
        if !self.running {
            return -self.friction.min(self.omega.abs() * 0.05);
        }
        let rpm = self.rpm();
        let raw = self.torque_curve.lookup(rpm);
        let cut = if rpm >= self.max_rpm { 0.0 } else { 1.0 };
        let combustion = raw * throttle.clamp(0.0, 1.0) * cut;

        // Idle controller — pumps a little air to hold idle RPM when throttle
        // is off and engine is below idle.
        let idle = if throttle < 0.05 && rpm < self.idle_rpm {
            (self.idle_rpm - rpm) / self.idle_rpm * 30.0
        } else {
            0.0
        };

        // Pumping / friction loss, scaled with RPM.
        let friction = self.friction * (rpm / self.max_rpm).clamp(0.05, 1.5);

        combustion + idle - friction
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clutch {
    /// `[0, 1]` — 0 = fully released (open), 1 = fully engaged.
    pub engagement: f32,
    /// Maximum static torque the clutch can transmit at full engagement (Nm).
    pub max_torque: f32,
    /// Current slip rate (rad/s) — telemetry.
    pub slip: f32,
}

impl Clutch {
    pub fn new(max_torque: f32) -> Self {
        Self {
            engagement: 1.0,
            max_torque,
            slip: 0.0,
        }
    }

    /// Compute the torque transmitted from the engine side to the gearbox
    /// input. Positive engine_omega - gearbox_omega means engine drives box.
    pub fn transmit(&mut self, engine_omega: f32, gearbox_input_omega: f32, requested: f32) -> f32 {
        self.slip = engine_omega - gearbox_input_omega;
        let cap = self.max_torque * self.engagement.clamp(0.0, 1.0);
        // Below the static cap we transmit the whole requested torque, above
        // it we slip and transmit at most `cap` in the direction of slip.
        if requested.abs() <= cap {
            requested
        } else {
            cap.copysign(requested)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gearbox {
    /// Gear ratios (output_rpm = input_rpm / ratio). Index 0 = reverse,
    /// index 1 = neutral (ratio = 0), then forward gears.
    pub ratios: Vec<f32>,
    pub current_gear: i32, // negative = reverse, 0 = neutral, 1..N = forward
    pub final_drive: f32,
    /// Driveline rotational inertia downstream of the gearbox (kg·m²).
    pub inertia: f32,
    /// Time constant for shifts (s).
    pub shift_time: f32,
    /// 0..1 — shift in progress (cuts torque transmission).
    pub shift_progress: f32,
}

impl Gearbox {
    /// Sedan-style 6-speed manual.
    pub fn manual_6() -> Self {
        Self {
            ratios: vec![3.50, 0.0, 3.50, 1.95, 1.30, 1.00, 0.80, 0.65],
            current_gear: 0,
            final_drive: 4.10,
            inertia: 0.05,
            shift_time: 0.35,
            shift_progress: 1.0,
        }
    }

    pub fn ratio(&self) -> f32 {
        let g = self.current_gear;
        if g == 0 {
            return 0.0;
        }
        let idx = if g < 0 { 0 } else { (g + 1) as usize };
        if idx >= self.ratios.len() {
            return 0.0;
        }
        let sign = if g < 0 { -1.0 } else { 1.0 };
        sign * self.ratios[idx]
    }

    /// Combined gear * final-drive ratio. Output_omega = engine_omega / total.
    pub fn total_ratio(&self) -> f32 {
        let r = self.ratio();
        if r == 0.0 { 0.0 } else { r * self.final_drive }
    }

    pub fn shift_to(&mut self, gear: i32) {
        if gear == self.current_gear {
            return;
        }
        self.current_gear = gear;
        self.shift_progress = 0.0;
    }

    pub fn tick(&mut self, dt: f32) {
        if self.shift_progress < 1.0 {
            self.shift_progress = (self.shift_progress + dt / self.shift_time).min(1.0);
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DifferentialKind {
    Open,
    /// Locked (welded). Both shafts share the same speed.
    Locked,
    /// Limited-slip differential. `lock_factor` in `[0, 1]` determines how
    /// strongly speed differences are penalised (1 = locked).
    Lsd {
        lock_factor: f32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Differential {
    pub kind: DifferentialKind,
}

impl Differential {
    pub fn open() -> Self {
        Self {
            kind: DifferentialKind::Open,
        }
    }

    pub fn lsd(lock_factor: f32) -> Self {
        Self {
            kind: DifferentialKind::Lsd {
                lock_factor: lock_factor.clamp(0.0, 1.0),
            },
        }
    }

    /// Split the input torque between two output shafts given their angular
    /// velocities. Returns `(t_left, t_right)`.
    pub fn split(&self, total: f32, omega_l: f32, omega_r: f32) -> (f32, f32) {
        match self.kind {
            DifferentialKind::Open => (total * 0.5, total * 0.5),
            DifferentialKind::Locked => {
                // Each shaft gets half torque + a clamping torque proportional
                // to the speed mismatch.
                let mismatch = omega_r - omega_l;
                let clamp = mismatch * 50.0; // stiff coupling
                (total * 0.5 + clamp, total * 0.5 - clamp)
            }
            DifferentialKind::Lsd { lock_factor } => {
                let mismatch = omega_r - omega_l;
                let clamp = mismatch * 30.0 * lock_factor;
                (total * 0.5 + clamp, total * 0.5 - clamp)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DrivelineLayout {
    Fwd,
    Rwd,
    Awd { front_split: f32 }, // 0.0 = full RWD, 1.0 = full FWD
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Powertrain {
    pub engine: Engine,
    pub clutch: Clutch,
    pub gearbox: Gearbox,
    pub front_diff: Differential,
    pub rear_diff: Differential,
    pub layout: DrivelineLayout,
}

impl Powertrain {
    pub fn sedan() -> Self {
        Self {
            engine: Engine::new(TorqueCurve::na_2_0_gasoline()),
            clutch: Clutch::new(420.0),
            gearbox: Gearbox::manual_6(),
            front_diff: Differential::open(),
            rear_diff: Differential::open(),
            layout: DrivelineLayout::Fwd,
        }
    }

    /// Distribute drive torque to a slice of `(omega_left, omega_right)`
    /// front + rear wheel speeds. Returns `(front_l, front_r, rear_l, rear_r)`.
    pub fn distribute(
        &self,
        shaft_torque: f32,
        wheel_omegas: [(f32, f32); 2], // [(front_l, front_r), (rear_l, rear_r)]
    ) -> [(f32, f32); 2] {
        let (front_share, rear_share) = match self.layout {
            DrivelineLayout::Fwd => (1.0, 0.0),
            DrivelineLayout::Rwd => (0.0, 1.0),
            DrivelineLayout::Awd { front_split } => (front_split, 1.0 - front_split),
        };
        let front = self.front_diff.split(
            shaft_torque * front_share,
            wheel_omegas[0].0,
            wheel_omegas[0].1,
        );
        let rear = self.rear_diff.split(
            shaft_torque * rear_share,
            wheel_omegas[1].0,
            wheel_omegas[1].1,
        );
        [front, rear]
    }
}

#[inline]
pub fn rpm_to_rad(rpm: f32) -> f32 {
    rpm * std::f32::consts::TAU / 60.0
}
#[inline]
pub fn rad_to_rpm(omega: f32) -> f32 {
    omega * 60.0 / std::f32::consts::TAU
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torque_curve_interpolates() {
        let c = TorqueCurve::na_2_0_gasoline();
        // Between 3500 (200) and 4500 (200) -> 200.
        assert!((c.lookup(4000.0) - 200.0).abs() < 1.0);
        // Below first sample.
        assert!((c.lookup(500.0) - 130.0).abs() < 1.0);
        // Above last sample (rev cut).
        assert!(c.lookup(8000.0).abs() < 1e-3);
    }

    #[test]
    fn engine_idle_holds_rpm_when_off_throttle() {
        let mut e = Engine::new(TorqueCurve::na_2_0_gasoline());
        e.omega = rpm_to_rad(700.0); // below idle
        let t = e.net_torque(0.0);
        assert!(t > 0.0); // idle controller should produce positive torque
    }

    #[test]
    fn clutch_slips_above_capacity() {
        let mut c = Clutch::new(100.0);
        let t = c.transmit(200.0, 100.0, 500.0);
        assert!((t - 100.0).abs() < 1e-3);
    }

    #[test]
    fn gearbox_neutral_has_zero_total_ratio() {
        let mut g = Gearbox::manual_6();
        g.shift_to(0);
        assert_eq!(g.total_ratio(), 0.0);
    }

    #[test]
    fn gearbox_first_gear_ratio_correct() {
        let mut g = Gearbox::manual_6();
        g.shift_to(1);
        // 3.50 (1st) * 4.10 (final)
        assert!((g.total_ratio() - 14.35).abs() < 1e-3);
    }

    #[test]
    fn gearbox_reverse_ratio_negative() {
        let mut g = Gearbox::manual_6();
        g.shift_to(-1);
        assert!(g.total_ratio() < 0.0);
    }

    #[test]
    fn open_diff_splits_evenly() {
        let d = Differential::open();
        let (l, r) = d.split(100.0, 10.0, 12.0);
        assert!((l - 50.0).abs() < 1e-3);
        assert!((r - 50.0).abs() < 1e-3);
    }

    #[test]
    fn locked_diff_clamps_speed_mismatch() {
        let d = Differential {
            kind: DifferentialKind::Locked,
        };
        let (l, r) = d.split(100.0, 10.0, 12.0);
        // Right wheel is spinning faster -> torque shifts to the left.
        assert!(l > r);
    }

    #[test]
    fn fwd_powertrain_only_drives_front() {
        let p = Powertrain::sedan();
        let out = p.distribute(200.0, [(10.0, 10.0), (12.0, 12.0)]);
        assert!(out[0].0 > 0.0 && out[0].1 > 0.0);
        assert_eq!(out[1].0, 0.0);
        assert_eq!(out[1].1, 0.0);
    }
}
