//! Blocky voxel chunk adapter — Minecraft-style flat-shaded cubes with
//! streaming + CPU collision probing.
//!
//! State is kept behind `Rc<VoxelShared>` so the same adapter handle
//! can be passed to:
//!   - `KamiApp::with_pipeline(adapter.clone())` for rendering
//!   - `KamiApp::with_floor_probe(move |p| adapter2.sample_floor(p))`
//!     for Y-axis collision
//!   - game tick hooks for mining (future)
//!
//! `VoxelChunkAdapter` is Clone (shallow `Rc` clone). All mutation
//! goes through `RefCell<...>` interior mutability (single-threaded
//! WASM context).
//!
//! Streaming model matches `TerrainAdapter`: `(2·r+1)³` chunk window
//! around the camera's chunk coord, 1 chunk generated per frame
//! (budget), far chunks unloaded.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::scene_pipelines::{VoxelPipeline, VoxelUniform};
use kami_render::RenderContext;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use wgpu::util::DeviceExt;

use crate::{fog_from_sun, sun_from_time};

pub const CHUNK_SIZE: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct VoxelVertex {
    pos: [f32; 3],
    norm: [f32; 3],
    col: [f32; 3],
}

#[derive(Clone)]
pub struct VoxelChunk {
    data: [u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
    origin: Vec3,
}

impl VoxelChunk {
    pub fn new(origin: Vec3) -> Self {
        Self {
            data: [0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
            origin,
        }
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, material: u8) {
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            self.data[x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE] = material;
        }
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || y < 0 || z < 0
            || x >= CHUNK_SIZE as i32
            || y >= CHUNK_SIZE as i32
            || z >= CHUNK_SIZE as i32
        {
            return 0;
        }
        self.data[x as usize + y as usize * CHUNK_SIZE + z as usize * CHUNK_SIZE * CHUNK_SIZE]
    }

    pub fn origin(&self) -> Vec3 {
        self.origin
    }
}

pub type VoxelPalette = Vec<[f32; 3]>;

struct ChunkGpu {
    vb: wgpu::Buffer,
    vertex_count: u32,
}

/// Per-coord generator. `(cx, cy, cz)` = chunk coord (each unit = 16 m).
/// Return a filled `VoxelChunk` (with `origin` set to
/// `(cx*16, cy*16, cz*16)`), or `None` for empty space.
pub type ChunkGenerator = Box<dyn Fn(i32, i32, i32) -> Option<VoxelChunk>>;

struct VoxelShared {
    pipeline: VoxelPipeline,
    palette: VoxelPalette,
    /// Cloned device/queue so mutation ops (mining, placing) can rebuild
    /// chunk GPU buffers without requiring the caller to thread the
    /// `RenderContext` through.
    device: wgpu::Device,
    /// GPU buffers, keyed by chunk coord.
    gpu: RefCell<HashMap<(i32, i32, i32), ChunkGpu>>,
    /// CPU voxel data (for collision / mining). Kept in sync with `gpu`.
    cpu: RefCell<HashMap<(i32, i32, i32), VoxelChunk>>,
    pending: RefCell<VecDeque<(i32, i32, i32)>>,
    generator: Option<ChunkGenerator>,
    /// View radius in chunks (0 = no streaming, static).
    view_radius: i32,
}

#[derive(Clone)]
pub struct VoxelChunkAdapter {
    inner: Rc<VoxelShared>,
}

impl VoxelChunkAdapter {
    /// Static adapter (manual `insert_chunk` only, no streaming).
    pub fn new(ctx: &RenderContext, palette: VoxelPalette) -> Self {
        Self {
            inner: Rc::new(VoxelShared {
                pipeline: VoxelPipeline::new(&ctx.device, ctx.format),
                palette,
                device: ctx.device.clone(),
                gpu: RefCell::new(HashMap::new()),
                cpu: RefCell::new(HashMap::new()),
                pending: RefCell::new(VecDeque::new()),
                generator: None,
                view_radius: 0,
            }),
        }
    }

