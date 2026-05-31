//! High-fidelity car plant: drive a `kami_vehicle::Vehicle` (BeamNG-grade
//! soft-body sedan) with the same [`Autopilot`](crate::autopilot::Autopilot)
//! that drives the kinematic plants.
//!
//! Frame mapping. `kami-vehicle` is **y-up**: its horizontal ground plane is
//! `(x, z)` and `y` is height. The autonomy stack is **z-up planar `(x, y)`**.
//! We map `kami-vehicle (x, z) -> autonomy (x, y)` and derive yaw from the
//! refreshed chassis-forward axis. Heading/steer signs follow the chassis
//! forward axis; for straight-line and gentle-curve tracking this is exact,
//! and the adapter documents that aggressive countersteer sign should be
//! validated against the soft-body before fielding.
//!
//! Only compiled with the `soft-body-car` feature.

use glam::Vec2;
use kami_vehicle::{
    ground::FlatGround,
    models::garage::{build, VehicleKind},
    IntegratorMode, Vehicle,
};

use crate::plant::Plant;
use crate::types::{Command, Pose2};

/// A soft-body car wrapped as an autonomy [`Plant`].
pub struct SoftBodyCar {
    pub vehicle: Vehicle,
    ground: FlatGround,
}

impl SoftBodyCar {
    /// Build `kind` at the world origin, pre-warm the suspension, and lock it
    /// into forward gear ready for autonomous control.
    pub fn new(kind: VehicleKind, ground_height: f32) -> Self {
        let mut vehicle = build(kind);
        vehicle.set_integrator_mode(IntegratorMode::Xpbd);
        vehicle.powertrain.gearbox.current_gear = 1;
        vehicle.powertrain.gearbox.shift_progress = 1.0;
        let ground = FlatGround::new(ground_height);
        vehicle.settle(&ground, 0.8);
        vehicle.refresh_chassis_frame();
        Self { vehicle, ground }
    }

    /// Planar forward axis `(x, y)` from the (y-up) chassis forward.
    fn planar_forward(&self) -> Vec2 {
        let f = self.vehicle.chassis_forward;
        let v = Vec2::new(f.x, f.z);
        if v.length_squared() > 1e-6 {
            v.normalize()
        } else {
            Vec2::X
        }
    }
}

impl Plant for SoftBodyCar {
    fn pose(&self) -> Pose2 {
        let com = self.vehicle.center_of_mass();
        let fwd = self.planar_forward();
        Pose2::new(com.x, com.z, fwd.y.atan2(fwd.x))
    }

    fn speed(&self) -> f32 {
        self.vehicle.speed()
    }

    fn step(&mut self, mut cmd: Command, dt: f32) {
        cmd.clamp();
        let c = &mut self.vehicle.controls;
        c.throttle = cmd.throttle;
        c.brake = cmd.brake;
        c.steer = cmd.steer;
        c.handbrake = cmd.handbrake;
        c.requested_gear = 1;
        c.ignition = true;

        self.vehicle.step(dt, &self.ground);
        self.vehicle.refresh_chassis_frame();
    }
}
