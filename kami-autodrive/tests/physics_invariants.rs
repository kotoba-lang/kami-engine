//! Physical-invariant checks on the dynamics plants — that they obey the laws
//! they claim, not merely that the autopilot can reach a goal with them.
//! Open-loop (no autopilot): constant command, measure the steady state.

use kami_autodrive::{Command, FixedWing, Multirotor, Plant, Pose2, ShipHydro, VehicleClass};

const G: f32 = 9.81;

fn cmd(throttle: f32, steer: f32) -> Command {
    Command {
        throttle,
        brake: 0.0,
        steer,
        handbrake: 0.0,
        reverse: false,
    }
}

#[test]
fn ship_settles_into_a_steady_turning_circle() {
    let dt = 0.05;
    let mut ship = ShipHydro::new(Pose2::new(0.0, 0.0, 0.0), VehicleClass::Ship.limits());
    let c = cmd(1.0, 0.6); // full ahead, rudder hard over

    // Settle.
    for _ in 0..1200 {
        ship.step(c, dt); // 60 s
    }
    let r1 = ship.r;
    for _ in 0..200 {
        ship.step(c, dt); // +10 s
    }
    let r2 = ship.r;

    assert!(r1.abs() > 1e-3, "ship should be turning (r = {r1:.4})");
    assert!(
        (r1 - r2).abs() < 0.05 * r1.abs().max(1e-3),
        "yaw rate should reach a steady value ({r1:.4} → {r2:.4})"
    );
    // Coriolis coupling: a turn induces outward sway.
    assert!(
        ship.v.abs() > 0.01,
        "steady turn should carry sway (v = {:.3})",
        ship.v
    );
    let radius = ship.u / r2;
    assert!(
        radius.is_finite() && radius.abs() > 5.0,
        "turning radius {radius:.1} m"
    );
}

#[test]
fn fixed_wing_obeys_the_coordinated_turn_rate() {
    let dt = 1.0 / 60.0;
    let limits = VehicleClass::Aircraft.limits();
    let mut plane = FixedWing::new(Pose2::new(0.0, 0.0, 0.0), 500.0, limits);
    let bank_max = limits.max_steer.max(0.6);
    let c = cmd(0.7, 1.0); // partial thrust, full bank

    // Settle airspeed + turn rate.
    for _ in 0..2000 {
        plane.step(c, dt);
    }
    let yaw_a = plane.pose().yaw;
    plane.step(c, dt);
    let yaw_b = plane.pose().yaw;
    let psi_dot = (yaw_b - yaw_a) / dt;
    let v = plane.airspeed;

    assert!(
        !plane.stalled,
        "should be a sustained coordinated turn, not stalled"
    );
    let expected = G * bank_max.tan() / v; // ψ̇ = g·tanφ / V
    assert!(
        (psi_dot - expected).abs() < 0.15 * expected,
        "turn rate {psi_dot:.4} should match g·tanφ/V = {expected:.4} (V={v:.1})"
    );
    assert!(v > plane.stall_speed(), "must stay above stall");
}

#[test]
fn multirotor_coasts_to_rest_under_drag() {
    let dt = 1.0 / 50.0;
    let mut drone = Multirotor::new(Pose2::new(0.0, 0.0, 0.0), VehicleClass::Drone.limits());
    // Build up forward speed.
    for _ in 0..150 {
        drone.step(cmd(1.0, 0.0), dt);
    }
    let v_moving = drone.speed();
    assert!(v_moving > 3.0, "drone should be moving ({v_moving:.1} m/s)");

    // Cut command: aerodynamic drag must bleed the speed toward hover. (Drag
    // is quadratic, so the low-speed tail decays slowly — give it 20 s.)
    for _ in 0..1000 {
        drone.step(Command::coast(), dt);
    }
    let v_rest = drone.speed();
    assert!(
        v_rest < 0.35 * v_moving,
        "drag should decelerate it ({v_moving:.1} → {v_rest:.2})"
    );
}

#[test]
fn ship_reverse_thrust_decelerates_then_backs_up() {
    let dt = 0.05;
    let mut ship = ShipHydro::new(Pose2::new(0.0, 0.0, 0.0), VehicleClass::Ship.limits());
    for _ in 0..400 {
        ship.step(cmd(1.0, 0.0), dt); // get underway
    }
    let u_fwd = ship.u;
    assert!(u_fwd > 1.0, "ship should be making way ({u_fwd:.1} m/s)");

    // Full astern (brake channel = reverse propeller in the surge model).
    let astern = Command {
        throttle: 0.0,
        brake: 1.0,
        steer: 0.0,
        handbrake: 0.0,
        reverse: false,
    };
    for _ in 0..600 {
        ship.step(astern, dt);
    }
    assert!(
        ship.u < u_fwd,
        "astern thrust must slow the ship ({u_fwd:.1} → {:.1})",
        ship.u
    );
}
