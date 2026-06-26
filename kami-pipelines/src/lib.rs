//! Shared `RenderPipeline` adapters.
//!
//! Wraps `kami-render::scene_pipelines` + `kami-terrain` +
//! `kami-vegetation` into composable units any `kami-app-{game}` crate
//! can register via the builder. Game-specific pipelines (voxel PBR,
//! SDF character, brainrot NPC shader) stay in their own game crate.
//!
//! Current adapters:
//! - `SkyAdapter` — procedural atmosphere + cloud ray-march
//! - `TerrainAdapter` — streaming heightmap chunks with integrated
//!   per-chunk grass instancing
//! - helpers: `sun_from_time`, `fog_from_sun`

pub mod water;
pub use water::WaterAdapter;

pub mod voxel;
pub use voxel::{CHUNK_SIZE, VoxelChunk, VoxelChunkAdapter, VoxelPalette};

pub mod particle;
pub use particle::ParticleAdapter;

pub mod field_vis;
pub use field_vis::{FieldLayer, FieldVisAdapter};

pub mod edge_vis;
pub use edge_vis::EdgeVisAdapter;

pub mod face_vis;
pub use face_vis::FaceVisAdapter;

pub mod atlas_vis;
pub use atlas_vis::{AtlasSprite, AtlasVisAdapter, atlas_slot};

pub mod field_icon;
pub use field_icon::{FieldIcon, FieldIconMap, FieldIconRule};

mod scene_mesh;
pub use scene_mesh::{unit_box, unit_cylinder};

pub mod bim_scene;
pub use bim_scene::{BimSceneAdapter, Pick};

pub mod cad_scene;
pub use cad_scene::{CadPick, CadSceneAdapter};

pub mod gsplat;
pub use gsplat::{GsplatAdapter, GsplatError, GsplatFormat, MAX_SPLATS_PER_CLOUD};

use glam::{Mat4, Vec3};
use hecs::World;
use kami_app::{Camera, RenderPipeline};
use kami_render::RenderContext;
use kami_render::scene_pipelines::{SkyPipeline, SkyUniform};

/// Day/night sun direction from `camera.time`. Period = 180 s
/// (3 minutes for a full cycle — fast enough to observe during testing).
/// Returns a normalized `(x, y, z)` sun direction. `y > 0` = above horizon.
pub fn sun_from_time(time: f32) -> Vec3 {
    let period = 180.0_f32;
    let angle = (time / period) * std::f32::consts::TAU;
    // Slight east-west drift + high noon altitude; keep z small so the
    // sun crosses the scene rather than circling overhead.
    let y = angle.sin();
    let x = angle.cos() * 0.55;
    let z = 0.35;
    Vec3::new(x, y, z).normalize()
}

/// Horizon color from sun altitude. Daytime = soft blue, sunset = warm
/// orange, night = deep indigo. Used by both Sky and Terrain fog.
pub fn fog_from_sun(sun_dir: Vec3) -> Vec3 {
    let alt = sun_dir.y.clamp(-1.0, 1.0);
    // Day (alt ≈ 1) → pale sky blue.
    let day = Vec3::new(0.62, 0.74, 0.88);
    // Sunset (alt ≈ 0) → warm orange.
    let dusk = Vec3::new(0.92, 0.55, 0.35);
    // Night (alt ≈ -1) → indigo.
    let night = Vec3::new(0.08, 0.10, 0.18);
    if alt >= 0.0 {
        // Blend dusk → day as alt rises.
        let t = alt.powf(0.5);
        dusk * (1.0 - t) + day * t
    } else {
        // Below horizon: dusk → night as it sinks.
        let t = (-alt).powf(0.5);
        dusk * (1.0 - t) + night * t
    }
}

/// `SkyPipeline` adapter — wraps `kami_render::scene_pipelines::SkyPipeline`
/// into the `RenderPipeline` trait.
///
/// Writes a fullscreen quad sampling the procedural atmosphere (rayleigh
/// gradient + sun disc + cloud ray-march). Needs `inv_vp` uniform updated
/// per frame from the camera.
pub struct SkyAdapter {
    pipeline: SkyPipeline,
    overcast: f32,
}