    /// Streaming adapter: generator is called once per chunk coord
    /// inside the `(2·view_radius + 1)³` window around the camera.
    pub fn streaming<F>(
        ctx: &RenderContext,
        palette: VoxelPalette,
        view_radius: i32,
        generator: F,
    ) -> Self
    where
        F: Fn(i32, i32, i32) -> Option<VoxelChunk> + 'static,
    {
        Self {
            inner: Rc::new(VoxelShared {
                pipeline: VoxelPipeline::new(&ctx.device, ctx.format),
                palette,
                device: ctx.device.clone(),
                gpu: RefCell::new(HashMap::new()),
                cpu: RefCell::new(HashMap::new()),
                pending: RefCell::new(VecDeque::new()),
                generator: Some(Box::new(generator)),
                view_radius,
            }),
        }
    }

    /// Insert a pre-authored chunk (bypasses streaming). Use for
    /// landmarks / authored structures that shouldn't be
    /// procedurally regenerated.
    pub fn insert_chunk(&self, ctx: &RenderContext, chunk: VoxelChunk) {
        let coord = (
            (chunk.origin.x / CHUNK_SIZE as f32).floor() as i32,
            (chunk.origin.y / CHUNK_SIZE as f32).floor() as i32,
            (chunk.origin.z / CHUNK_SIZE as f32).floor() as i32,
        );
        if let Some(gpu) = build_chunk_gpu(ctx, &chunk, &self.inner.palette) {
            self.inner.gpu.borrow_mut().insert(coord, gpu);
        }
        self.inner.cpu.borrow_mut().insert(coord, chunk);
    }

    /// Is the given world position inside a solid voxel?
    /// Used by `KamiApp::with_floor_probe` for Y collision and by
    /// ray-march mining in tick hooks.
    pub fn is_solid(&self, world: Vec3) -> bool {
        let cx = (world.x / CHUNK_SIZE as f32).floor() as i32;
        let cy = (world.y / CHUNK_SIZE as f32).floor() as i32;
        let cz = (world.z / CHUNK_SIZE as f32).floor() as i32;
        let cpu = self.inner.cpu.borrow();
        let Some(chunk) = cpu.get(&(cx, cy, cz)) else {
            return false;
        };
        let lx = (world.x - chunk.origin.x).floor() as i32;
        let ly = (world.y - chunk.origin.y).floor() as i32;
        let lz = (world.z - chunk.origin.z).floor() as i32;
        chunk.get(lx, ly, lz) != 0
    }

