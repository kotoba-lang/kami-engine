//! kami-vehicle — BeamNG-grade soft-body vehicle physics.
//!
//! ```
//! use kami_vehicle::{models::sedan::{sedan, SedanSpec}, ground::FlatGround};
//!
//! let mut car = sedan(&SedanSpec::default());
//! let road = FlatGround::new(0.0);
//! car.controls.throttle = 1.0;
//! car.powertrain.gearbox.shift_to(1);
//! for _ in 0..120 {
//!     car.step(1.0 / 60.0, &road);
//! }
//! println!("speed: {:.1} m/s, rpm: {:.0}", car.speed(), car.engine_rpm());
//! ```
//!
//! Granularity (per car):
//!   * **~80 mass nodes** (chassis floor / belt-line / roof / cargo / wheel
//!     hubs / tire rings),
//!   * **~250 beams** (chassis frame + crush zones + suspension + tire
//!     side-walls + tire tread),
//!   * **4 wheels** with Pacejka 1996 magic-formula tire model,
//!   * full powertrain — engine torque curve / clutch / 6-speed gearbox /
//!     differential (open / locked / LSD) / FWD-RWD-AWD driveline,
//!   * **2 kHz** internal substepping (semi-implicit Euler) regardless of
//!     render rate,
//!   * plastic deformation on every beam (yield + work hardening), grouped
//!     break-zones,
//!   * JBeam-subset JSON loader for swapping cars at runtime.
//!
//! See `ARCHITECTURE.md` and `README.md` for module-level details.

pub mod beam;
pub mod builder;
pub mod controls;
pub mod ground;
pub mod implicit;
pub mod integrator;
pub mod jbeam;
pub mod models;
pub mod node;
pub mod powertrain;
pub mod rigid_chassis;
pub mod triangle;
pub mod vehicle;
pub mod wheel;

pub use beam::{Beam, BeamId, BeamType, BreakGroup, DeformParams};
pub use builder::VehicleBuilder;
pub use controls::Controls;
pub use ground::{
    ClosureGround, FlatGround, Ground, GroundSample, MapGround, SurfaceKind, SurfaceZone,
};
pub use integrator::IntegratorConfig;
pub use jbeam::{JBeamError, JBeamFile, load_str};
pub use models::garage::{VehicleKind, build as build_vehicle};
pub use node::{Node, NodeGroup, NodeId};
pub use powertrain::{
    Clutch, Differential, DifferentialKind, DrivelineLayout, Engine, Gearbox, Powertrain,
    TorqueCurve,
};
pub use triangle::{Triangle, TriangleGroup, TriangleId};
pub use vehicle::{IntegratorMode, Vehicle};
pub use wheel::{
    ContactForces, ContactInputs, PacejkaParams, Wheel, WheelId, cornering_stiffness, pacejka_force,
};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::models::sedan::{SedanSpec, sedan};

    #[test]
    fn sedan_settles_with_no_input() {
        let mut v = sedan(&SedanSpec::default());
        let g = FlatGround::new(0.0);
        for _ in 0..240 {
            v.step(1.0 / 60.0, &g);
        }
        // After 4 seconds, the body should still have its structure
        // (no NaN, finite COM, no infinite growth).
        let com = v.center_of_mass();
        assert!(com.is_finite());
        assert!(v.body_velocity().y.abs() < 10.0);
    }

    #[test]
    fn awd_distributes_torque_to_all_four_wheels() {
        let mut v = sedan(&SedanSpec {
            layout: DrivelineLayout::Awd { front_split: 0.5 },
            ..Default::default()
        });
        v.controls.throttle = 1.0;
        v.powertrain.gearbox.current_gear = 1;
        v.powertrain.gearbox.shift_progress = 1.0;
        let g = FlatGround::new(0.0);
        v.step(1.0 / 60.0, &g);
        // After 1 frame all 4 wheels should have non-zero drive torque.
        for w in &v.wheels {
            assert!(
                w.drive_torque.abs() > 1.0,
                "wheel {} drive torque too low",
                w.id
            );
        }
    }

    #[test]
    fn locked_gearbox_yields_zero_drive_torque() {
        let mut v = sedan(&SedanSpec::default());
        v.controls.throttle = 1.0;
        v.powertrain.gearbox.shift_to(0); // neutral
        let g = FlatGround::new(0.0);
        v.step(1.0 / 60.0, &g);
        for w in &v.wheels {
            assert_eq!(w.drive_torque, 0.0);
        }
    }
}
