//! Quarry walk scene: Player state + input + physics + character mesh builder.
//!
//! Pure Rust logic, no web-sys. Consumed by kami-web entry point which plumbs
//! events and WebGPU commands.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

// ── Character mesh (procedural humanoid from boxes) ──

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CharVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

pub struct CharMesh {
    pub vertices: Vec<CharVertex>,
    pub indices: Vec<u32>,
}

fn push_box(
    verts: &mut Vec<CharVertex>,
    idx: &mut Vec<u32>,
    cx: f32,
    cy: f32,
    cz: f32,
    sx: f32,
    sy: f32,
    sz: f32,
    color: [f32; 3],
) {
    let (hx, hy, hz) = (sx * 0.5, sy * 0.5, sz * 0.5);
    let corners = [
        [cx - hx, cy - hy, cz - hz],
        [cx + hx, cy - hy, cz - hz],
        [cx + hx, cy + hy, cz - hz],
        [cx - hx, cy + hy, cz - hz],
        [cx - hx, cy - hy, cz + hz],
        [cx + hx, cy - hy, cz + hz],
        [cx + hx, cy + hy, cz + hz],
        [cx - hx, cy + hy, cz + hz],
    ];
    let faces: [([usize; 4], [f32; 3]); 6] = [
        ([0, 1, 2, 3], [0.0, 0.0, -1.0]),
        ([5, 4, 7, 6], [0.0, 0.0, 1.0]),
        ([4, 0, 3, 7], [-1.0, 0.0, 0.0]),
        ([1, 5, 6, 2], [1.0, 0.0, 0.0]),
        ([3, 2, 6, 7], [0.0, 1.0, 0.0]),
        ([4, 5, 1, 0], [0.0, -1.0, 0.0]),
    ];
    for (corner_idx, n) in faces {
        let base = verts.len() as u32;
        for i in corner_idx {
            verts.push(CharVertex {
                position: corners[i],
                normal: n,
                color,
            });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Build a procedural humanoid mesh (13 boxes, ~200 verts).
pub fn build_character_mesh() -> CharMesh {
    let mut v = Vec::new();
    let mut i = Vec::new();
    let skin = [0.78, 0.66, 0.55];
    let cloth = [0.42, 0.44, 0.50];
    let pants = [0.28, 0.26, 0.22];
    let boots = [0.15, 0.13, 0.10];
    let pack = [0.38, 0.32, 0.24];
    // Head
    push_box(&mut v, &mut i, 0.0, 1.72, 0.0, 0.22, 0.26, 0.22, skin);
    // Torso
    push_box(&mut v, &mut i, 0.0, 1.30, 0.0, 0.46, 0.56, 0.26, cloth);
    // Backpack
    push_box(&mut v, &mut i, 0.0, 1.30, -0.22, 0.42, 0.55, 0.24, pack);
    // Arms
    push_box(&mut v, &mut i, -0.32, 1.30, 0.0, 0.16, 0.38, 0.16, cloth);
    push_box(&mut v, &mut i, 0.32, 1.30, 0.0, 0.16, 0.38, 0.16, cloth);
    push_box(&mut v, &mut i, -0.32, 0.92, 0.0, 0.14, 0.36, 0.14, skin);
    push_box(&mut v, &mut i, 0.32, 0.92, 0.0, 0.14, 0.36, 0.14, skin);
    // Legs
    push_box(&mut v, &mut i, -0.14, 0.70, 0.0, 0.20, 0.48, 0.22, pants);
    push_box(&mut v, &mut i, 0.14, 0.70, 0.0, 0.20, 0.48, 0.22, pants);
    push_box(&mut v, &mut i, -0.14, 0.26, 0.0, 0.18, 0.40, 0.20, pants);
    push_box(&mut v, &mut i, 0.14, 0.26, 0.0, 0.18, 0.40, 0.20, pants);
    // Boots
    push_box(&mut v, &mut i, -0.14, 0.04, 0.03, 0.20, 0.12, 0.32, boots);
    push_box(&mut v, &mut i, 0.14, 0.04, 0.03, 0.20, 0.12, 0.32, boots);
    CharMesh {
        vertices: v,
        indices: i,
    }
}

// ── Player state ──

pub const EYE_HEIGHT: f32 = 1.72;
pub const GRAVITY: f32 = 15.0;
pub const JUMP_VELOCITY: f32 = 5.5;
pub const WALK_SPEED: f32 = 3.5;
pub const SPRINT_MULT: f32 = 1.8;

#[derive(Debug, Clone)]
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub y_vel: f32,
    pub on_ground: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub facing: f32,
    pub move_speed: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            y_vel: 0.0,
            on_ground: true,
            yaw: 0.0,
            pitch: 0.0,
            facing: 0.0,
            move_speed: 0.0,
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct InputState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub sprint: bool,
    pub jump_pressed: bool,
    pub toggle_fp: bool,
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    pub wheel: f32,
}

/// Mode for third-person / first-person camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraMode {
    ThirdPerson,
    FirstPerson,
}

#[derive(Debug, Clone)]
pub struct CameraState {
    pub mode: CameraMode,
    pub distance: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            mode: CameraMode::ThirdPerson,
            distance: 5.0,
        }
    }
}

/// Sample ground height at (x, z). Supplied by the WASM host (via heightmap cache).
pub type HeightSampler<'a> = &'a dyn Fn(f32, f32) -> f32;

