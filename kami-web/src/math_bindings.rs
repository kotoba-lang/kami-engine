//! WASM-exposed math + heightmap sampling for browser demos.
//!
//! Why here (not a separate `kami-math` crate)?
//! - `glam` is the shared workspace math dep; wrapping it in a new crate adds
//!   Shannon redundancy (η=0) with no new capability.
//! - Same pattern as `VEG_CACHE`: cache large state inside WASM memory,
//!   expose thin functions that return `Vec<f32>` (zero-copy into JS
//!   Float32Array via wasm-bindgen).
//! - Keeps the dependency DAG flat — no new crate, no new workspace member.

use glam::{Mat4, Vec3};
use wasm_bindgen::prelude::*;

// ── Heightmap cache (per-session) ──
thread_local! {
    static HEIGHTMAP: std::cell::RefCell<HeightmapCache> = std::cell::RefCell::new(HeightmapCache::default());
}

#[derive(Default)]
struct HeightmapCache {
    heights: Vec<f32>,
    width: u32,
    depth: u32,
    origin_x: f32,
    origin_z: f32,
}

/// Cache a heightmap for per-frame `sample_terrain_height` calls.
/// Call once after terrain generation. Subsequent sample calls are ~5ns each.
#[wasm_bindgen]
pub fn cache_heightmap(config_json: &str) -> u32 {
    use kami_terrain::{BiomePreset, Heightmap, HeightmapConfig};
    #[derive(serde::Deserialize)]
    struct Cfg {
        width: Option<u32>,
        depth: Option<u32>,
        seed: Option<f32>,
        max_height: Option<f32>,
        frequency: Option<f32>,
        octaves: Option<u32>,
        origin_x: Option<f32>,
        origin_z: Option<f32>,
        biome: Option<String>,
    }
    let cfg: Cfg = serde_json::from_str(config_json).unwrap_or(Cfg {
        width: None,
        depth: None,
        seed: None,
        max_height: None,
        frequency: None,
        octaves: None,
        origin_x: None,
        origin_z: None,
        biome: None,
    });
    let biome = match cfg.biome.as_deref() {
        Some("quarry") => BiomePreset::Quarry,
        Some("desert") => BiomePreset::Desert,
        Some("tundra") => BiomePreset::Tundra,
        _ => BiomePreset::Plains,
    };
    let seed = cfg.seed.unwrap_or(42.0);
    let mut hm_cfg = biome.heightmap(seed);
    if let Some(mh) = cfg.max_height {
        hm_cfg.max_height = mh;
    }
    if let Some(f) = cfg.frequency {
        hm_cfg.frequency = f;
    }
    if let Some(o) = cfg.octaves {
        hm_cfg.octaves = o;
    }
    let w = cfg.width.unwrap_or(257);
    let d = cfg.depth.unwrap_or(257);
    let ox = cfg.origin_x.unwrap_or(-128.0);
    let oz = cfg.origin_z.unwrap_or(-128.0);
    let hm = Heightmap::generate(w, d, ox, oz, &hm_cfg);

    HEIGHTMAP.with(|c| {
        *c.borrow_mut() = HeightmapCache {
            heights: hm.data,
            width: w,
            depth: d,
            origin_x: ox,
            origin_z: oz,
        };
    });
    w * d
}

/// Sample terrain height at world (x, z) with bilinear interpolation.
/// Returns 0 if outside the cached heightmap.
#[wasm_bindgen]
pub fn sample_terrain_height(x: f32, z: f32) -> f32 {
    HEIGHTMAP.with(|c| {
        let c = c.borrow();
        if c.heights.is_empty() {
            return 0.0;
        }
        let fx = x - c.origin_x;
        let fz = z - c.origin_z;
        if fx < 0.0 || fz < 0.0 || fx >= (c.width - 1) as f32 || fz >= (c.depth - 1) as f32 {
            return 0.0;
        }
        let x0 = fx.floor() as u32;
        let z0 = fz.floor() as u32;
        let x1 = x0 + 1;
        let z1 = z0 + 1;
        let tx = fx - x0 as f32;
        let tz = fz - z0 as f32;
        let idx = |cx: u32, cz: u32| c.heights[(cz * c.width + cx) as usize];
        let h00 = idx(x0, z0);
        let h10 = idx(x1, z0);
        let h01 = idx(x0, z1);
        let h11 = idx(x1, z1);
        let ix0 = h00 * (1.0 - tx) + h10 * tx;
        let ix1 = h01 * (1.0 - tx) + h11 * tx;
        ix0 * (1.0 - tz) + ix1 * tz
    })
}

// ── Matrix math (glam-backed, returned as column-major f32×16) ──

fn mat4_to_vec(m: Mat4) -> Vec<f32> {
    m.to_cols_array().to_vec()
}

// `look_at` / `mul_mat4` (archived 2026-04-14): superseded by `view_projection`
// which composes `perspective * look_at_rh` in a single WASM boundary crossing.
// Only the legacy JS-hybrid demos (now in _archive/) invoked them separately.

#[wasm_bindgen]
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Vec<f32> {
    mat4_to_vec(Mat4::perspective_rh(fov_y, aspect, near, far))
}

fn vec_to_mat4(v: &[f32]) -> Mat4 {
    let mut arr = [0f32; 16];
    arr.copy_from_slice(&v[..16]);
    Mat4::from_cols_array(&arr)
}

#[wasm_bindgen]
pub fn invert_mat4(m: &[f32]) -> Vec<f32> {
    mat4_to_vec(vec_to_mat4(m).inverse())
}

/// Build `viewProj = perspective * lookAt` in one call (saves one WASM boundary crossing).
#[wasm_bindgen]
pub fn view_projection(
    eye_x: f32,
    eye_y: f32,
    eye_z: f32,
    target_x: f32,
    target_y: f32,
    target_z: f32,
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> Vec<f32> {
    let view = Mat4::look_at_rh(
        Vec3::new(eye_x, eye_y, eye_z),
        Vec3::new(target_x, target_y, target_z),
        Vec3::Y,
    );
    let proj = Mat4::perspective_rh(fov_y, aspect, near, far);
    mat4_to_vec(proj * view)
}
