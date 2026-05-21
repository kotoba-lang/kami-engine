//! Camera: perspective projection + orbit controls + orthographic side-scroll/top-down.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// Camera projection mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraMode {
    Perspective,
    OrthographicSide,
    OrthographicTop,
    /// Map view: top-down with optional tilt (pitch 0°=overhead, up to 85°).
    /// Uses perspective when pitch > 0, orthographic when pitch == 0.
    MapView {
        /// Map zoom level (0..22, fractional). Controls ortho extent.
        zoom: f32,
        /// Compass bearing in radians (0 = north up).
        bearing: f32,
        /// Tilt in radians (0 = straight down, max ~1.48 = 85°).
        pitch: f32,
    },
}

impl Default for CameraMode {
    fn default() -> Self {
        CameraMode::Perspective
    }
}

/// Build a column-major orthographic projection matrix (right-handed, depth [0,1]).
pub fn ortho_matrix(width: f32, height: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let hw = width * 0.5;
    let hh = height * 0.5;
    Mat4::orthographic_rh(-hw, hw, -hh, hh, near, far).to_cols_array_2d()
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
    pub position: [f32; 3],
    pub _pad: f32,
}

pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub mode: CameraMode,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            position: Vec3::new(0.0, 10.0, 20.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: 60.0_f32.to_radians(),
            aspect,
            near: 0.5,
            far: 256.0,
            mode: CameraMode::default(),
        }
    }

    pub fn uniform(&self) -> CameraUniform {
        let view = Mat4::look_at_rh(self.position, self.target, self.up);
        let projection = match self.mode {
            CameraMode::Perspective => {
                Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far)
            }
            CameraMode::OrthographicSide => {
                // Width derived from a base ortho height scaled by aspect ratio.
                let ortho_height = 16.0;
                let ortho_width = ortho_height * self.aspect;
                let hw = ortho_width * 0.5;
                let hh = ortho_height * 0.5;
                Mat4::orthographic_rh(-hw, hw, -hh, hh, self.near, self.far)
            }
            CameraMode::OrthographicTop => {
                // Ortho extent derived from camera altitude (position.y)
                let ortho_height = self.position.y.abs().max(1.0);
                let ortho_width = ortho_height * self.aspect;
                let hw = ortho_width * 0.5;
                let hh = ortho_height * 0.5;
                Mat4::orthographic_rh(-hw, hw, -hh, hh, self.near, self.far)
            }
            CameraMode::MapView { zoom, pitch, .. } => {
                // Viewport in world pixels: at integer zoom, 1 screen pixel = 1 world pixel.
                // At fractional zoom, scale by 2^(fract(zoom)).
                let frac_scale = 2.0_f32.powf(zoom - zoom.floor());
                // Visible extent = viewport_px / frac_scale
                // We use a fixed reference: viewport height in world units.
                // viewport_height_world = viewport_px_h / frac_scale.
                // Since we don't store viewport pixel size here, use aspect ratio:
                // half_h = altitude (ortho extent is tied to camera height).
                let altitude = 256.0 * 2.0_f32.powf(16.0 - zoom);
                if pitch < 0.01 {
                    // Pure top-down orthographic: extent = camera altitude
                    let half_h = altitude;
                    let half_w = half_h * self.aspect;
                    Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, self.near, self.far)
                } else {
                    // Tilted: perspective — FOV controls how much we see
                    let fov = 0.6 + pitch * 0.4;
                    Mat4::perspective_rh(fov, self.aspect, self.near, self.far)
                }
            }
        };
        CameraUniform {
            view: view.to_cols_array_2d(),
            projection: projection.to_cols_array_2d(),
            position: self.position.to_array(),
            _pad: 0.0,
        }
    }

    /// Orbit around target.
    pub fn orbit(&mut self, yaw: f32, pitch: f32, distance: f32) {
        let x = distance * pitch.cos() * yaw.sin();
        let y = distance * pitch.sin();
        let z = distance * pitch.cos() * yaw.cos();
        self.position = self.target + Vec3::new(x, y, z);
    }

    /// Set camera position directly.
    pub fn set_position(&mut self, pos: Vec3) {
        self.position = pos;
        self.target = pos + Vec3::new(0.0, 0.0, -1.0);
    }

    /// First-person camera: move by delta and look in yaw/pitch direction.
    pub fn move_fps(&mut self, delta: Vec3, yaw: f32, pitch: f32) {
        self.position += delta;
        let forward = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        self.target = self.position + forward;
    }

    /// Map-view camera: position above center, looking down with optional tilt.
    /// `center_x`, `center_z` are world-pixel offsets from the projection center.
    pub fn map_view_update(
        &mut self,
        center_x: f32,
        center_z: f32,
        zoom: f32,
        bearing: f32,
        pitch: f32,
    ) {
        self.mode = CameraMode::MapView {
            zoom,
            bearing,
            pitch,
        };

        // Camera height derived from zoom: higher zoom = closer
        let altitude = 256.0 * 2.0_f32.powf(16.0 - zoom);

        // Camera sits above center, tilted by pitch, rotated by bearing
        let cos_p = pitch.cos();
        let sin_p = pitch.sin();
        let cos_b = bearing.cos();
        let sin_b = bearing.sin();

        // Offset from target: pitch tilts back, bearing rotates
        let back_dist = altitude * sin_p;
        let up_dist = altitude * cos_p;
        let offset_x = -back_dist * sin_b;
        let offset_z = -back_dist * cos_b;

        self.position = Vec3::new(center_x + offset_x, up_dist, center_z + offset_z);
        self.target = Vec3::new(center_x, 0.0, center_z);
        // When looking straight down, up=(0,1,0) is parallel to look direction
        // → degenerate look_at matrix.  Use -Z (north) as up instead.
        if pitch < 0.01 {
            // Up = north direction rotated by bearing
            self.up = Vec3::new(-sin_b, 0.0, -cos_b);
        } else {
            self.up = Vec3::Y;
        }
    }

    /// Side-scroll camera: follow a player on the XY plane, looking along -Z.
    pub fn side_scroll_update(&mut self, player_x: f32, player_y: f32) {
        self.position = Vec3::new(player_x, player_y + 2.0, 20.0);
        self.target = Vec3::new(player_x, player_y + 2.0, 0.0);
        self.up = Vec3::Y;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LightUniform {
    pub direction: [f32; 3],
    pub _pad0: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    pub view_proj: [[f32; 4]; 4],
}

impl LightUniform {
    pub fn directional(direction: Vec3, color: Vec3, intensity: f32) -> Self {
        let light_pos = -direction.normalize() * 50.0;
        let view = Mat4::look_at_rh(light_pos, Vec3::ZERO, Vec3::Y);
        let projection = Mat4::orthographic_rh(-30.0, 30.0, -30.0, 30.0, 0.1, 100.0);
        Self {
            direction: direction.normalize().to_array(),
            _pad0: 0.0,
            color: color.to_array(),
            intensity,
            view_proj: (projection * view).to_cols_array_2d(),
        }
    }
}

/// PBR material parameters for GPU uniform buffer.
///
/// Extended with subsurface scattering (SSS), anisotropic hair shading,
/// and eye rendering for Final Fantasy-quality character rendering.
/// Layout: 128B total (aligned to 16B boundary).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MaterialUniform {
    // --- Standard PBR (32B) ---
    pub albedo: [f32; 4],    // 16B — base color + alpha
    pub metallic: f32,       // 4B
    pub roughness: f32,      // 4B
    pub has_albedo_tex: u32, // 4B — 0 or 1
    pub has_normal_tex: u32, // 4B — 0 or 1

    // --- Subsurface Scattering for skin (32B) ---
    /// SSS scatter color (RGB) + strength (A). Warm tones for skin (e.g. [0.8, 0.3, 0.2, 0.6]).
    pub subsurface_color: [f32; 4], // 16B
    /// SSS scatter radius per channel (RGB, mm). Controls light diffusion depth.
    pub subsurface_radius: [f32; 3], // 12B
    /// SSS model: 0=disabled, 1=Burley (diffusion profile), 2=random walk.
    pub sss_model: u32, // 4B

    // --- Anisotropic Hair Shading (32B) ---
    /// Hair tangent direction (object space). Marschner model primary specular.
    pub aniso_tangent: [f32; 3], // 12B
    /// Anisotropic specular strength (0=isotropic, 1=full aniso). Kajiya-Kay R lobe.
    pub aniso_strength: f32, // 4B
    /// Hair scatter color (RGB, transmission through thin fibers) + shift (A, specular highlight offset).
    pub hair_scatter: [f32; 4], // 16B

    // --- Eye / Clearcoat / Emission (32B) ---
    /// Clearcoat intensity (eye cornea, lacquer). Disney BRDF clearcoat layer.
    pub clearcoat: f32, // 4B
    /// Clearcoat roughness (0=mirror, 1=diffuse). Eye cornea ≈ 0.02.
    pub clearcoat_roughness: f32, // 4B
    /// Emission color (RGB, HDR). Glow effects, bioluminescence.
    pub emission: [f32; 3], // 12B
    /// Texture flags: bit0=has_mr_tex, bit1=has_sss_tex, bit2=has_emission_tex,
    /// bit3=has_clearcoat_tex, bit4=has_aniso_tex.
    pub tex_flags: u32, // 4B
    /// Parallax depth for eye iris refraction (0=flat, 0.02=subtle, 0.05=deep).
    pub parallax_depth: f32, // 4B  (offset 120)
    pub _pad: f32,           // 4B  (offset 124) — total 128B
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            albedo: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            has_albedo_tex: 0,
            has_normal_tex: 0,
            subsurface_color: [0.0; 4],
            subsurface_radius: [0.0; 3],
            sss_model: 0,
            aniso_tangent: [0.0, 1.0, 0.0],
            aniso_strength: 0.0,
            hair_scatter: [0.0; 4],
            clearcoat: 0.0,
            clearcoat_roughness: 0.5,
            emission: [0.0; 3],
            tex_flags: 0,
            parallax_depth: 0.0,
            _pad: 0.0,
        }
    }
}