/// Advance player physics + input by `dt` seconds.
pub fn tick_player(
    player: &mut Player,
    input: &mut InputState,
    sample_h: HeightSampler,
    dt: f32,
    world_bound: f32,
) {
    // Look (yaw/pitch from mouse delta)
    player.yaw -= input.mouse_dx * 0.0025;
    player.pitch = (player.pitch - input.mouse_dy * 0.0025).clamp(-1.3, 1.3);
    input.mouse_dx = 0.0;
    input.mouse_dy = 0.0;

    // Movement vector in player's local frame
    let fx = player.yaw.sin();
    let fz = player.yaw.cos();
    let rx = player.yaw.cos();
    let rz = -player.yaw.sin();
    let mut mx = 0.0;
    let mut mz = 0.0;
    if input.forward {
        mx += fx;
        mz += fz;
    }
    if input.back {
        mx -= fx;
        mz -= fz;
    }
    if input.left {
        mx -= rx;
        mz -= rz;
    }
    if input.right {
        mx += rx;
        mz += rz;
    }
    let mag = (mx * mx + mz * mz).sqrt();
    if mag > 0.0 {
        mx /= mag;
        mz /= mag;
    }
    let sprint = if input.sprint { SPRINT_MULT } else { 1.0 };
    let speed = WALK_SPEED * sprint;
    player.x += mx * speed * dt;
    player.z += mz * speed * dt;
    player.move_speed = mag * speed;

    // Character facing follows movement (smoothed)
    if mag > 0.01 {
        let target = mx.atan2(mz);
        let mut diff = target - player.facing;
        while diff > std::f32::consts::PI {
            diff -= std::f32::consts::TAU;
        }
        while diff < -std::f32::consts::PI {
            diff += std::f32::consts::TAU;
        }
        player.facing += diff * (dt * 10.0).min(1.0);
    }

    // World bounds clamp
    player.x = player.x.clamp(-world_bound, world_bound);
    player.z = player.z.clamp(-world_bound, world_bound);

    // Ground collision
    let ground = sample_h(player.x, player.z);
    if !player.on_ground {
        // Semi-implicit Euler: update velocity first, then integrate position
        player.y_vel -= GRAVITY * dt;
        player.y += player.y_vel * dt;
        if player.y <= ground {
            player.y = ground;
            player.y_vel = 0.0;
            player.on_ground = true;
        }
    } else {
        player.y = ground;
        if input.jump_pressed {
            player.y_vel = JUMP_VELOCITY;
            player.on_ground = false;
        }
    }
    input.jump_pressed = false;
}

/// Compute eye + target positions for current camera mode.
pub fn camera_matrices(
    player: &Player,
    cam: &CameraState,
    sample_h: HeightSampler,
) -> (Vec3, Vec3) {
    match cam.mode {
        CameraMode::FirstPerson => {
            let ey = player.y + EYE_HEIGHT;
            let eye = Vec3::new(player.x, ey, player.z);
            let look_x = player.yaw.sin() * player.pitch.cos();
            let look_y = player.pitch.sin();
            let look_z = player.yaw.cos() * player.pitch.cos();
            let target = eye + Vec3::new(look_x, look_y, look_z);
            (eye, target)
        }
        CameraMode::ThirdPerson => {
            let d = cam.distance;
            let up = 2.2;
            let cx = player.x - player.yaw.sin() * d;
            let cz = player.z - player.yaw.cos() * d;
            let mut cy = player.y + EYE_HEIGHT + up + player.pitch.sin() * 3.0;
            // Raycast camera to avoid clipping terrain
            let ground_at_cam = sample_h(cx, cz);
            if cy < ground_at_cam + 1.5 {
                cy = ground_at_cam + 1.5;
            }
            let eye = Vec3::new(cx, cy, cz);
            let target = Vec3::new(player.x, player.y + EYE_HEIGHT * 0.7, player.z);
            (eye, target)
        }
    }
}

/// Build model matrix for character (translate + rotate around Y by facing).
pub fn character_model_matrix(player: &Player) -> Mat4 {
    Mat4::from_translation(Vec3::new(player.x, player.y, player.z))
        * Mat4::from_rotation_y(player.facing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_x: f32, _z: f32) -> f32 {
        0.0
    }

    #[test]
    fn char_mesh_nonzero() {
        let m = build_character_mesh();
        assert!(!m.vertices.is_empty());
        assert!(m.indices.len() % 3 == 0);
    }

    #[test]
    fn player_forward_moves_z() {
        let mut p = Player::default();
        let mut i = InputState::default();
        i.forward = true;
        let flat_fn: &dyn Fn(f32, f32) -> f32 = &flat;
        tick_player(&mut p, &mut i, flat_fn, 1.0, 100.0);
        assert!(
            p.z > 3.0,
            "forward should move +z by ~WALK_SPEED: got {}",
            p.z
        );
    }

    #[test]
    fn gravity_pulls_down() {
        let mut p = Player::default();
        p.on_ground = false;
        p.y = 10.0;
        let mut i = InputState::default();
        let flat_fn: &dyn Fn(f32, f32) -> f32 = &flat;
        tick_player(&mut p, &mut i, flat_fn, 0.5, 100.0);
        assert!(p.y < 10.0);
    }

    #[test]
    fn jump_from_ground() {
        let mut p = Player::default();
        let mut i = InputState::default();
        i.jump_pressed = true;
        let flat_fn: &dyn Fn(f32, f32) -> f32 = &flat;
        tick_player(&mut p, &mut i, flat_fn, 0.016, 100.0);
        assert!(!p.on_ground);
        assert!(p.y_vel > 0.0);
    }
}
