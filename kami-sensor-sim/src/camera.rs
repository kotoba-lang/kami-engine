//! Pinhole camera — point projection + depth image rasterization.
//!
//! Conventions mirror Isaac Sim / ROS / OpenCV:
//!   - World frame: right-handed, gravity along -z.
//!   - Camera frame: +x right, +y down, +z forward (OpenCV / Isaac Sim).
//!   - Extrinsics: world→camera transform stored as `glam::Affine3A`.
//!   - Intrinsics: pinhole `(fx, fy, cx, cy)` + image size + (near, far) clip.
//!
//! Output coordinates are integer pixel `(u, v)` with `u = column` (left→right),
//! `v = row` (top→bottom), matching standard image-buffer indexing.

use glam::{Affine3A, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraIntrinsics {
    pub fx: f32,
    pub fy: f32,
    pub cx: f32,
    pub cy: f32,
    pub width: u32,
    pub height: u32,
    pub near: f32,
    pub far: f32,
}

impl CameraIntrinsics {
    /// Build intrinsics from a horizontal field of view (radians).
    /// Square pixels assumed; cx/cy are taken at the image centre.
    pub fn from_hfov(width: u32, height: u32, hfov_rad: f32) -> Self {
        let fx = (width as f32 / 2.0) / (hfov_rad / 2.0).tan();
        // Square pixels: fy = fx
        CameraIntrinsics {
            fx,
            fy: fx,
            cx: width as f32 / 2.0,
            cy: height as f32 / 2.0,
            width,
            height,
            near: 0.05,
            far: 1000.0,
        }
    }
}

/// One projected point: integer pixel coords + camera-space depth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    pub u: u32,
    pub v: u32,
    pub depth: f32,
}

/// Width × height float depth buffer in row-major order.
/// `+∞` = empty pixel. Origin (0, 0) is the top-left.
#[derive(Debug, Clone)]
pub struct DepthImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<f32>,
}

impl DepthImage {
    pub fn empty(width: u32, height: u32) -> Self {
        DepthImage {
            width,
            height,
            pixels: vec![f32::INFINITY; (width * height) as usize],
        }
    }

    pub fn at(&self, u: u32, v: u32) -> Option<f32> {
        if u >= self.width || v >= self.height {
            return None;
        }
        Some(self.pixels[(v * self.width + u) as usize])
    }

    /// Number of non-infinite pixels.
    pub fn populated_count(&self) -> usize {
        self.pixels.iter().filter(|d| d.is_finite()).count()
    }
}

/// Mirror of `isaacsim.sensors.Camera` (subset).
///
/// Holds intrinsics + extrinsics (world→camera). At R1.1 the renderer is a
/// software splatter that projects discrete 3D points to a depth buffer
/// (no shading, no materials). R1.2 swaps in kami-render WGSL raster +
/// optionally kami-rt for hardware ray-tracing.
#[derive(Debug, Clone)]
pub struct Camera {
    pub name: String,
    pub prim_path: String,
    pub intrinsics: CameraIntrinsics,
    /// world → camera homogeneous transform.
    pub view: Affine3A,
}

impl Camera {
    pub fn new(name: impl Into<String>, prim_path: impl Into<String>, intr: CameraIntrinsics) -> Self {
        Camera {
            name: name.into(),
            prim_path: prim_path.into(),
            intrinsics: intr,
            view: Affine3A::IDENTITY,
        }
    }

    /// Set the world→camera transform directly.
    pub fn set_view(&mut self, view: Affine3A) {
        self.view = view;
    }

    /// Set the camera pose from an eye position + look-at target + world-up.
    /// Mirrors `isaacsim.core.api.Camera.set_world_pose(...)` + look-at helpers.
    pub fn look_at(&mut self, eye: Vec3, target: Vec3, up: Vec3) {
        // Camera forward (+z in OpenCV convention) points FROM camera TOWARD target.
        let forward = (target - eye).normalize();
        let right = forward.cross(up).normalize();
        let new_up = right.cross(forward); // already orthonormal
        // OpenCV-style: x_cam = +right, y_cam = -world_up (image y is down),
        // z_cam = +forward. Express world→camera as the rotation that maps
        // those camera axes onto canonical basis vectors.
        let mat = glam::Mat3::from_cols(right, -new_up, forward).transpose();
        let rot = Affine3A::from_mat3(mat);
        let t = -mat * eye;
        self.view = Affine3A::from_translation(t) * rot * Affine3A::IDENTITY;
        // Equivalent compact form: world→camera = R * world_translation_to_origin.
        self.view = Affine3A::from_mat3_translation(mat, t);
    }

    /// Project a single world-space point. Returns `None` if the point is
    /// behind the camera, outside the [near, far] clip range, or outside the
    /// image rectangle.
    pub fn project_world_point(&self, p_world: Vec3) -> Option<Projection> {
        let p_cam = self.view.transform_point3(p_world);
        let depth = p_cam.z;
        if depth <= self.intrinsics.near || depth >= self.intrinsics.far {
            return None;
        }
        let u_f = self.intrinsics.fx * (p_cam.x / depth) + self.intrinsics.cx;
        let v_f = self.intrinsics.fy * (p_cam.y / depth) + self.intrinsics.cy;
        if !u_f.is_finite() || !v_f.is_finite() {
            return None;
        }
        if u_f < 0.0 || v_f < 0.0 {
            return None;
        }
        let u = u_f.floor() as i64;
        let v = v_f.floor() as i64;
        if u >= self.intrinsics.width as i64 || v >= self.intrinsics.height as i64 {
            return None;
        }
        Some(Projection { u: u as u32, v: v as u32, depth })
    }

