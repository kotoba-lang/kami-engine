//! Vehicle garage — preset library of buildable vehicles.
//!
//! Each kind picks a tuned `SedanSpec` (the underlying parametric model is
//! shared — we just vary wheelbase, mass, height, drive layout, engine
//! curve, and tire grip).

use crate::powertrain::{DrivelineLayout, TorqueCurve};
use crate::vehicle::Vehicle;
use crate::wheel::PacejkaParams;

use super::sedan::{SedanSpec, sedan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleKind {
    Sedan,
    Hatchback,
    Suv,
    Sports,
    Pickup,
    Bus,
}

impl VehicleKind {
    pub fn id(self) -> &'static str {
        match self {
            VehicleKind::Sedan => "sedan",
            VehicleKind::Hatchback => "hatchback",
            VehicleKind::Suv => "suv",
            VehicleKind::Sports => "sports",
            VehicleKind::Pickup => "pickup",
            VehicleKind::Bus => "bus",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "hatchback" => VehicleKind::Hatchback,
            "suv" => VehicleKind::Suv,
            "sports" => VehicleKind::Sports,
            "pickup" => VehicleKind::Pickup,
            "bus" => VehicleKind::Bus,
            _ => VehicleKind::Sedan,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            VehicleKind::Sedan => "Sedan (4-door, FWD, 2.0L NA)",
            VehicleKind::Hatchback => "Hatchback (compact, FWD, 1.5L)",
            VehicleKind::Suv => "SUV (tall, AWD, turbo 2.0L)",
            VehicleKind::Sports => "Sports (low, RWD, turbo 2.0L)",
            VehicleKind::Pickup => "Pickup (long, RWD, V6)",
            VehicleKind::Bus => "Bus (heavy, RWD, diesel)",
        }
    }

    pub fn spec(self) -> SedanSpec {
        match self {
            VehicleKind::Sedan => SedanSpec::default(),
            VehicleKind::Hatchback => SedanSpec {
                wheelbase: 2.45,
                track_width: 1.50,
                ride_height: 0.50,
                roof_height: 1.05,
                overhang_front: 0.85,
                overhang_rear: 0.55,
                mass_chassis: 700.0,
                mass_engine: 180.0,
                mass_cabin: 420.0,
                wheel_radius: 0.30,
                wheel_width: 0.20,
                layout: DrivelineLayout::Fwd,
                turbo: false,
            },
            VehicleKind::Suv => SedanSpec {
                wheelbase: 2.85,
                track_width: 1.65,
                ride_height: 0.65,
                roof_height: 1.15,
                overhang_front: 1.00,
                overhang_rear: 1.05,
                mass_chassis: 1100.0,
                mass_engine: 300.0,
                mass_cabin: 700.0,
                wheel_radius: 0.36,
                wheel_width: 0.25,
                layout: DrivelineLayout::Awd { front_split: 0.45 },
                turbo: true,
            },
            VehicleKind::Sports => SedanSpec {
                wheelbase: 2.55,
                track_width: 1.62,
                ride_height: 0.42,
                roof_height: 0.85,
                overhang_front: 0.85,
                overhang_rear: 0.85,
                mass_chassis: 720.0,
                mass_engine: 240.0,
                mass_cabin: 380.0,
                wheel_radius: 0.34,
                wheel_width: 0.26,
                layout: DrivelineLayout::Rwd,
                turbo: true,
            },
            VehicleKind::Pickup => SedanSpec {
                wheelbase: 3.20,
                track_width: 1.70,
                ride_height: 0.60,
                roof_height: 1.20,
                overhang_front: 1.00,
                overhang_rear: 1.30,
                mass_chassis: 1200.0,
                mass_engine: 320.0,
                mass_cabin: 480.0,
                wheel_radius: 0.38,
                wheel_width: 0.27,
                layout: DrivelineLayout::Rwd,
                turbo: false,
            },
            VehicleKind::Bus => SedanSpec {
                wheelbase: 4.50,
                track_width: 1.90,
                ride_height: 0.60,
                roof_height: 2.40, // tall bus body
                overhang_front: 0.80,
                overhang_rear: 1.50,
                mass_chassis: 1900.0,
                mass_engine: 480.0,
                mass_cabin: 1200.0,
                wheel_radius: 0.42,
                wheel_width: 0.30,
                layout: DrivelineLayout::Rwd,
                turbo: false,
            },
        }
    }
}

