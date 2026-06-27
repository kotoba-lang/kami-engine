//! Per-vehicle-class kinematic limits.
//!
//! FIDELITY MAP. Every class now has a physics plant of its own (see
//! `crate::dynamics` and `crate::vehicle_adapter`); these limits only scale the
//! shared guidance/navigation/control loop:
//!
//! - `Car` — `kami-vehicle` BeamNG soft-body (`soft-body-car`), or `BicycleModel`.
//! - `Ship` — `dynamics::ShipHydro`: Fossen 3-DOF surge/sway/yaw, added mass, quadratic damping, rudder.
//! - `Drone` — `dynamics::Multirotor`: thrust-vector tilt + aero drag + yaw + sideslip damping.
//! - `Aircraft` — `dynamics::FixedWing`: lift/drag, ISA density, stall, bank-to-turn.
//!
//! Honest caveats remain: the non-car plants are reduced-order (3-DOF point /
//! coordinated-turn), not full 6-DOF CFD; the aircraft holds cruise altitude
//! and cannot hover (it overflies a goal rather than stopping).

/// Kinematic envelope used by the planner and controllers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleLimits {
    /// Cruising speed ceiling (m/s).
    pub max_speed: f32,
    /// Forward acceleration ceiling (m/s²).
    pub max_accel: f32,
    /// Braking deceleration ceiling (m/s², positive).
    pub max_decel: f32,
    /// Bicycle wheelbase / effective turning length (m). Larger = wider turns.
    pub wheelbase: f32,
    /// Steering angle ceiling (rad).
    pub max_steer: f32,
    /// Effective minimum turning radius (m) used by the pursuit controller to
    /// scale path curvature into a normalised steer command — decoupled from
    /// `wheelbase` so non-bicycle plants (ship rudder, multirotor yaw, aircraft
    /// bank) steer correctly.
    pub turn_radius_ref: f32,
    /// Collision footprint radius (m) for configuration-space inflation.
    pub footprint_radius: f32,
}

/// The four supported vehicle classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleClass {
    Car,
    Ship,
    Drone,
    Aircraft,
}

impl VehicleClass {
    /// Class-representative kinematic limits.
    pub fn limits(self) -> VehicleLimits {
        match self {
            // Matches a mid-size sedan (kami-vehicle default).
            VehicleClass::Car => VehicleLimits {
                max_speed: 25.0,
                max_accel: 4.0,
                max_decel: 8.0,
                wheelbase: 2.7,
                max_steer: 0.61, // ~35°
                turn_radius_ref: 4.5,
                footprint_radius: 1.3,
            },
            // Small civilian vessel: slow, wide turns, gentle accel.
            VehicleClass::Ship => VehicleLimits {
                max_speed: 8.0,
                max_accel: 0.5,
                max_decel: 1.0,
                wheelbase: 30.0,
                max_steer: 0.52,
                turn_radius_ref: 40.0,
                footprint_radius: 6.0,
            },
            // Ground-projected agile multirotor.
            VehicleClass::Drone => VehicleLimits {
                max_speed: 15.0,
                max_accel: 6.0,
                max_decel: 6.0,
                wheelbase: 0.5,
                max_steer: 1.20,
                turn_radius_ref: 2.5,
                footprint_radius: 0.6,
            },
            // Fixed-wing on taxi / coordinated-turn projection.
            VehicleClass::Aircraft => VehicleLimits {
                max_speed: 60.0,
                max_accel: 3.0,
                max_decel: 4.0,
                wheelbase: 15.0,
                max_steer: 0.35,
                turn_radius_ref: 250.0,
                footprint_radius: 8.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_sane_positive_limits() {
        for c in [
            VehicleClass::Car,
            VehicleClass::Ship,
            VehicleClass::Drone,
            VehicleClass::Aircraft,
        ] {
            let l = c.limits();
            assert!(l.max_speed > 0.0, "{c:?} max_speed");
            assert!(l.max_accel > 0.0 && l.max_decel > 0.0, "{c:?} accel/decel");
            assert!(
                l.wheelbase > 0.0 && l.max_steer > 0.0,
                "{c:?} steering geometry"
            );
            assert!(l.turn_radius_ref > 0.0, "{c:?} turn_radius_ref");
            assert!(l.footprint_radius > 0.0, "{c:?} footprint");
        }
    }
}
