//! Cross-crate integration: an `isaacsim.sensors`-shaped IMU mounted on an
//! `isaacsim.core.api` articulation, validating the documented data path
//!   `kami_genesis::ArticulationView::{get_world_pose, get_world_velocity}`
//!     → `kami_sensor_sim::Imu::sample`.
//!
//! Proves the two Isaac-compat crates compose into a sensor-on-robot rig with
//! physically correct readings — no NVIDIA code, all KAMI-native solver.

use kami_articulated::parse_urdf;
use kami_genesis::IsaacWorld;
use kami_sensor_sim::Imu;

const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");

/// Pull a link's world-frame pose+velocity off the Isaac view and feed the IMU.
fn sample_imu(
    imu: &mut Imu,
    world: &IsaacWorld,
    h: kami_genesis::ArticulationHandle,
    link: &str,
    time: f32,
) -> kami_sensor_sim::ImuReading {
    let view = world.articulation(h).unwrap();
    let (_p, q) = view.get_world_pose(link).expect("pose");
    let (v, w) = view.get_world_velocity(link).expect("vel");
    imu.sample(
        glam::Vec3::from(v),
        glam::Vec3::from(w),
        glam::Quat::from_xyzw(q[1], q[2], q[3], q[0]), // (w,x,y,z) → xyzw
        time,
    )
}

#[test]
fn cart_imu_reads_plus_g_at_rest_and_proper_accel_under_force() {
    let dt = 1.0 / 240.0;
    let mut world = IsaacWorld::new(dt);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    let mut imu = Imu::new("imu", "/World/cart/imu", "cart");
    world.reset();
    imu.reset();

    // At rest: a real accelerometer reads +g along world/body up (z); the cart
    // does not rotate, so body frame == world frame.
    let r0 = sample_imu(&mut imu, &world, h, "cart", world.current_time());
    assert!(
        (r0.linear_acceleration.z - 9.81).abs() < 1e-2,
        "rest +g: {:?}",
        r0.linear_acceleration
    );
    assert!(
        r0.linear_acceleration.x.abs() < 1e-2,
        "rest x≈0: {:?}",
        r0.linear_acceleration
    );

    // Push the cart along +x; the proper acceleration must show a +x component
    // (inertial accel from the drive force) on top of the +g up reading.
    let mut saw_positive_ax = false;
    for _ in 0..15 {
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_efforts(&[25.0, 0.0]);
        world.step();
        let r = sample_imu(&mut imu, &world, h, "cart", world.current_time());
        assert!(r.linear_acceleration.is_finite());
        if r.linear_acceleration.x > 0.5 {
            saw_positive_ax = true;
        }
        // The up-reading persists throughout (no vertical motion).
        assert!(
            (r.linear_acceleration.z - 9.81).abs() < 1e-1,
            "z stays +g: {:?}",
            r.linear_acceleration
        );
    }
    assert!(saw_positive_ax, "IMU never saw the +x drive acceleration");
}

#[test]
fn pole_imu_reports_nonzero_angular_velocity_while_swinging() {
    // Release the pole from a tilted cart-pole and let gravity swing it; the
    // pole link's IMU must report a non-zero body angular velocity.
    let dt = 1.0 / 240.0;
    let mut world = IsaacWorld::new(dt);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    let mut imu = Imu::new("imu", "/World/pole_link/imu", "pole_link");
    world.reset();
    imu.reset();
    // Tilt the pole off-vertical so gravity produces a torque (DOF order
    // [cart_slider, pole_revolute]).
    world
        .articulation_mut(h)
        .unwrap()
        .set_joint_positions(&[0.0, 0.3]);

    let mut max_wy = 0.0_f32;
    for _ in 0..120 {
        world.step(); // no effort: free swing under gravity
        let r = sample_imu(&mut imu, &world, h, "pole_link", world.current_time());
        assert!(r.angular_velocity.is_finite());
        max_wy = max_wy.max(r.angular_velocity.y.abs());
    }
    assert!(
        max_wy > 0.1,
        "swinging pole IMU saw little angular velocity: {max_wy}"
    );
}