    /// Rasterize a point cloud into a depth image (z-buffer, nearest wins).
    pub fn render_points_to_depth_image(&self, points: &[Vec3]) -> DepthImage {
        let mut img = DepthImage::empty(self.intrinsics.width, self.intrinsics.height);
        for p in points {
            if let Some(proj) = self.project_world_point(*p) {
                let idx = (proj.v * self.intrinsics.width + proj.u) as usize;
                if proj.depth < img.pixels[idx] {
                    img.pixels[idx] = proj.depth;
                }
            }
        }
        img
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn cam_at_origin_facing_plus_z() -> Camera {
        let intr = CameraIntrinsics::from_hfov(640, 480, 90f32.to_radians());
        let mut c = Camera::new("test_cam", "/World/test_cam", intr);
        c.set_view(Affine3A::IDENTITY); // world == camera
        c
    }

    #[test]
    fn hfov_intrinsics_match_isaac_sim_defaults() {
        // For 640x480 and hfov=90°, fx = 320 / tan(45°) = 320.
        let intr = CameraIntrinsics::from_hfov(640, 480, 90f32.to_radians());
        assert!((intr.fx - 320.0).abs() < 1e-3);
        assert!((intr.fy - 320.0).abs() < 1e-3);
        assert!((intr.cx - 320.0).abs() < 1e-3);
        assert!((intr.cy - 240.0).abs() < 1e-3);
    }

    #[test]
    fn point_on_optical_axis_projects_to_image_centre() {
        let cam = cam_at_origin_facing_plus_z();
        let proj = cam.project_world_point(Vec3::new(0.0, 0.0, 5.0)).unwrap();
        assert_eq!(proj.u, 320);
        assert_eq!(proj.v, 240);
        assert!((proj.depth - 5.0).abs() < 1e-5);
    }

    #[test]
    fn point_behind_camera_returns_none() {
        let cam = cam_at_origin_facing_plus_z();
        assert!(cam.project_world_point(Vec3::new(0.0, 0.0, -1.0)).is_none());
    }

    #[test]
    fn point_below_near_clip_returns_none() {
        let cam = cam_at_origin_facing_plus_z();
        // near = 0.05 by default
        assert!(cam.project_world_point(Vec3::new(0.0, 0.0, 0.04)).is_none());
    }

    #[test]
    fn point_beyond_far_clip_returns_none() {
        let cam = cam_at_origin_facing_plus_z();
        // far = 1000.0 by default
        assert!(cam.project_world_point(Vec3::new(0.0, 0.0, 1001.0)).is_none());
    }

    #[test]
    fn off_axis_point_projects_to_correct_pixel() {
        // At z=5, x=1 in world → camera-space x=1, depth=5.
        // u = fx * (1/5) + cx = 320 * 0.2 + 320 = 384.
        let cam = cam_at_origin_facing_plus_z();
        let proj = cam.project_world_point(Vec3::new(1.0, 0.0, 5.0)).unwrap();
        assert_eq!(proj.u, 384);
        assert_eq!(proj.v, 240);
    }

    #[test]
    fn render_points_to_depth_image_keeps_nearest() {
        let cam = cam_at_origin_facing_plus_z();
        let points = vec![
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 2.0), // closer at same pixel
            Vec3::new(1.0, 0.0, 5.0),
        ];
        let img = cam.render_points_to_depth_image(&points);
        // Center pixel keeps the nearer point (2.0 m, not 5.0 m)
        assert!((img.at(320, 240).unwrap() - 2.0).abs() < 1e-5);
        // Off-axis pixel got the other point at 5 m
        assert!((img.at(384, 240).unwrap() - 5.0).abs() < 1e-5);
        // Empty pixel still infinity
        assert!(img.at(0, 0).unwrap().is_infinite());
        assert_eq!(img.populated_count(), 2);
    }

    #[test]
    fn look_at_origin_projects_target_to_centre() {
        let intr = CameraIntrinsics::from_hfov(640, 480, 90f32.to_radians());
        let mut cam = Camera::new("c", "/c", intr);
        cam.look_at(
            Vec3::new(0.0, 0.0, -5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        // Eye 5 m back along -z looking toward origin: target should be the
        // image centre.
        let proj = cam.project_world_point(Vec3::ZERO).unwrap();
        // Tolerate ±2 pixels for numerical noise in look_at.
        assert!(proj.u >= 318 && proj.u <= 322, "u={}", proj.u);
        assert!(proj.v >= 238 && proj.v <= 242, "v={}", proj.v);
        assert!((proj.depth - 5.0).abs() < 1e-4);
    }

    #[test]
    fn off_screen_point_returns_none() {
        let cam = cam_at_origin_facing_plus_z();
        // 100 m off to the right at z=1: u = fx*(100/1) + cx = 32000+320 >> 640
        assert!(cam.project_world_point(Vec3::new(100.0, 0.0, 1.0)).is_none());
    }
}