impl MaterialUniform {
    /// Skin material preset: Burley SSS diffusion profile for realistic human skin.
    /// `tone` controls skin lightness (0.0=dark, 1.0=very fair).
    pub fn skin(tone: f32) -> Self {
        let base = 0.4 + tone * 0.5;
        Self {
            albedo: [base, base * 0.82, base * 0.72, 1.0],
            metallic: 0.0,
            roughness: 0.35,
            subsurface_color: [0.85, 0.25, 0.15, 0.65],
            subsurface_radius: [1.2, 0.4, 0.2],
            sss_model: 1, // Burley
            ..Default::default()
        }
    }

    /// Hair material preset: anisotropic Marschner specular + fiber scatter.
    /// `hue` is hair color hue (0.0=red, 0.1=blonde, 0.5=brunette, 0.7=black).
    pub fn hair(hue: f32, lightness: f32) -> Self {
        let r = (1.0 - (hue * 6.0 - 0.0).abs().min(1.0)) * lightness;
        let g = (1.0 - (hue * 6.0 - 2.0).abs().min(1.0)) * lightness;
        let b = (1.0 - (hue * 6.0 - 4.0).abs().min(1.0)) * lightness;
        Self {
            albedo: [r.max(0.05), g.max(0.05), b.max(0.05), 1.0],
            metallic: 0.0,
            roughness: 0.28,
            aniso_tangent: [0.0, 1.0, 0.0],
            aniso_strength: 0.85,
            hair_scatter: [r * 0.6, g * 0.6, b * 0.6, 0.15],
            ..Default::default()
        }
    }

    /// Eye material preset: clearcoat cornea + parallax iris refraction.
    /// `iris_color` is RGB iris color.
    pub fn eye(iris_color: [f32; 3]) -> Self {
        Self {
            albedo: [iris_color[0], iris_color[1], iris_color[2], 1.0],
            metallic: 0.0,
            roughness: 0.05,
            clearcoat: 0.95,
            clearcoat_roughness: 0.02,
            parallax_depth: 0.03,
            ..Default::default()
        }
    }

    /// Lip material preset: subtle SSS + glossy clearcoat.
    pub fn lip(color: [f32; 3]) -> Self {
        Self {
            albedo: [color[0], color[1], color[2], 1.0],
            metallic: 0.0,
            roughness: 0.25,
            clearcoat: 0.4,
            clearcoat_roughness: 0.15,
            subsurface_color: [0.9, 0.2, 0.15, 0.3],
            subsurface_radius: [0.5, 0.15, 0.1],
            sss_model: 1,
            ..Default::default()
        }
    }

    /// Fabric material preset: diffuse with subtle roughness variation.
    pub fn fabric(color: [f32; 4], roughness: f32) -> Self {
        Self {
            albedo: color,
            metallic: 0.0,
            roughness,
            ..Default::default()
        }
    }
}