impl SkyAdapter {
    pub fn new(ctx: &RenderContext) -> Self {
        let pipeline = SkyPipeline::new(&ctx.device, ctx.format);
        Self {
            pipeline,
            overcast: 0.15,
        }
    }

    pub fn with_overcast(mut self, v: f32) -> Self {
        self.overcast = v.clamp(0.0, 1.0);
        self
    }
}

impl RenderPipeline for SkyAdapter {
    fn prepare(&mut self, _ctx: &RenderContext, _camera: &Camera, _world: &World) {}

    fn record(
        &self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera: &Camera,
        _world: &World,
    ) {
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let inv_vp = (proj * view_m).inverse();
        let eye = Vec3::from_array(u.position);
        let sun_dir = sun_from_time(camera.time);
        let fog_color = fog_from_sun(sun_dir);
        let sky_u = SkyUniform {
            inv_vp: inv_vp.to_cols_array(),
            cam_pos: eye.to_array(),
            _p0: 0.0,
            sun_dir: sun_dir.to_array(),
            _p1: 0.0,
            fog_color: fog_color.to_array(),
            overcast: self.overcast,
            scroll_x: camera.time * 0.02,
            scroll_z: camera.time * 0.015,
            altitude: 1200.0,
            _p2: 0.0,
        };
        ctx.queue
            .write_buffer(&self.pipeline.uniform, 0, bytemuck::bytes_of(&sky_u));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("isekai-v2.sky"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.55,
                        g: 0.70,
                        b: 0.86,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline.pipeline);
        pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
        // Sky uses a procedural fullscreen triangle (3 vertices, no vertex buffer).
        pass.draw(0..3, 0..1);
    }
}

/// `TerrainPipeline` adapter with **runtime chunk streaming**.
///
/// Keeps a `(2*radius+1)²` window of chunks centered on the camera.
/// Each `prepare()` tick:
///   1. computes camera's chunk coord
///   2. unloads chunks outside the window
///   3. enqueues missing chunks into `pending`
///   4. generates + uploads **one** chunk per frame (budget — avoids
///      frame spikes)
///
/// FBM noise is world-space continuous so neighbouring chunks share
/// edge heights (seamless). Memory: ~50k verts × 48 B × (2r+1)² chunks.
struct TerrainChunkGpu {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    index_count: u32,
    /// Per-chunk vegetation instance buffer (all species mixed).
    /// `place_instances` returns instances sorted-by-species, so
    /// `veg_ranges` describes contiguous `(species_id, start, count)`
    /// spans for per-species indexed instanced draws.
    veg_instance_vb: Option<wgpu::Buffer>,
    veg_ranges: Vec<(u32, u32, u32)>,
}

pub struct TerrainAdapter {
    pipeline: kami_render::scene_pipelines::TerrainPipeline,
    veg_pipeline: kami_render::scene_pipelines::VegetationPipeline,
    chunks: std::collections::HashMap<(i32, i32), TerrainChunkGpu>,
    pending: std::collections::VecDeque<(i32, i32)>,
    hm_cfg: kami_terrain::HeightmapConfig,
    splat_thresholds: kami_terrain::SplatThresholds,
    palette: kami_terrain::MaterialPalette,
    /// Chunk size in world meters. Vertices are **always 1m-spaced**.
    chunk_extent: u32,
    view_radius: i32,
}

impl TerrainAdapter {
    /// Streaming terrain. Loads `(2*view_radius + 1)²` chunks around the
    /// camera; new chunks arrive at 1 per frame.
    ///
    /// `chunk_extent` is world meters per chunk; vertex spacing is fixed
    /// at 1m (per `Heightmap::generate`), so verts per chunk =
    /// `(chunk_extent + 1)²`. For isekai, 128m → 129² = 16.6k verts per
    /// chunk.
    pub fn streaming(
        ctx: &RenderContext,
        biome: kami_terrain::BiomePreset,
        seed: f32,
        chunk_extent: u32,
        view_radius: i32,
    ) -> Self {
        Self::streaming_with_config(
            ctx,
            biome.heightmap(seed),
            biome.splat_thresholds(),
            biome.palette(),
            chunk_extent,
            view_radius,
        )
    }

