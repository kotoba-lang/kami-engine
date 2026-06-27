//! `CadSceneAdapter` — renders pre-tessellated CAD part batches with
//! mouse-projected ray-pick + amber highlight.
//!
//! Phase 1 contract: callers hand over triangle batches (positions +
//! normals + indices + base colour + world transform + feature_id +
//! feature_name). BREP → mesh tessellation happens either in the game
//! crate (via `kami-sdf` / `kami-scad` for demos) or in the
//! `cad-job.etzhayyim.com` CF Container service (for real STEP / IGES
//! imports). The adapter stays dumb about topology: it only cares
//! about GPU-ready triangles + a caller-assigned string id per batch.
//!
//! Clone-safe (`Rc<CadInner>`) so `KamiApp::with_pipeline(adapter.clone())`
//! and a tick hook can share the same GPU + CPU state.

use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::RenderContext;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::scene_mesh::SceneMeshCore;

struct CpuPickData {
    feature_id: String,
    feature_name: String,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    base_color: [f32; 3],
}

struct CadInner {
    core: RefCell<SceneMeshCore>,
    cpu: RefCell<Vec<CpuPickData>>,
    selected: Cell<Option<usize>>,
    device: wgpu::Device,
}

#[derive(Clone)]
pub struct CadSceneAdapter {
    inner: Rc<CadInner>,
}

#[derive(Debug, Clone)]
pub struct CadPick {
    pub feature_id: String,
    pub feature_name: String,
    pub world_pos: Vec3,
    pub distance: f32,
}

impl CadSceneAdapter {
    pub fn new(ctx: &RenderContext) -> Self {
        Self {
            inner: Rc::new(CadInner {
                core: RefCell::new(SceneMeshCore::new(ctx, "cad_scene", 0.0002)),
                cpu: RefCell::new(Vec::new()),
                selected: Cell::new(None),
                device: ctx.device.clone(),
            }),
        }
    }

    pub fn push_triangles(
        &self,
        ctx: &RenderContext,
        feature_id: impl Into<String>,
        feature_name: impl Into<String>,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
        base_color: [f32; 3],
        world: Mat4,
    ) {
        if positions.is_empty() || indices.is_empty() {
            return;
        }
        let mut core = self.inner.core.borrow_mut();
        let mut cpu = self.inner.cpu.borrow_mut();

        let normal_mat = world.inverse().transpose();
        let world_positions: Vec<[f32; 3]> = positions
            .iter()
            .map(|p| {
                let wp = world.transform_point3(Vec3::new(p[0], p[1], p[2]));
                [wp.x, wp.y, wp.z]
            })
            .collect();
        let world_normals: Vec<[f32; 3]> = normals
            .iter()
            .map(|n| {
                let wn = normal_mat
                    .transform_vector3(Vec3::new(n[0], n[1], n[2]))
                    .normalize_or_zero();
                [wn.x, wn.y, wn.z]
            })
            .collect();

        core.push_batch(ctx, positions, normals, indices, base_color, world);
        cpu.push(CpuPickData {
            feature_id: feature_id.into(),
            feature_name: feature_name.into(),
            positions: world_positions,
            normals: world_normals,
            indices: indices.to_vec(),
            base_color,
        });
    }

    pub fn batch_count(&self) -> usize {
        self.inner.core.borrow().batches.len()
    }

    /// Re-bake batch `idx` from model-local geometry with a new `world`
    /// transform. Lets an animated scene (e.g. the kami-genesis physics arm)
    /// move a part every frame without rebuilding the whole scene. Keeps the
    /// CPU pick data in sync so ray-pick still hits the moved part. A no-op if
    /// `idx` is out of range or the batch is currently highlighted (selection
    /// colour is restored on the next `set_highlighted`).
    pub fn replace_batch_world(
        &self,
        idx: usize,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
        base_color: [f32; 3],
        world: Mat4,
    ) {
        if positions.is_empty() || indices.is_empty() {
            return;
        }
        let normal_mat = world.inverse().transpose();
        let world_positions: Vec<[f32; 3]> = positions
            .iter()
            .map(|p| {
                let wp = world.transform_point3(Vec3::new(p[0], p[1], p[2]));
                [wp.x, wp.y, wp.z]
            })
            .collect();
        let world_normals: Vec<[f32; 3]> = normals
            .iter()
            .map(|n| {
                let wn = normal_mat
                    .transform_vector3(Vec3::new(n[0], n[1], n[2]))
                    .normalize_or_zero();
                [wn.x, wn.y, wn.z]
            })
            .collect();

        {
            let mut core = self.inner.core.borrow_mut();
            core.replace_batch(
                &self.inner.device,
                idx,
                positions,
                normals,
                indices,
                base_color,
                world,
            );
        }
        let mut cpu = self.inner.cpu.borrow_mut();
        if let Some(batch) = cpu.get_mut(idx) {
            batch.positions = world_positions;
            batch.normals = world_normals;
            batch.indices = indices.to_vec();
            batch.base_color = base_color;
        }
    }

