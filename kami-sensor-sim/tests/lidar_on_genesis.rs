//! Cross-crate perception integration: an `isaacsim.sensors`-shaped Lidar
//! mounted on an `isaacsim.core.api` articulation, sensing a fixed obstacle.
//!
//! Validates the data path
//!   `kami_genesis::ArticulationView::get_world_pose` (sensor mount pose)
//!     → `kami_sensor_sim::Lidar::{view, acquire_data}` (range returns)
//! by driving the cartpole's cart toward an obstacle and confirming the
//! measured range shrinks monotonically and matches the geometric distance.

use glam::{Affine3A, Vec3};
use kami_articulated::parse_urdf;
use kami_genesis::{ArticulationHandle, IsaacWorld};
use kami_sensor_sim::{Lidar, LidarIntrinsics, LidarReturn, Primitive, Scene};

const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");

/// Nearest finite range among beams that struck primitive `prim`.
fn min_range_to(returns: &[LidarReturn], prim: usize) -> Option<f32> {
    returns
        .iter()
        .filter(|r| r.prim_index == prim && r.range.is_finite())
        .map(|r| r.range)
        .fold(None, |acc, x| Some(acc.map_or(x, |a: f32| a.min(x))))
}

/// Mount the lidar at the cart's world pose (world-aligned axes, +x forward)
/// and acquire the nearest range to the obstacle (primitive 0).
fn sense_obstacle(
    world: &IsaacWorld,
    h: ArticulationHandle,
    lidar: &mut Lidar,
    scene: &Scene,
) -> Option<f32> {
    let (p, _q) = world
        .articulation(h)
        .unwrap()
        .get_world_pose("cart")
        .unwrap();
    // world → sensor: translate so the sensor origin sits at the cart.
    lidar.view = Affine3A::from_translation(-Vec3::from(p));
    min_range_to(&lidar.acquire_data(scene), 0)
}

#[test]
fn cart_mounted_lidar_range_to_obstacle_shrinks_as_it_approaches() {
    let dt = 1.0 / 240.0;
    let mut world = IsaacWorld::new(dt);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    world.reset();

    // A fixed sphere obstacle 5 m ahead along +x.
    const D: f32 = 5.0;
    const R: f32 = 0.5;
    let mut scene = Scene::new();
    scene.add(Primitive::Sphere {
        center: Vec3::new(D, 0.0, 0.0),
        radius: R,
    });
    let mut lidar = Lidar::new("lidar", "/World/cart/lidar", LidarIntrinsics::vlp16());

    // At rest (cart x≈0) the range ≈ D − R.
    let r0 = sense_obstacle(&world, h, &mut lidar, &scene).expect("obstacle sensed at start");
    assert!((r0 - (D - R)).abs() < 0.2, "start range {r0} != ~{}", D - R);

    // Drive the cart toward the obstacle (+x) and re-sense.
    for _ in 0..80 {
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_efforts(&[20.0, 0.0]);
        world.step();
    }
    let (p, _q) = world
        .articulation(h)
        .unwrap()
        .get_world_pose("cart")
        .unwrap();
    let cart_x = p[0];
    assert!(cart_x > 0.3, "cart should have advanced: {cart_x}");

    let r1 = sense_obstacle(&world, h, &mut lidar, &scene).expect("obstacle still sensed");
    // Range shrank by roughly how far the cart advanced.
    assert!(r1 < r0 - 0.2, "range did not shrink: {r0} -> {r1}");
    assert!(
        (r1 - (D - cart_x - R)).abs() < 0.25,
        "range {r1} != geometric {}",
        D - cart_x - R
    );
}

#[test]
fn empty_scene_yields_no_obstacle_returns() {
    let mut world = IsaacWorld::new(1.0 / 240.0);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    world.reset();
    let scene = Scene::new(); // nothing to hit
    let mut lidar = Lidar::new("lidar", "/World/cart/lidar", LidarIntrinsics::vlp16());
    assert!(
        sense_obstacle(&world, h, &mut lidar, &scene).is_none(),
        "empty scene should miss"
    );
}