    /// Like [`streaming`](Self::streaming) but seeded from explicit terrain config structs
    /// instead of a hardcoded [`kami_terrain::BiomePreset`] — the **executor edge** a
    /// consumer uses to drive terrain from `kami-terrain-scene`'s biome EDN (ADR-0044/0046):
    /// `resolve_biome(name)` →
    /// `BiomeSpec::{to_heightmap_config,to_splat_thresholds,to_material_palette}` → here.
    /// Behaviourally identical to `streaming` when given the same biome's configs.
    pub fn streaming_with_config(
        ctx: &RenderContext,
        hm_cfg: kami_terrain::HeightmapConfig,
        splat_thresholds: kami_terrain::SplatThresholds,
        palette: kami_terrain::MaterialPalette,
        chunk_extent: u32,
        view_radius: i32,
    ) -> Self {
        let pipeline = kami_render::scene_pipelines::TerrainPipeline::new(&ctx.device, ctx.format);
        // Vegetation pipeline with an unused instance_vb (capacity=1 dummy);
        // each chunk owns its own instance_vb that we bind at draw time.
        let mut veg_pipeline =
            kami_render::scene_pipelines::VegetationPipeline::new(&ctx.device, ctx.format, 1);
        // Upload cross-quad species meshes (grass is species 0).
        let mesh_lib = kami_vegetation::mesh::species_mesh_library();
        let meshes: Vec<(u32, Vec<f32>, Vec<u32>)> = mesh_lib
            .into_iter()
            .map(|(id, m)| (id as u32, m.vertices, m.indices))
            .collect();
        veg_pipeline.upload_species_meshes(&ctx.device, &meshes);
        Self {
            pipeline,
            veg_pipeline,
            chunks: std::collections::HashMap::new(),
            pending: std::collections::VecDeque::new(),
            hm_cfg,
            splat_thresholds,
            palette,
            chunk_extent,
            view_radius,
        }
    }

