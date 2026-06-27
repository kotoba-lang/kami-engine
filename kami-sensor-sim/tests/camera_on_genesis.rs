//! Cross-crate perception integration: an `isaacsim.sensors`-shaped pinhole
//! Camera observing an `isaacsim.core.api` articulation.
//!
//! Validates the data path
//!   `kami_genesis::ArticulationView::get_world_pose` (link world point)
//!     → `kami_sensor_sim::Camera::{project_world_point, render_points_to_depth_image}`
//! by driving the cartpole's cart along the camera's optical axis and checking
//! the projected depth tracks the cart's distance while it stays image-centred.

use glam::Vec3;
use kami_articulated::parse_urdf;
use kami_genesis::IsaacWorld;
use kami_sensor_sim::{Camera, CameraIntrinsics};

const CARTPOLE_URDF: &str = include_str!("../../fixtures/cartpole/cartpole.urdf");

#[test]
fn camera_projects_cart_link_with_depth_tracking_its_distance() {
    let mut world = IsaacWorld::new(1.0 / 240.0);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    world.reset();

    // Camera 3 m behind the origin looking along +x (the cart's slide axis).
    let intr = CameraIntrinsics::from_hfov(64, 64, 60f32.to_radians());
    let mut cam = Camera::new("cam", "/World/cam", intr);
    cam.look_at(Vec3::new(-3.0, 0.0, 0.0), Vec3::ZERO, Vec3::Z);

    let cart_point = |w: &IsaacWorld| -> Vec3 {
        let (p, _q) = w.articulation(h).unwrap().get_world_pose("cart").unwrap();
        Vec3::from(p)
    };

    // At rest the cart sits at the origin → on the optical axis, depth ≈ 3.
    let p0 = cam
        .project_world_point(cart_point(&world))
        .expect("cart in view at start");
    assert!(
        (p0.depth - 3.0).abs() < 0.1,
        "start depth {} != ~3",
        p0.depth
    );
    assert!(
        (p0.u as i32 - 32).abs() <= 1 && (p0.v as i32 - 32).abs() <= 1,
        "cart not centred: ({},{})",
        p0.u,
        p0.v
    );

    // Drive the cart along +x (away from the camera); depth must grow.
    for _ in 0..80 {
        world
            .articulation_mut(h)
            .unwrap()
            .set_joint_efforts(&[20.0, 0.0]);
        world.step();
    }
    let cart = cart_point(&world);
    let p1 = cam.project_world_point(cart).expect("cart still in view");
    assert!(
        p1.depth > p0.depth + 0.2,
        "depth did not grow: {} -> {}",
        p0.depth,
        p1.depth
    );
    // Depth equals distance along the optical axis: eye at x=-3 → depth = cart_x + 3.
    assert!(
        (p1.depth - (cart.x + 3.0)).abs() < 0.1,
        "depth {} != geometric {}",
        p1.depth,
        cart.x + 3.0
    );
    // Cart stays on the optical axis → still image-centred.
    assert!(
        (p1.u as i32 - 32).abs() <= 1,
        "cart drifted off-centre: u={}",
        p1.u
    );

    // A depth image of the cart point is populated at the cart's pixel.
    let img = cam.render_points_to_depth_image(&[cart]);
    assert_eq!(
        img.populated_count(),
        1,
        "depth image should hold the one cart point"
    );
    assert!((img.at(p1.u, p1.v).unwrap() - p1.depth).abs() < 1e-4);
}

#[test]
fn point_behind_camera_is_not_projected() {
    let mut world = IsaacWorld::new(1.0 / 240.0);
    let h = world
        .add_articulation(parse_urdf(CARTPOLE_URDF).unwrap())
        .unwrap();
    world.reset();
    // Camera looks along -x; the cart at the origin is behind it → no projection.
    let intr = CameraIntrinsics::from_hfov(64, 64, 60f32.to_radians());
    let mut cam = Camera::new("cam", "/World/cam", intr);
    cam.look_at(
        Vec3::new(-3.0, 0.0, 0.0),
        Vec3::new(-6.0, 0.0, 0.0),
        Vec3::Z,
    );
    let (p, _q) = world
        .articulation(h)
        .unwrap()
        .get_world_pose("cart")
        .unwrap();
    assert!(
        cam.project_world_point(Vec3::from(p)).is_none(),
        "behind-camera point projected"
    );
}