    pub fn pick_ray(&self, origin: Vec3, dir: Vec3) -> Option<CadPick> {
        let dir = dir.normalize_or_zero();
        if dir.length_squared() < 1.0e-6 {
            return None;
        }
        let cpu = self.inner.cpu.borrow();
        let mut best: Option<(f32, usize, Vec3)> = None;
        for (i, batch) in cpu.iter().enumerate() {
            for tri in batch.indices.chunks_exact(3) {
                let a = Vec3::from_array(batch.positions[tri[0] as usize]);
                let b = Vec3::from_array(batch.positions[tri[1] as usize]);
                let c = Vec3::from_array(batch.positions[tri[2] as usize]);
                if let Some(t) = ray_triangle(origin, dir, a, b, c) {
                    match best {
                        Some((best_t, _, _)) if best_t <= t => {}
                        _ => best = Some((t, i, origin + dir * t)),
                    }
                }
            }
        }
        best.map(|(t, i, p)| CadPick {
            feature_id: cpu[i].feature_id.clone(),
            feature_name: cpu[i].feature_name.clone(),
            world_pos: p,
            distance: t,
        })
    }

    pub fn pick_from_camera_if_clicked(&self, camera: &mut Camera) -> Option<CadPick> {
        if !camera.consume_action() {
            return None;
        }
        let (origin, dir) = camera.ray_from_ndc(camera.mouse_ndc_x, camera.mouse_ndc_y);
        self.pick_ray(origin, dir)
    }

    pub fn set_highlighted_by_id(&self, feature_id: &str) {
        let idx = {
            let cpu = self.inner.cpu.borrow();
            cpu.iter().position(|b| b.feature_id == feature_id)
        };
        self.set_highlighted(idx);
    }

    pub fn set_highlighted(&self, batch_index: Option<usize>) {
        if self.inner.selected.get() == batch_index {
            return;
        }
        if let Some(prev) = self.inner.selected.get() {
            self.rebuild_batch_color(prev, None);
        }
        if let Some(next) = batch_index {
            self.rebuild_batch_color(next, Some([1.0, 0.78, 0.22]));
        }
        self.inner.selected.set(batch_index);
    }

    pub fn selected_feature(&self) -> Option<(String, String)> {
        let cpu = self.inner.cpu.borrow();
        self.inner.selected.get().and_then(|i| {
            cpu.get(i)
                .map(|b| (b.feature_id.clone(), b.feature_name.clone()))
        })
    }

    fn rebuild_batch_color(&self, idx: usize, override_color: Option<[f32; 3]>) {
        let cpu = self.inner.cpu.borrow();
        let Some(batch) = cpu.get(idx) else { return };
        let color = override_color.unwrap_or(batch.base_color);
        let mut core = self.inner.core.borrow_mut();
        core.replace_batch_color(
            &self.inner.device,
            idx,
            &batch.positions,
            &batch.normals,
            &batch.indices,
            color,
        );
    }
}

impl RenderPipeline for CadSceneAdapter {
    fn record(
        &self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &Camera,
        _world: &World,
    ) {
        let core = self.inner.core.borrow();
        core.record(ctx, encoder, view, depth_view, camera);
    }
}

fn ray_triangle(origin: Vec3, dir: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<f32> {
    const EPS: f32 = 1.0e-6;
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = dir.cross(edge2);
    let a = edge1.dot(h);
    if a.abs() < EPS {
        return None;
    }
    let f = 1.0 / a;
    let s = origin - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = f * dir.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * edge2.dot(q);
    if t > EPS { Some(t) } else { None }
}