    fn generate_chunk(&self, ctx: &RenderContext, cx: i32, cz: i32) -> TerrainChunkGpu {
        use wgpu::util::DeviceExt;
        // +1 because heightmap needs N+1 samples to cover N meters with
        // shared edge vertices. This guarantees adjacent chunks produce
        // identical heights at the shared boundary.
        let res = self.chunk_extent + 1;
        let ox = (cx * self.chunk_extent as i32) as f32;
        let oz = (cz * self.chunk_extent as i32) as f32;
        let hm = kami_terrain::Heightmap::generate(res, res, ox, oz, &self.hm_cfg);
        let splat = kami_terrain::Splatmap::from_heightmap(
            &hm,
            self.splat_thresholds.sand_line,
            self.splat_thresholds.snow_line,
            self.splat_thresholds.rock_slope,
        );
        // scale = 1.0 because Heightmap::generate samples at 1 world-m
        // per cell. Using scale != 1.0 stretches the mesh horizontally
        // without re-sampling the heights → visible seams.
        let mesh = kami_terrain::generate_chunk_mesh(&hm, &splat, ox, oz, 1, 1.0, 0);
        let vb = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("isekai-v2.terrain.vb"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ib = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("isekai-v2.terrain.ib"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        // Place vegetation instances inside this chunk. `place_instances`
        // centres its scatter at world origin; we translate into chunk-
        // local world space after. Filter to grass for isekai Plains
        // biome (1 species, dense coverage).
        let placement = kami_vegetation::place_instances(
            &hm,
            &splat,
            ox,
            oz,
            &kami_vegetation::PlacementConfig {
                seed: ((cx.wrapping_mul(73856093)) ^ (cz.wrapping_mul(19349663))) as u32,
                extent: self.chunk_extent as f32,
                // All 5 species (grass / fern / palm / conifer / bush);
                // each species applies its own height / slope / splat
                // affinity for biome-aware placement.
                density_scale: 0.25,
                species_filter: Vec::new(),
            },
        );
        // Post-translate from centred-at-origin to chunk-local world.
        let chunk_cx = ox + (self.chunk_extent as f32) * 0.5;
        let chunk_cz = oz + (self.chunk_extent as f32) * 0.5;
        let mut instances: Vec<kami_vegetation::InstanceData> = placement;
        for inst in instances.iter_mut() {
            inst.position[0] += chunk_cx;
            inst.position[2] += chunk_cz;
        }
        // Compute contiguous (species, start, count) ranges. Relies on
        // `place_instances` returning species-sorted output.
        let mut veg_ranges: Vec<(u32, u32, u32)> = Vec::new();
        if !instances.is_empty() {
            let mut current = instances[0].species as u32;
            let mut start: u32 = 0;
            for (i, inst) in instances.iter().enumerate() {
                let sp = inst.species as u32;
                if sp != current {
                    veg_ranges.push((current, start, i as u32 - start));
                    current = sp;
                    start = i as u32;
                }
            }
            veg_ranges.push((current, start, instances.len() as u32 - start));
        }

        let veg_instance_vb = if instances.is_empty() {
            None
        } else {
            Some(
                ctx.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("veg.instances"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        };

        TerrainChunkGpu {
            vb,
            ib,
            index_count: mesh.indices.len() as u32,
            veg_instance_vb,
            veg_ranges,
        }
    }

    /// Legacy static-grid constructor (kept for tests / embed scenarios).
    pub fn from_biome_grid(
        ctx: &RenderContext,
        biome: kami_terrain::BiomePreset,
        seed: f32,
        grid: u32,
        chunk_extent: u32,
    ) -> Self {
        let mut adapter = Self::streaming(ctx, biome, seed, chunk_extent, 0);
        let half = grid as i32 / 2;
        for dz in -half..half {
            for dx in -half..half {
                let chunk = adapter.generate_chunk(ctx, dx, dz);
                adapter.chunks.insert((dx, dz), chunk);
            }
        }
        adapter
    }

    /// Backwards-compat single-chunk constructor.
    pub fn from_biome(
        ctx: &RenderContext,
        biome: kami_terrain::BiomePreset,
        seed: f32,
        extent: u32,
    ) -> Self {
        Self::from_biome_grid(ctx, biome, seed, 1, extent)
    }
}

impl RenderPipeline for TerrainAdapter {
    fn prepare(&mut self, ctx: &RenderContext, camera: &Camera, _world: &World) {
        if self.view_radius <= 0 {
            return; // static-grid mode (from_biome_grid) — skip streaming
        }
        // Camera chunk coord (floor div).
        let eye = Vec3::from_array(camera.as_render().uniform().position);
        let ext = self.chunk_extent as f32;
        let ccx = (eye.x / ext).floor() as i32;
        let ccz = (eye.z / ext).floor() as i32;
        let r = self.view_radius;

        // Unload chunks outside the window.
        self.chunks
            .retain(|&(cx, cz), _| (cx - ccx).abs() <= r && (cz - ccz).abs() <= r);

        // Enqueue missing chunks (spiral from center so near chunks load first).
        for ring in 0..=r {
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue; // skip interior of the ring
                    }
                    let coord = (ccx + dx, ccz + dz);
                    if !self.chunks.contains_key(&coord)
                        && !self.pending.iter().any(|c| *c == coord)
                    {
                        self.pending.push_back(coord);
                    }
                }
            }
        }

        // Budget: 1 chunk per frame. Generation + upload take ~5-15 ms.
        if let Some(coord) = self.pending.pop_front() {
            // Guard against stale pending entries (already outside window).
            if (coord.0 - ccx).abs() <= r
                && (coord.1 - ccz).abs() <= r
                && !self.chunks.contains_key(&coord)
            {
                let chunk = self.generate_chunk(ctx, coord.0, coord.1);
                self.chunks.insert(coord, chunk);
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
        let u = camera.as_render().uniform();
        let view_m = Mat4::from_cols_array_2d(&u.view);
        let proj = Mat4::from_cols_array_2d(&u.projection);
        let view_proj = proj * view_m;
        let eye = Vec3::from_array(u.position);
        let mut base_col = [[0.0; 4]; 4];
        let mut tip_col = [[0.0; 4]; 4];
        for i in 0..4 {
            base_col[i] = [
                self.palette.base[i][0],
                self.palette.base[i][1],
                self.palette.base[i][2],
                0.0,
            ];
            tip_col[i] = [
                self.palette.tip[i][0],
                self.palette.tip[i][1],
                self.palette.tip[i][2],
                0.0,
            ];
        }
        let sun_dir = sun_from_time(camera.time);
        let fog_color = fog_from_sun(sun_dir);
        // Sun color shifts warm near horizon, cool near noon.
        let warmth = 1.0 - sun_dir.y.max(0.0);
        let sun_color = [1.0, 0.96 - warmth * 0.12, 0.88 - warmth * 0.28];
        let t_u = kami_render::scene_pipelines::TerrainUniform {
            view_proj: view_proj.to_cols_array(),
            cam_pos: eye.to_array(),
            _p0: 0.0,
            sun_dir: sun_dir.to_array(),
            _p1: 0.0,
            sun_color,
            // 640m view radius at 1m/chunk → fog must fade near 500m so
            // far chunks don't pop in visually. 0.0012 gives ~500m half-life.
            fog_density: 0.0012,
            fog_color: fog_color.to_array(),
            _p2: 0.0,
            base_col,
            tip_col,
        };
        ctx.queue
            .write_buffer(&self.pipeline.uniform, 0, bytemuck::bytes_of(&t_u));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("isekai-v2.terrain"),
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
        pass.set_pipeline(&self.pipeline.pipeline);
        pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
        for chunk in self.chunks.values() {
            pass.set_vertex_buffer(0, chunk.vb.slice(..));
            pass.set_index_buffer(chunk.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..chunk.index_count, 0, 0..1);
        }

        // Vegetation pass: share pipeline + bind group, switch to
        // grass species mesh, bind each chunk's per-chunk instance
        // buffer at slot 1, draw as instanced.
        if !self.veg_pipeline.species_meshes.is_empty() {
            let v_u = kami_render::scene_pipelines::VegetationUniform {
                view_proj: view_proj.to_cols_array(),
                cam_pos: eye.to_array(),
                time: camera.time,
                sun_dir: sun_dir.to_array(),
                wind_speed: 2.0,
                fog_color: fog_color.to_array(),
                fog_density: 0.0012,
                wind_dir: [1.0, 0.3],
                gust_mul: 1.0,
                biome_dry: 0.0,
            };
            ctx.queue
                .write_buffer(&self.veg_pipeline.uniform, 0, bytemuck::bytes_of(&v_u));
            // Draw each chunk × each species range with the matching
            // per-species mesh. `veg_pipeline.species_meshes` is indexed
            // by `SpeciesId` u32 (0=grass, 1=fern, 2=palm, 3=conifer,
            // 4=bush). Since instances inside each chunk are sorted by
            // species, `(start, count)` ranges map 1:1 to draw calls.
            pass.set_pipeline(&self.veg_pipeline.pipeline);
            pass.set_bind_group(0, &self.veg_pipeline.bind_group, &[]);
            for chunk in self.chunks.values() {
                let Some(inst_buf) = &chunk.veg_instance_vb else {
                    continue;
                };
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                for &(species_id, start, count) in &chunk.veg_ranges {
                    let Some(mesh) = self.veg_pipeline.species_meshes.get(species_id as usize)
                    else {
                        continue;
                    };
                    pass.set_vertex_buffer(0, mesh.vb.slice(..));
                    pass.set_index_buffer(mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, start..start + count);
                }
            }
        }
    }
}
