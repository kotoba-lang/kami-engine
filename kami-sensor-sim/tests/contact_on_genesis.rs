//! Cross-crate integration: an `isaacsim.sensors`-shaped ContactSensor on an
//! `isaacsim.core.api` articulation link, detecting overlap with an obstacle.
//!
//! Data path: `kami_genesis::ArticulationView::get_world_pose` (link sphere
//! centre) → `kami_sensor_sim::ContactSensor::sample`. The cart is teleported
//! along +x (`set_joint_positions`) toward a fixed obstacle so the transition
//! into contact and the growing penetration depth are deterministic.

use glam::Vec3;
use kami_articulated::parse_urdf;
use kami_genesis::{ArticulationHandle, IsaacWorld};
use kami_sensor_sim::{ContactSensor, Primitive, Scene};

const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");

/// Teleport the cart to `x` and sample the contact sensor at its world pose.
fn contact_at(
    world: &mut IsaacWorld,
    h: ArticulationHandle,
    sensor: &ContactSensor,
    scene: &Scene,
    x: f32,
) -> kami_sensor_sim::ContactReading {
    world
        .articulation_mut(h)
        .unwrap()
        .set_joint_positions(&[x, 0.0]);
    let (p, _q) = world
        .articulation(h)
        .unwrap()
        .get_world_pose("cart")
        .unwrap();
    sensor.sample(Vec3::from(p), scene, 0.0)
}

#[test]
fn cart_contact_sensor_detects_obstacle_and_penetration_grows() {
    let mut world = IsaacWorld::new(1.0 / 240.0);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    world.reset();

    // Obstacle sphere centred at x=1.0; sensor collision sphere radius 0.3.
    // Surfaces meet when centre distance < 0.3 + 0.3 = 0.6, i.e. cart_x > 0.4.
    let mut scene = Scene::new();
    scene.add(Primitive::Sphere {
        center: Vec3::new(1.0, 0.0, 0.0),
        radius: 0.3,
    });
    let sensor = ContactSensor::new("contact", "/World/cart/contact", "cart", 0.3);

    // Far away → no contact, positive clearance.
    let far = contact_at(&mut world, h, &sensor, &scene, 0.0);
    assert!(!far.in_contact, "should not be in contact at x=0");
    assert!(
        (far.closest_distance - 0.4).abs() < 1e-3,
        "clearance {} != 0.4",
        far.closest_distance
    );
    assert_eq!(far.penetration_depth, 0.0);

    // Just past the touch threshold → in contact, small penetration.
    let touch = contact_at(&mut world, h, &sensor, &scene, 0.5);
    assert!(touch.in_contact, "should be in contact at x=0.5");
    assert!(
        (touch.penetration_depth - 0.1).abs() < 1e-3,
        "penetration {} != 0.1",
        touch.penetration_depth
    );
    assert_eq!(touch.closest_primitive, 0);

    // Deeper → penetration grows.
    let deep = contact_at(&mut world, h, &sensor, &scene, 0.7);
    assert!(deep.in_contact);
    assert!(
        deep.penetration_depth > touch.penetration_depth,
        "penetration did not grow: {} -> {}",
        touch.penetration_depth,
        deep.penetration_depth
    );
    assert!(
        (deep.penetration_depth - 0.3).abs() < 1e-3,
        "penetration {} != 0.3",
        deep.penetration_depth
    );
    // Contact normal is finite and unit-ish.
    assert!(
        (deep.contact_normal.length() - 1.0).abs() < 1e-3,
        "normal not unit: {:?}",
        deep.contact_normal
    );
}

#[test]
fn empty_scene_never_reports_contact() {
    let mut world = IsaacWorld::new(1.0 / 240.0);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    world.reset();
    let scene = Scene::new();
    let sensor = ContactSensor::new("contact", "/World/cart/contact", "cart", 0.5);
    let r = contact_at(&mut world, h, &sensor, &scene, 0.0);
    assert!(!r.in_contact && r.closest_primitive == usize::MAX);
}