    /// Does any solid voxel overlap the axis-aligned box `[min, max]`?
    /// Used by `KamiApp` AABB sweep for wall collision.
    pub fn aabb_solid(&self, min: Vec3, max: Vec3) -> bool {
        let x0 = min.x.floor() as i32;
        let x1 = max.x.ceil() as i32;
        let y0 = min.y.floor() as i32;
        let y1 = max.y.ceil() as i32;
        let z0 = min.z.floor() as i32;
        let z1 = max.z.ceil() as i32;
        for x in x0..x1 {
            for y in y0..y1 {
                for z in z0..z1 {
                    if self.is_solid(Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Amanatides & Woo DDA raycast across loaded chunks. Steps 1
    /// voxel cell at a time and returns the first solid cell hit + the
    /// face normal. `max_dist` is meters.
    pub fn raycast(
        &self,
        origin: Vec3,
        dir: Vec3,
        max_dist: f32,
    ) -> Option<(glam::IVec3, Vec3)> {
        let d = dir.normalize();
        let mut x = origin.x.floor() as i32;
        let mut y = origin.y.floor() as i32;
        let mut z = origin.z.floor() as i32;
        let step_x = if d.x > 0.0 { 1 } else if d.x < 0.0 { -1 } else { 0 };
        let step_y = if d.y > 0.0 { 1 } else if d.y < 0.0 { -1 } else { 0 };
        let step_z = if d.z > 0.0 { 1 } else if d.z < 0.0 { -1 } else { 0 };
        let next_boundary = |v: f32, step: i32| -> f32 {
            if step > 0 { v.floor() + 1.0 } else { v.floor() }
        };
        let t_for = |boundary: f32, start: f32, dir_c: f32| -> f32 {
            if dir_c == 0.0 { f32::INFINITY } else { (boundary - start) / dir_c }
        };
        let mut t_max_x = t_for(next_boundary(origin.x, step_x), origin.x, d.x);
        let mut t_max_y = t_for(next_boundary(origin.y, step_y), origin.y, d.y);
        let mut t_max_z = t_for(next_boundary(origin.z, step_z), origin.z, d.z);
        let t_delta_x = if d.x != 0.0 { (1.0 / d.x).abs() } else { f32::INFINITY };
        let t_delta_y = if d.y != 0.0 { (1.0 / d.y).abs() } else { f32::INFINITY };
        let t_delta_z = if d.z != 0.0 { (1.0 / d.z).abs() } else { f32::INFINITY };

        let mut normal = Vec3::ZERO;
        let mut t = 0.0_f32;
        while t <= max_dist {
            if self.is_solid(Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5)) {
                return Some((glam::IVec3::new(x, y, z), normal));
            }
            if t_max_x < t_max_y && t_max_x < t_max_z {
                x += step_x;
                t = t_max_x;
                t_max_x += t_delta_x;
                normal = Vec3::new(-step_x as f32, 0.0, 0.0);
            } else if t_max_y < t_max_z {
                y += step_y;
                t = t_max_y;
                t_max_y += t_delta_y;
                normal = Vec3::new(0.0, -step_y as f32, 0.0);
            } else {
                z += step_z;
                t = t_max_z;
                t_max_z += t_delta_z;
                normal = Vec3::new(0.0, 0.0, -step_z as f32);
            }
        }
        None
    }

    /// Place a voxel at block coord with material id. Creates the
    /// owning chunk if missing (only for streaming mode — static mode
    /// returns early if chunk doesn't exist). Same mesh rebuild logic
    /// as `break_voxel`.
    pub fn set_voxel(&self, block: glam::IVec3, material: u8) {
        let cs = CHUNK_SIZE as i32;
        let cc = (
            block.x.div_euclid(cs),
            block.y.div_euclid(cs),
            block.z.div_euclid(cs),
        );
        let lx = block.x.rem_euclid(cs) as usize;
        let ly = block.y.rem_euclid(cs) as usize;
        let lz = block.z.rem_euclid(cs) as usize;
        {
            let mut cpu = self.inner.cpu.borrow_mut();
            let chunk = cpu.entry(cc).or_insert_with(|| {
                VoxelChunk::new(Vec3::new(
                    (cc.0 * cs) as f32,
                    (cc.1 * cs) as f32,
                    (cc.2 * cs) as f32,
                ))
            });
            if chunk.get(lx as i32, ly as i32, lz as i32) == material {
                return;
            }
            chunk.set(lx, ly, lz, material);
        }
        self.rebuild_chunk_gpu(cc);
        if lx == 0 { self.rebuild_chunk_gpu((cc.0 - 1, cc.1, cc.2)); }
        if lx == (cs - 1) as usize { self.rebuild_chunk_gpu((cc.0 + 1, cc.1, cc.2)); }
        if ly == 0 { self.rebuild_chunk_gpu((cc.0, cc.1 - 1, cc.2)); }
        if ly == (cs - 1) as usize { self.rebuild_chunk_gpu((cc.0, cc.1 + 1, cc.2)); }
        if lz == 0 { self.rebuild_chunk_gpu((cc.0, cc.1, cc.2 - 1)); }
        if lz == (cs - 1) as usize { self.rebuild_chunk_gpu((cc.0, cc.1, cc.2 + 1)); }
    }

    /// Mine: remove voxel at block coord, regenerate that chunk's GPU
    /// mesh (and neighbours if the removed block exposed their faces).
    pub fn break_voxel(&self, block: glam::IVec3) {
        let cs = CHUNK_SIZE as i32;
        let cc = (
            block.x.div_euclid(cs),
            block.y.div_euclid(cs),
            block.z.div_euclid(cs),
        );
        let lx = block.x.rem_euclid(cs) as usize;
        let ly = block.y.rem_euclid(cs) as usize;
        let lz = block.z.rem_euclid(cs) as usize;
        {
            let mut cpu = self.inner.cpu.borrow_mut();
            let Some(chunk) = cpu.get_mut(&cc) else { return };
            if chunk.get(lx as i32, ly as i32, lz as i32) == 0 {
                return;
            }
            chunk.set(lx, ly, lz, 0);
        }
        // Rebuild the owning chunk's GPU mesh.
        self.rebuild_chunk_gpu(cc);
        // Rebuild neighbours if the mined block sat on their boundary
        // (face exposure now changed).
        if lx == 0 { self.rebuild_chunk_gpu((cc.0 - 1, cc.1, cc.2)); }
        if lx == (cs - 1) as usize { self.rebuild_chunk_gpu((cc.0 + 1, cc.1, cc.2)); }
        if ly == 0 { self.rebuild_chunk_gpu((cc.0, cc.1 - 1, cc.2)); }
        if ly == (cs - 1) as usize { self.rebuild_chunk_gpu((cc.0, cc.1 + 1, cc.2)); }
        if lz == 0 { self.rebuild_chunk_gpu((cc.0, cc.1, cc.2 - 1)); }
        if lz == (cs - 1) as usize { self.rebuild_chunk_gpu((cc.0, cc.1, cc.2 + 1)); }
    }

    fn rebuild_chunk_gpu(&self, cc: (i32, i32, i32)) {
        let cpu = self.inner.cpu.borrow();
        let Some(chunk) = cpu.get(&cc) else { return };
        let new_gpu = build_chunk_gpu_with_device(&self.inner.device, chunk, &self.inner.palette);
        drop(cpu);
        let mut gpu = self.inner.gpu.borrow_mut();
        match new_gpu {
            Some(g) => { gpu.insert(cc, g); }
            None => { gpu.remove(&cc); }
        }
    }

    /// Find the topmost solid voxel directly below `world` (within the
    /// same vertical column). Returns the surface Y, or `None` if no
    /// voxels are loaded in that XZ column.
    pub fn sample_floor(&self, world: Vec3) -> Option<f32> {
        let cx = (world.x / CHUNK_SIZE as f32).floor() as i32;
        let cz = (world.z / CHUNK_SIZE as f32).floor() as i32;
        let cpu = self.inner.cpu.borrow();
        // Scan all loaded chunks in this column, highest cy first.
        let mut best: Option<f32> = None;
        for ((ccx, _cy, ccz), chunk) in cpu.iter() {
            if *ccx != cx || *ccz != cz {
                continue;
            }
            let lx = (world.x - chunk.origin.x).floor() as i32;
            let lz = (world.z - chunk.origin.z).floor() as i32;
            if lx < 0 || lx >= CHUNK_SIZE as i32 || lz < 0 || lz >= CHUNK_SIZE as i32 {
                continue;
            }
            // Scan top-down for highest solid.
            for ly in (0..CHUNK_SIZE as i32).rev() {
                if chunk.get(lx, ly, lz) != 0 {
                    let surface = chunk.origin.y + ly as f32 + 1.0;
                    best = Some(match best {
                        Some(b) if b >= surface => b,
                        _ => surface,
                    });
                    break;
                }
            }
        }
        best
    }
}

fn build_chunk_gpu(
    ctx: &RenderContext,
    chunk: &VoxelChunk,
    palette: &[[f32; 3]],
) -> Option<ChunkGpu> {
    build_chunk_gpu_with_device(&ctx.device, chunk, palette)
}

/// Greedy meshing — merges coplanar same-material exposed faces into
/// the largest possible rectangular quads per slice. Typical reduction
/// for blocky terrain: 50-80 % fewer vertices vs the naive per-cell
/// face emission. Classic Minecraft algorithm (Mikola Lysenko 2012,
/// MIT-licensed reference; implementation here is a clean reimpl).
///
/// For each of the 6 face directions (±X, ±Y, ±Z), iterate the N=16
/// slices perpendicular to that axis. On each slice, build a 2D mask
/// of (material id for exposed faces, 0 elsewhere). Scan the mask
/// row-major; on a non-zero cell, extend rightward (v) while material
/// matches, then extend downward (u) while the full row matches,
/// zero the merged rectangle and emit one quad for it.
fn greedy_face_quads(
    chunk: &VoxelChunk,
    d: usize,
    sign: i32,
    palette: &[[f32; 3]],
    verts: &mut Vec<VoxelVertex>,
) {
    let u = (d + 1) % 3;
    let v = (d + 2) % 3;
    let cs = CHUNK_SIZE;
    let origin = chunk.origin();

    for n in 0..cs as i32 {
        // Build exposure mask for this slice.
        let mut mask = vec![0u8; cs * cs];
        for uc in 0..cs {
            for vc in 0..cs {
                let mut pos = [0i32; 3];
                pos[d] = n;
                pos[u] = uc as i32;
                pos[v] = vc as i32;
                let mat = chunk.get(pos[0], pos[1], pos[2]);
                if mat == 0 {
                    continue;
                }
                let mut npos = pos;
                npos[d] = n + sign;
                if chunk.get(npos[0], npos[1], npos[2]) == 0 {
                    mask[uc * cs + vc] = mat;
                }
            }
        }

        // Greedy rectangle merge.
        for uc in 0..cs {
            let mut vc = 0;
            while vc < cs {
                let mat = mask[uc * cs + vc];
                if mat == 0 {
                    vc += 1;
                    continue;
                }
                // Extend along v.
                let mut v_end = vc + 1;
                while v_end < cs && mask[uc * cs + v_end] == mat {
                    v_end += 1;
                }
                // Extend along u.
                let mut u_end = uc + 1;
                'outer: while u_end < cs {
                    for vv in vc..v_end {
                        if mask[u_end * cs + vv] != mat {
                            break 'outer;
                        }
                    }
                    u_end += 1;
                }
                // Zero the merged region.
                for uu in uc..u_end {
                    for vv in vc..v_end {
                        mask[uu * cs + vv] = 0;
                    }
                }

                // Emit one quad.
                let plane = if sign > 0 { (n + 1) as f32 } else { n as f32 };
                let mut corners = [[0.0_f32; 3]; 4];
                let uv_lo_hi = [
                    (uc as f32, vc as f32),
                    (u_end as f32, vc as f32),
                    (u_end as f32, v_end as f32),
                    (uc as f32, v_end as f32),
                ];
                for (i, (uu, vv)) in uv_lo_hi.iter().enumerate() {
                    corners[i][d] = plane;
                    corners[i][u] = *uu;
                    corners[i][v] = *vv;
                    corners[i][0] += origin.x;
                    corners[i][1] += origin.y;
                    corners[i][2] += origin.z;
                }
                let col = palette.get(mat as usize).copied().unwrap_or([1.0, 0.0, 1.0]);
                let mut normal = [0.0_f32; 3];
                normal[d] = sign as f32;

                // Winding: cross(u_axis, v_axis) = +normal for +sign,
                // reversed for -sign. Back-cull requires CCW from
                // outside; flip winding for -sign.
                let idx: [usize; 6] = if sign > 0 {
                    [0, 1, 2, 0, 2, 3]
                } else {
                    [0, 2, 1, 0, 3, 2]
                };
                for &i in &idx {
                    verts.push(VoxelVertex {
                        pos: corners[i],
                        norm: normal,
                        col,
                    });
                }

                vc = v_end;
            }
        }
    }
}

fn build_chunk_gpu_with_device(
    device: &wgpu::Device,
    chunk: &VoxelChunk,
    palette: &[[f32; 3]],
) -> Option<ChunkGpu> {
    let mut verts: Vec<VoxelVertex> = Vec::new();
    for d in 0..3 {
        for sign in [1, -1] {
            greedy_face_quads(chunk, d, sign, palette, &mut verts);
        }
    }
    if verts.is_empty() {
        return None;
    }
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("voxel.chunk.vb"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    Some(ChunkGpu {
        vb,
        vertex_count: verts.len() as u32,
    })
}

impl RenderPipeline for VoxelChunkAdapter {
    fn prepare(&mut self, ctx: &RenderContext, camera: &Camera, _world: &World) {
        if self.inner.view_radius <= 0 || self.inner.generator.is_none() {
            return;
        }
        let eye = Vec3::from_array(camera.as_render().uniform().position);
        let ccx = (eye.x / CHUNK_SIZE as f32).floor() as i32;
        let ccy = (eye.y / CHUNK_SIZE as f32).floor() as i32;
        let ccz = (eye.z / CHUNK_SIZE as f32).floor() as i32;
        let r = self.inner.view_radius;

        // Unload chunks outside window (but keep pre-authored chunks).
        // Pre-authored is identified by "not in generator's output" —
        // for simplicity, we treat all entries the same. Static-use
        // sites should call `insert_chunk` on a non-streaming adapter.
        {
            let mut gpu = self.inner.gpu.borrow_mut();
            let mut cpu = self.inner.cpu.borrow_mut();
            gpu.retain(|&(cx, cy, cz), _| {
                (cx - ccx).abs() <= r && (cy - ccy).abs() <= r && (cz - ccz).abs() <= r
            });
            cpu.retain(|&(cx, cy, cz), _| {
                (cx - ccx).abs() <= r && (cy - ccy).abs() <= r && (cz - ccz).abs() <= r
            });
        }

        // Enqueue missing chunks.
        {
            let gpu = self.inner.gpu.borrow();
            let cpu = self.inner.cpu.borrow();
            let mut pending = self.inner.pending.borrow_mut();
            for dz in -r..=r {
                for dy in -r..=r {
                    for dx in -r..=r {
                        let coord = (ccx + dx, ccy + dy, ccz + dz);
                        if !gpu.contains_key(&coord)
                            && !cpu.contains_key(&coord)
                            && !pending.iter().any(|c| *c == coord)
                        {
                            pending.push_back(coord);
                        }
                    }
                }
            }
        }

        // Budget: 1 chunk per frame.
        let coord = self.inner.pending.borrow_mut().pop_front();
        if let Some(coord) = coord {
            if (coord.0 - ccx).abs() <= r
                && (coord.1 - ccy).abs() <= r
                && (coord.2 - ccz).abs() <= r
                && !self.inner.cpu.borrow().contains_key(&coord)
            {
                let gen_fn = self.inner.generator.as_ref().unwrap();
                if let Some(chunk) = gen_fn(coord.0, coord.1, coord.2) {
                    if let Some(gpu) = build_chunk_gpu(ctx, &chunk, &self.inner.palette) {
                        self.inner.gpu.borrow_mut().insert(coord, gpu);
                    }
                    self.inner.cpu.borrow_mut().insert(coord, chunk);
                } else {
                    // Generator returned empty; record an empty CPU entry
                    // so we don't re-enqueue it every frame.
                    self.inner.cpu.borrow_mut().insert(coord, VoxelChunk::new(
                        Vec3::new(
                            (coord.0 * CHUNK_SIZE as i32) as f32,
                            (coord.1 * CHUNK_SIZE as i32) as f32,
                            (coord.2 * CHUNK_SIZE as i32) as f32,
                        ),
                    ));
                }
            }
        }
    }

    fn record(
        &self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &Camera,
        _world: &World,
    ) {
        let gpu = self.inner.gpu.borrow();
        if gpu.is_empty() {
            return;
        }
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let vp = proj * view_m;
        let sun_dir = sun_from_time(camera.time);
        let fog = fog_from_sun(sun_dir);
        let warmth = 1.0 - sun_dir.y.max(0.0);
        let sun_color = [1.0, 0.96 - warmth * 0.12, 0.88 - warmth * 0.28];
        let vu = VoxelUniform {
            view_proj: vp.to_cols_array(),
            cam_pos: u.position,
            _p0: 0.0,
            sun_dir: sun_dir.to_array(),
            _p1: 0.0,
            sun_color,
            fog_density: 0.0012,
            fog_color: fog.to_array(),
            _p2: 0.0,
        };
        ctx.queue
            .write_buffer(&self.inner.pipeline.uniform, 0, bytemuck::bytes_of(&vu));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("voxel.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.inner.pipeline.pipeline);
        pass.set_bind_group(0, &self.inner.pipeline.bind_group, &[]);
        for chunk in gpu.values() {
            pass.set_vertex_buffer(0, chunk.vb.slice(..));
            pass.draw(0..chunk.vertex_count, 0..1);
        }
    }
}