pub fn build(kind: VehicleKind) -> Vehicle {
    let mut v = sedan(&kind.spec());
    v.name = kind.id().to_string();
    // Rigid-chassis projection (translation-only) prevents the chassis
    // from collapsing under sustained suspension cycles, at the cost
    // of slightly damped pitch / roll motion. Net effect on driving
    // is positive: wheels stay grounded, the car accelerates properly.
    v.enable_rigid_chassis();

    // Per-kind powertrain + tire tuning that overrides the sedan default.
    match kind {
        VehicleKind::Sports => {
            v.powertrain.engine.torque_curve = TorqueCurve::turbo_2_0();
            v.powertrain.engine.max_rpm = 7800.0;
            v.powertrain.gearbox.final_drive = 3.85;
            for w in v.wheels.iter_mut() {
                w.tire = PacejkaParams::road_dry();
                w.tire.d_long = 1.20; // sticky tire
                w.tire.d_lat = 1.20;
            }
        }
        VehicleKind::Suv => {
            v.powertrain.engine.torque_curve = TorqueCurve::turbo_2_0();
            v.powertrain.gearbox.final_drive = 4.50; // more reduction
        }
        VehicleKind::Hatchback => {
            v.powertrain.engine.max_rpm = 6800.0;
            v.powertrain.gearbox.final_drive = 4.30;
        }
        VehicleKind::Pickup => {
            v.powertrain.engine.torque_curve = TorqueCurve {
                points: vec![
                    (800.0, 280.0),
                    (1500.0, 380.0),
                    (2500.0, 480.0),
                    (3500.0, 470.0),
                    (4500.0, 380.0),
                    (5500.0, 250.0),
                    (6000.0, 0.0),
                ],
            };
            v.powertrain.engine.max_rpm = 6000.0;
            v.powertrain.gearbox.final_drive = 4.80;
        }
        VehicleKind::Bus => {
            // Diesel: massive low-RPM torque, low rev limit.
            v.powertrain.engine.torque_curve = TorqueCurve {
                points: vec![
                    (600.0, 600.0),
                    (1200.0, 1100.0),
                    (1800.0, 1200.0),
                    (2400.0, 1100.0),
                    (3000.0, 800.0),
                    (3600.0, 0.0),
                ],
            };
            v.powertrain.engine.max_rpm = 3600.0;
            v.powertrain.gearbox.final_drive = 5.50;
        }
        VehicleKind::Sedan => {}
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::FlatGround;

    #[test]
    fn all_kinds_build_without_panic() {
        for &k in &[
            VehicleKind::Sedan,
            VehicleKind::Hatchback,
            VehicleKind::Suv,
            VehicleKind::Sports,
            VehicleKind::Pickup,
            VehicleKind::Bus,
        ] {
            let v = build(k);
            assert!(v.nodes.len() >= 70);
            assert_eq!(v.wheels.len(), 4);
            assert!(v.total_mass > 800.0);
        }
    }

    #[test]
    fn pickup_is_heavier_than_hatchback() {
        let pickup = build(VehicleKind::Pickup).total_mass;
        let hatch = build(VehicleKind::Hatchback).total_mass;
        assert!(pickup > hatch);
    }

    #[test]
    fn from_id_round_trips() {
        for &k in &[
            VehicleKind::Sedan,
            VehicleKind::Hatchback,
            VehicleKind::Suv,
            VehicleKind::Sports,
            VehicleKind::Pickup,
            VehicleKind::Bus,
        ] {
            assert_eq!(VehicleKind::from_id(k.id()), k);
        }
    }

    #[test]
    fn each_kind_settles_without_breaking_more_than_a_handful_of_beams() {
        let g = FlatGround::new(0.0);
        for &k in &[
            VehicleKind::Sedan,
            VehicleKind::Hatchback,
            VehicleKind::Suv,
            VehicleKind::Sports,
            VehicleKind::Pickup,
        ] {
            let mut v = build(k);
            for _ in 0..240 {
                v.step(1.0 / 60.0, &g);
            }
            let broken = v.beams.iter().filter(|b| b.broken).count();
            assert!(
                broken < 20,
                "{} broke {} beams during settle",
                k.id(),
                broken
            );
            assert!(
                v.center_of_mass().y > 0.0 && v.center_of_mass().y.is_finite(),
                "{} fell through ground or NaN: COM y = {}",
                k.id(),
                v.center_of_mass().y
            );
        }
    }
}
