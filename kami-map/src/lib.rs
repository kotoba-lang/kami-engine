//! kami-map: WebGPU map renderer for maps.gftd.ai.
//!
//! Replaces MapLibre GL JS with KAMI Engine wgpu rendering.
//! Entry point: `KamiMap::create(canvas_id, options_json)`.

use kami_atmosphere::Weather;
use kami_geo::projection::{self, LngLat, TileCoord, WorldPx};
use kami_geo::tile::TileManager;
use kami_render::camera::{Camera, CameraUniform, LightUniform, MaterialUniform};
use kami_render::pipeline;
use kami_render::texture;
use sgp4::chrono::{DateTime, Utc};

use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

mod input;
mod mvt;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MapOptions {
    center: [f64; 2],
    zoom: f64,
    #[serde(default = "default_tile_url", alias = "tile_url")]
    tile_url: String,
    #[serde(default = "default_dem_tile_url", alias = "dem_tile_url")]
    dem_tile_url: String,
    #[serde(default, alias = "orbital_systems")]
    orbital_systems: Vec<OrbitalSystemConfig>,
    #[serde(default, alias = "orbital_bodies")]
    orbital_bodies: Vec<OrbitalBodyConfig>,
    #[serde(default, alias = "celestial_catalogs")]
    celestial_catalogs: Vec<CelestialCatalogConfig>,
    #[serde(default, alias = "celestial_objects")]
    celestial_objects: Vec<CelestialObjectConfig>,
    #[serde(default)]
    bearing: f64,
    #[serde(default)]
    pitch: f64,
}

fn default_tile_url() -> String {
    "https://tile.openstreetmap.org/{z}/{x}/{y}.png".into()
}

fn default_dem_tile_url() -> String {
    "https://elevation-tiles-prod.s3.amazonaws.com/terrarium/{z}/{x}/{y}.png".into()
}

#[derive(Serialize)]
struct ViewportState {
    lng: f64,
    lat: f64,
    zoom: f64,
    bearing: f64,
    pitch: f64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrbitalSystemConfig {
    system_id: String,
    #[serde(default)]
    parent_system_id: Option<String>,
    #[serde(default)]
    frame: String,
    #[serde(default)]
    primary_body_id: Option<String>,
    #[serde(default)]
    scale_kind: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrbitalBodyConfig {
    body_id: String,
    #[serde(default)]
    system_id: String,
    #[serde(default)]
    body_kind: String,
    #[serde(default)]
    parent_body_id: Option<String>,
    #[serde(default)]
    source_catalog: Option<String>,
    #[serde(default)]
    norad_id: Option<String>,
    #[serde(default)]
    tle_line1: Option<String>,
    #[serde(default)]
    tle_line2: Option<String>,
    #[serde(default)]
    semi_major_axis_m: Option<f64>,
    #[serde(default)]
    eccentricity: Option<f64>,
    #[serde(default)]
    inclination_deg: Option<f64>,
    #[serde(default)]
    orbital_period_s: Option<f64>,
    #[serde(default)]
    mean_longitude_deg: Option<f64>,
    #[serde(default)]
    render_radius_m: Option<f64>,
    #[serde(default)]
    color_hex: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CelestialCatalogConfig {
    catalog_id: String,
    #[serde(default)]
    authority: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    frame: String,
    #[serde(default)]
    coverage_kind: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CelestialObjectConfig {
    object_id: String,
    #[serde(default)]
    catalog_id: String,
    #[serde(default)]
    object_kind: String,
    #[serde(default)]
    parent_object_id: Option<String>,
    #[serde(default)]
    linked_system_id: Option<String>,
    #[serde(default)]
    linked_body_id: Option<String>,
    #[serde(default)]
    reference_frame: Option<String>,
    #[serde(default)]
    ra_deg: Option<f64>,
    #[serde(default)]
    dec_deg: Option<f64>,
    #[serde(default)]
    distance_au: Option<f64>,
    #[serde(default)]
    distance_ly: Option<f64>,
    #[serde(default)]
    radius_m: Option<f64>,
    #[serde(default)]
    mass_kg: Option<f64>,
    #[serde(default)]
    spectral_class: Option<String>,
    #[serde(default)]
    render_priority: Option<i64>,
}

/// A GPU-ready tile: textured quad positioned at tile world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectionMode {
    Flat,
    Globe,
    Cosmic,
}

enum TileGeometry {
    Flat { transform: Mat4 },
    Globe {
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
        index_count: u32,
    },
}

struct GpuTile {
    material_bind_group: wgpu::BindGroup,
    geometry: TileGeometry,
}

#[derive(Clone)]
struct DemTile {
    heights_m: Vec<f32>,
    width: u32,
    height: u32,
}

/// Kind of overlay source data. Mesh is regenerated on reproject when
/// source coordinates + zoom diverge from the camera.
#[derive(Clone, Debug)]
enum LayerSource {
    /// GeoJSON-style LineString / MultiLineString.
    Lines {
        lines: Vec<Vec<[f64; 2]>>,
        width: f32,
    },
    /// GeoJSON-style Polygon / MultiPolygon (outer rings only; no holes yet).
    Fill {
        rings: Vec<Vec<[f64; 2]>>,
    },
    /// Point set rendered as world-space discs.
    Circles {
        points: Vec<[f64; 2]>,
        radius_world_px: f32,
        segments: u32,
    },
    /// 3D extruded polygons (building footprints → roof + walls).
    /// `heights[i]` is the world-space height for `rings[i]`.
    Extrude {
        rings: Vec<Vec<[f64; 2]>>,
        heights: Vec<f32>,
        base: f32,
    },
}

/// A GPU-ready overlay layer (route, GeoJSON, etc.).
struct GpuLayer {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    material_bind_group: wgpu::BindGroup,
    /// Source + state for reprojection when camera zoom/center changes.
    source: Option<LayerSource>,
    color: [f32; 4],
    visible: bool,
    min_zoom: f64,
    max_zoom: f64,
}

struct TransientMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    material_bind_group: wgpu::BindGroup,
}

struct CachedTle {
    line1: String,
    line2: String,
    elements: sgp4::Elements,
    constants: sgp4::Constants,
}

// ── Main Map Engine ─────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct KamiMap {
    // GPU resources
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    #[allow(dead_code)]
    format: wgpu::TextureFormat,

    // Render pipeline resources
    pbr_pipeline: wgpu::RenderPipeline,
    atmosphere_pipeline: wgpu::RenderPipeline,
    camera_light_bg: wgpu::BindGroup,
    shadow_bg: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    depth_view: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    light_buffer: wgpu::Buffer,

    // Fallback textures for untextured materials
    fallback_white: texture::GpuTexture,
    fallback_normal: texture::GpuTexture,
    fallback_mr: texture::GpuTexture,

    // Shared tile quad mesh (all tiles reuse this geometry)
    tile_quad_vb: wgpu::Buffer,
    tile_quad_ib: wgpu::Buffer,
    tile_quad_index_count: u32,

    // Camera / viewport state
    camera: Camera,
    center: LngLat,
    zoom: f64,
    bearing: f64,
    pitch: f64,
    projection_mode: ProjectionMode,
    width: u32,
    height: u32,

    // Tile management
    tile_manager: TileManager,
    gpu_tiles: HashMap<TileCoord, GpuTile>,
    dem_tile_url: String,
    dem_tiles: HashMap<TileCoord, DemTile>,
    orbital_systems: Vec<OrbitalSystemConfig>,
    orbital_bodies: Vec<OrbitalBodyConfig>,
    celestial_catalogs: Vec<CelestialCatalogConfig>,
    celestial_objects: Vec<CelestialObjectConfig>,
    tle_cache: HashMap<String, CachedTle>,

    // Overlay layers (legacy, anonymous — used by set_route shim).
    gpu_layers: Vec<GpuLayer>,
    // Named overlay layers, MapLibre-style id → layer.
    named_layers: HashMap<String, GpuLayer>,
    // Preserve insertion order so render order matches addLayer() calls.
    layer_order: Vec<String>,

    // Animation
    fly_target: Option<FlyTarget>,
    cosmic_phase: f32,

    // Atmosphere
    weather: Weather,

    // Named-layer coordinate-space tracking.
    // Vertices are tessellated in world-px space at `layers_build_iz` floor zoom,
    // centered at `layers_build_center`. When zoom or center drifts enough that
    // the current coordinate space no longer matches, invalidate_layers() is called
    // automatically from frame() to re-tessellate at the new zoom/center.
    layers_build_iz: f64,
    layers_build_center: LngLat,
}

// Option B: expand Globe projection to cover zoom 0.2-5.5 (was 0.2-3.2).
// Covers the "world / hemisphere / continent" range where Web-Mercator
// Flat mode shows pixelated edges and high-lat distortion. Flat takes over
// above zoom 5.5 where users see metro-sized areas and Mercator distortion
// is imperceptible.
const GLOBE_ZOOM_THRESHOLD: f64 = 5.5;
const COSMIC_ZOOM_THRESHOLD: f64 = 0.2;
const GLOBE_RADIUS: f32 = 2048.0;
const MOON_ORBIT_RADIUS: f32 = GLOBE_RADIUS * 1.8;
const GEO_RING_RADIUS: f32 = GLOBE_RADIUS * 2.6;
const SOLAR_SYSTEM_RADIUS: f32 = GLOBE_RADIUS * 38.0;
const GALAXY_RADIUS: f32 = GLOBE_RADIUS * 220.0;
const UNIVERSE_RADIUS: f32 = GLOBE_RADIUS * 960.0;

struct FlyTarget {
    target_center: LngLat,
    target_zoom: f64,
    start_center: LngLat,
    start_zoom: f64,
    duration_ms: f32,
    elapsed_ms: f32,
}

// ── JS API ──────────────────────────────────────────────────────────────

#[wasm_bindgen]
impl KamiMap {
    /// Initialize the map on a canvas element.
    pub async fn create(canvas_id: &str, options_json: &str) -> Result<KamiMap, JsValue> {
        console_error_panic_hook::set_once();
        console_log::init_with_level(log::Level::Info).ok();

        let opts: MapOptions = serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("invalid options: {e}")))?;

        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or("canvas not found")?
            .dyn_into::<HtmlCanvasElement>()?;

        let width = canvas.client_width().max(1) as u32;
        let height = canvas.client_height().max(1) as u32;
        canvas.set_width(width);
        canvas.set_height(height);

        // Unified bootstrap via kami-render. Policy: Backends::BROWSER_WEBGPU | GL
        // + Limits::downlevel_webgl2_defaults (was downlevel_defaults — caused
        // silent WebGL2 fallback failure on non-WebGPU browsers).
        let target = wgpu::SurfaceTarget::Canvas(canvas.clone());
        let ctx = kami_render::RenderContext::for_web_surface(target, width, height, "kami-map")
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        web_sys::console::log_1(&format!("[kami-map] backend={:?}", ctx.backend).into());
        let device = ctx.device;
        let queue = ctx.queue;
        let surface = ctx.surface;
        let format = ctx.format;
        let config = ctx.config;

        // ── Render resources (replicates kami-web create_render_resources) ──
        let camera_uniform = CameraUniform {
            view: Mat4::IDENTITY.to_cols_array_2d(),
            projection: Mat4::IDENTITY.to_cols_array_2d(),
            position: [0.0; 3],
            _pad: 0.0,
        };
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Light from above-left. Shadow projection pushed far away so no tile
        // falls inside the uninitialized shadow map (avoids dark diamond artifact).
        let mut light = LightUniform::directional(Vec3::new(-0.3, -1.0, -0.2), Vec3::ONE, 3.0);
        // Override shadow view_proj to place shadow frustum at unreachable coords
        light.view_proj = Mat4::orthographic_rh(-1.0, 1.0, -1.0, 1.0, 0.1, 1.0).to_cols_array_2d();
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("light"),
            contents: bytemuck::bytes_of(&light),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_light_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cl-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let material_layout = pipeline::textured_material_layout(&device);

        // Shadow map (1024x1024 depth)
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"),
            size: wgpu::Extent3d {
                width: 1024,
                height: 1024,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-samp"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        let camera_light_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cl-bg"),
            layout: &camera_light_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-bg"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        // Depth buffer
        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&Default::default());

        let pbr_pipeline = pipeline::create_pbr_pipeline(
            &device,
            format,
            &camera_light_layout,
            &material_layout,
            &shadow_layout,
        );
        let atmosphere_pipeline = pipeline::create_mtoon_pipeline(
            &device,
            format,
            &camera_light_layout,
            &material_layout,
            &shadow_layout,
        );

        // Fallback textures
        let fallback_white = texture::default_white_texture(&device, &queue);
        let fallback_normal = texture::default_normal_texture(&device, &queue);
        let fallback_mr = texture::default_mr_texture(&device, &queue);

        // ── Shared tile quad geometry ──
        let tile_quad = kami_geo::mesh::tile_quad();
        let tile_quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tile-quad-vb"),
            contents: bytemuck::cast_slice(&tile_quad.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let tile_quad_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tile-quad-ib"),
            contents: bytemuck::cast_slice(&tile_quad.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut camera = Camera::new(width as f32 / height as f32);
        camera.near = 0.1;
        camera.far = 10_000_000.0;

        log::info!(
            "KamiMap created: {}x{}, zoom={}, center=[{},{}]",
            width,
            height,
            opts.zoom,
            opts.center[0],
            opts.center[1]
        );

        Ok(KamiMap {
            device,
            queue,
            surface,
            config,
            format,
            pbr_pipeline,
            atmosphere_pipeline,
            camera_light_bg,
            shadow_bg,
            material_layout,
            depth_view,
            camera_buffer,
            light_buffer,
            fallback_white,
            fallback_normal,
            fallback_mr,
            tile_quad_vb,
            tile_quad_ib,
            tile_quad_index_count: tile_quad.indices.len() as u32,
            camera,
            center: LngLat::new(opts.center[0], opts.center[1]),
            zoom: opts.zoom,
            bearing: opts.bearing,
            pitch: opts.pitch,
            projection_mode: if opts.zoom <= COSMIC_ZOOM_THRESHOLD {
                ProjectionMode::Cosmic
            } else if opts.zoom <= GLOBE_ZOOM_THRESHOLD {
                ProjectionMode::Globe
            } else {
                ProjectionMode::Flat
            },
            width,
            height,
            tile_manager: TileManager::new(opts.tile_url),
            gpu_tiles: HashMap::new(),
            dem_tile_url: opts.dem_tile_url,
            dem_tiles: HashMap::new(),
            orbital_systems: opts.orbital_systems,
            orbital_bodies: opts.orbital_bodies,
            celestial_catalogs: opts.celestial_catalogs,
            celestial_objects: opts.celestial_objects,
            tle_cache: HashMap::new(),
            gpu_layers: Vec::new(),
            named_layers: HashMap::new(),
            layer_order: Vec::new(),
            fly_target: None,
            cosmic_phase: 0.0,
            weather: Weather::default(),
            layers_build_iz: f64::NAN,
            layers_build_center: LngLat::new(opts.center[0], opts.center[1]),
        })
    }

    /// Upload a tile image (RGBA bytes) and register it for rendering.
    pub fn upload_tile(
        &mut self,
        z: u32,
        x: u32,
        y: u32,
        rgba_data: &[u8],
        img_width: u32,
        img_height: u32,
    ) {
        let coord = TileCoord { z, x, y };

        // Create GPU texture from RGBA pixels
        let gpu_tex = texture::create_texture(
            &self.device,
            &self.queue,
            rgba_data,
            img_width,
            img_height,
            "tile",
            false,
        );

        // Create UNLIT material for map tile (no PBR lighting)
        // _pad[0] > 0.5 triggers the unlit fast-path in pbr.wgsl
        let tile_unlit = self.projection_mode == ProjectionMode::Flat;
        let mat = MaterialUniform {
            albedo: [1.0, 1.0, 1.0, 1.0],
            has_albedo_tex: 1,
            roughness: if tile_unlit { 1.0 } else { 0.92 },
            metallic: 0.0,
            has_normal_tex: 0,
            _pad: if tile_unlit { 1.0 } else { 0.0 },
            ..Default::default()
        };
        let mat_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&mat),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: mat_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gpu_tex.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gpu_tex.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_normal.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_mr.sampler),
                },
            ],
        });

        let geometry = match self.projection_mode {
            ProjectionMode::Flat => {
                // Tiles are positioned relative to the integer-zoom center.
                let center_px = projection::lng_lat_to_world_px(self.center, self.zoom.floor());
                let tile_origin = coord.origin_px();
                let tx = (tile_origin.x - center_px.x) as f32;
                let tz = (tile_origin.y - center_px.y) as f32;
                TileGeometry::Flat {
                    transform: Mat4::from_translation(Vec3::new(tx, 0.0, tz)),
                }
            }
            ProjectionMode::Globe | ProjectionMode::Cosmic => {
                let segs = if coord.z <= 1 {
                    24
                } else if coord.z == 2 {
                    18
                } else {
                    14
                };
                let mesh = self.build_globe_tile_mesh(coord, segs);
                let vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("tile-globe-vb"),
                            contents: bytemuck::cast_slice(&mesh.vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                let index_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("tile-globe-ib"),
                            contents: bytemuck::cast_slice(&mesh.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });
                TileGeometry::Globe {
                    vertex_buffer,
                    index_buffer,
                    index_count: mesh.indices.len() as u32,
                }
            }
        };

        self.tile_manager.mark_ready(coord, 0, 0);
        self.gpu_tiles.insert(
            coord,
            GpuTile {
                material_bind_group: bind_group,
                geometry,
            },
        );
        log::info!(
            "upload_tile {}/{}/{} — total gpu_tiles={}",
            z,
            x,
            y,
            self.gpu_tiles.len()
        );
    }

    pub fn upload_dem_tile(
        &mut self,
        z: u32,
        x: u32,
        y: u32,
        heights_m: &[f32],
        width: u32,
        height: u32,
    ) {
        let coord = TileCoord { z, x, y };
        if width == 0 || height == 0 || heights_m.len() < (width * height) as usize {
            return;
        }
        self.dem_tiles.insert(
            coord,
            DemTile {
                heights_m: heights_m.to_vec(),
                width,
                height,
            },
        );
        if matches!(self.projection_mode, ProjectionMode::Globe | ProjectionMode::Cosmic) {
            let segs = if coord.z <= 1 {
                24
            } else if coord.z == 2 {
                18
            } else {
                14
            };
            let mesh = self.build_globe_tile_mesh(coord, segs);
            if let Some(gpu_tile) = self.gpu_tiles.get_mut(&coord) {
                let vertex_buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("tile-dem-vb"),
                        contents: bytemuck::cast_slice(&mesh.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let index_buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("tile-dem-ib"),
                        contents: bytemuck::cast_slice(&mesh.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                gpu_tile.geometry = TileGeometry::Globe {
                    vertex_buffer,
                    index_buffer,
                    index_count: mesh.indices.len() as u32,
                };
            }
        }
    }

    pub fn get_dem_tile_url(&self, z: u32, x: u32, y: u32) -> String {
        self.dem_tile_url
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string())
    }

    /// Get tile URLs that need fetching for the current viewport.
    /// Returns JSON array of {z, x, y, url} objects.
    pub fn tiles_to_fetch(&mut self) -> String {
        self.tile_manager.begin_frame();
        let visible = self.visible_tiles_for_current_projection();
        let to_fetch = self.tile_manager.tiles_to_fetch(&visible);
        let results: Vec<serde_json::Value> = to_fetch
            .iter()
            .map(|tc| {
                serde_json::json!({
                    "z": tc.z, "x": tc.x, "y": tc.y,
                    "url": tc.url(&self.tile_manager.tile_url_template),
                })
            })
            .collect();
        serde_json::to_string(&results).unwrap_or_else(|_| "[]".into())
    }

    /// Render one frame. Call from requestAnimationFrame.
    pub fn frame(&mut self, dt_ms: f32) {
        // Advance fly animation
        if let Some(ref mut fly) = self.fly_target {
            fly.elapsed_ms += dt_ms;
            let t = (fly.elapsed_ms / fly.duration_ms).min(1.0);
            let ease = t * t * (3.0 - 2.0 * t);
            self.center = LngLat::new(
                fly.start_center.lng + (fly.target_center.lng - fly.start_center.lng) * ease as f64,
                fly.start_center.lat + (fly.target_center.lat - fly.start_center.lat) * ease as f64,
            );
            self.zoom = fly.start_zoom + (fly.target_zoom - fly.start_zoom) * ease as f64;
            if t >= 1.0 {
                self.fly_target = None;
            }
        }
        self.center.lat = projection::clamp_lat(self.center.lat);
        self.zoom = self.zoom.clamp(-1.5, 22.0);
        self.cosmic_phase += dt_ms * 0.001;
        self.weather.tick(dt_ms * 0.001);
        self.sync_projection_mode();
        self.update_camera_uniform();
        self.update_flat_tile_transforms();
        // Re-tessellate named layers when the coordinate space has drifted.
        // Vertices are baked relative to the world-px origin at build time; a
        // zoom level change or large pan shifts that origin enough to misalign
        // named-layer geometry against the tile grid.
        {
            let iz = self.zoom.floor();
            let cpx = projection::lng_lat_to_world_px(self.center, iz);
            let bpx = projection::lng_lat_to_world_px(self.layers_build_center, iz);
            let dx = cpx.x - bpx.x;
            let dy = cpx.y - bpx.y;
            if iz != self.layers_build_iz || dx * dx + dy * dy > 256.0 * 256.0 {
                self.invalidate_layers();
            }
        }

        // Pre-allocate instance buffers BEFORE the render pass.
        let tile_inst_bufs: Vec<Option<wgpu::Buffer>> = self
            .gpu_tiles
            .values()
            .map(|gpu_tile| match &gpu_tile.geometry {
                TileGeometry::Flat { transform } => Some(
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("tile-instance"),
                            contents: bytemuck::cast_slice(&transform.to_cols_array()),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                ),
                TileGeometry::Globe { .. } => None,
            })
            .collect();

        let identity_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let cosmic_meshes = if self.projection_mode == ProjectionMode::Cosmic {
            self.build_cosmic_meshes()
        } else {
            Vec::new()
        };
        let atmosphere_meshes = if self.projection_mode != ProjectionMode::Flat {
            self.build_atmosphere_meshes()
        } else {
            Vec::new()
        };

        // Render
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("map-pbr"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.12,
                            g: 0.14,
                            b: 0.18,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_pipeline(&self.pbr_pipeline);
            pass.set_bind_group(0, &self.camera_light_bg, &[]);
            pass.set_bind_group(2, &self.shadow_bg, &[]);

            // Draw each tile
            for (gpu_tile, inst_buf) in self.gpu_tiles.values().zip(tile_inst_bufs.iter()) {
                pass.set_bind_group(1, &gpu_tile.material_bind_group, &[]);
                match (&gpu_tile.geometry, inst_buf) {
                    (TileGeometry::Flat { .. }, Some(inst_buf)) => {
                        pass.set_vertex_buffer(0, self.tile_quad_vb.slice(..));
                        pass.set_vertex_buffer(1, inst_buf.slice(..));
                        pass.set_index_buffer(self.tile_quad_ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..self.tile_quad_index_count, 0, 0..1);
                    }
                    (
                        TileGeometry::Globe {
                            vertex_buffer,
                            index_buffer,
                            index_count,
                        },
                        None,
                    ) => {
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, identity_buf.slice(..));
                        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..*index_count, 0, 0..1);
                    }
                    _ => {}
                }
            }

            for layer in &self.gpu_layers {
                pass.set_bind_group(1, &layer.material_bind_group, &[]);
                pass.set_vertex_buffer(0, layer.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, identity_buf.slice(..));
                pass.set_index_buffer(layer.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..layer.index_count, 0, 0..1);
            }
            for id in &self.layer_order {
                let Some(layer) = self.named_layers.get(id) else { continue };
                if !layer.visible {
                    continue;
                }
                if self.zoom < layer.min_zoom || self.zoom > layer.max_zoom {
                    continue;
                }
                if layer.index_count == 0 {
                    continue;
                }
                pass.set_bind_group(1, &layer.material_bind_group, &[]);
                pass.set_vertex_buffer(0, layer.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, identity_buf.slice(..));
                pass.set_index_buffer(layer.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..layer.index_count, 0, 0..1);
            }
            for mesh in &cosmic_meshes {
                pass.set_bind_group(1, &mesh.material_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, identity_buf.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
            if !atmosphere_meshes.is_empty() {
                pass.set_pipeline(&self.atmosphere_pipeline);
                pass.set_bind_group(0, &self.camera_light_bg, &[]);
                pass.set_bind_group(2, &self.shadow_bg, &[]);
                for mesh in &atmosphere_meshes {
                    pass.set_bind_group(1, &mesh.material_bind_group, &[]);
                    pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, identity_buf.slice(..));
                    pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
                pass.set_pipeline(&self.pbr_pipeline);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }

    /// Add a GeoJSON route (line) layer.
    pub fn set_route(&mut self, coords_json: &str, color_hex: &str, width: f32) {
        // Parse [[lng, lat], ...] coords
        let coords: Vec<[f64; 2]> = serde_json::from_str(coords_json).unwrap_or_default();
        if coords.len() < 2 {
            return;
        }

        let geo_mesh = if self.projection_mode == ProjectionMode::Globe {
            kami_geo::mesh::globe_line_to_ribbon(
                &coords,
                GLOBE_RADIUS,
                self.globe_overlay_width(width),
                6.0,
            )
        } else {
            let center_px = projection::lng_lat_to_world_px(self.center, self.zoom);
            kami_geo::mesh::line_to_ribbon(&coords, self.zoom, center_px, width, 0.1)
        };

        let color = parse_hex_color(color_hex);
        self.add_layer_from_mesh(&geo_mesh.vertices, &geo_mesh.indices, color);
    }

    /// Clear all overlay layers.
    pub fn clear_layers(&mut self) {
        self.gpu_layers.clear();
    }

    // ── Viewport controls ──

    pub fn set_center(&mut self, lng: f64, lat: f64) {
        self.center = LngLat::new(lng, projection::clamp_lat(lat));
    }
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom.clamp(-1.5, 22.0);
    }
    pub fn set_bearing(&mut self, degrees: f64) {
        self.bearing = degrees.to_radians();
    }
    pub fn set_pitch(&mut self, degrees: f64) {
        self.pitch = degrees.to_radians().clamp(0.0, 1.48);
    }

    pub fn fly_to(&mut self, lng: f64, lat: f64, zoom: f64, duration_ms: u32) {
        self.fly_target = Some(FlyTarget {
            target_center: LngLat::new(lng, lat),
            target_zoom: zoom,
            start_center: self.center,
            start_zoom: self.zoom,
            duration_ms: duration_ms as f32,
            elapsed_ms: 0.0,
        });
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.camera.aspect = width as f32 / height as f32;
        // Recreate depth buffer
        let depth_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_view = depth_tex.create_view(&Default::default());
    }

    pub fn get_viewport(&self) -> String {
        serde_json::to_string(&ViewportState {
            lng: self.center.lng,
            lat: self.center.lat,
            zoom: self.zoom,
            bearing: self.bearing.to_degrees(),
            pitch: self.pitch.to_degrees(),
        })
        .unwrap_or_default()
    }

    pub fn get_zoom(&self) -> f64 {
        self.zoom
    }

    // ── Input handlers ──

    pub fn on_pointer_down(&mut self, x: f32, y: f32, button: u32) {
        input::on_pointer_down(self, x, y, button);
    }
    pub fn on_pointer_move(&mut self, _x: f32, _y: f32, dx: f32, dy: f32) {
        input::on_pointer_move(self, dx, dy);
    }
    pub fn on_pointer_up(&mut self, _x: f32, _y: f32) {
        input::on_pointer_up(self);
    }
    pub fn on_wheel(&mut self, delta: f64) {
        self.zoom = (self.zoom - delta * 0.003).clamp(-1.5, 22.0);
    }

    pub fn unproject(&self, screen_x: f32, screen_y: f32) -> String {
        if self.projection_mode == ProjectionMode::Globe {
            let ll = self
                .globe_unproject(screen_x, screen_y)
                .unwrap_or(self.center);
            return serde_json::to_string(&[ll.lng, ll.lat]).unwrap_or_default();
        }
        let center_px = projection::lng_lat_to_world_px(self.center, self.zoom);
        let dx = (screen_x - self.width as f32 / 2.0) as f64;
        let dy = (screen_y - self.height as f32 / 2.0) as f64;
        let wp = WorldPx {
            x: center_px.x + dx,
            y: center_px.y + dy,
        };
        let ll = projection::world_px_to_lng_lat(wp, self.zoom);
        serde_json::to_string(&[ll.lng, ll.lat]).unwrap_or_default()
    }

    /// Project a geographic coordinate to screen pixels. Returns JSON `[x, y]`.
    pub fn project(&self, lng: f64, lat: f64) -> String {
        if self.projection_mode == ProjectionMode::Globe {
            if let Some((x, y)) = self.globe_project(lng, lat) {
                return serde_json::to_string(&[x, y]).unwrap_or_default();
            }
            return serde_json::to_string(&[-1.0_f32, -1.0_f32]).unwrap_or_default();
        }
        let center_px = projection::lng_lat_to_world_px(self.center, self.zoom);
        let wp = projection::lng_lat_to_world_px(LngLat::new(lng, lat), self.zoom);
        let x = (wp.x - center_px.x) as f32 + self.width as f32 * 0.5;
        let y = (wp.y - center_px.y) as f32 + self.height as f32 * 0.5;
        serde_json::to_string(&[x, y]).unwrap_or_default()
    }

    /// Adjust center + zoom so the bounding box fits the viewport with optional padding px.
    pub fn fit_bounds(
        &mut self,
        min_lng: f64,
        min_lat: f64,
        max_lng: f64,
        max_lat: f64,
        padding_px: f32,
    ) {
        let center_lng = (min_lng + max_lng) * 0.5;
        let center_lat = (min_lat + max_lat) * 0.5;
        let usable_w = (self.width as f32 - padding_px * 2.0).max(16.0) as f64;
        let usable_h = (self.height as f32 - padding_px * 2.0).max(16.0) as f64;
        // Binary search zoom.
        let mut lo = -1.5_f64;
        let mut hi = 22.0_f64;
        for _ in 0..32 {
            let mid = (lo + hi) * 0.5;
            let ne = projection::lng_lat_to_world_px(LngLat::new(max_lng, min_lat), mid);
            let sw = projection::lng_lat_to_world_px(LngLat::new(min_lng, max_lat), mid);
            let w = (ne.x - sw.x).abs();
            let h = (sw.y - ne.y).abs();
            if w <= usable_w && h <= usable_h {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.center = LngLat::new(center_lng, center_lat);
        self.zoom = lo.clamp(-1.5, 22.0);
        self.invalidate_layers();
    }

    /// Add or replace a named line layer. `lines_json` is a JSON array of polylines
    /// where each polyline is an array of `[lng, lat]`.
    pub fn add_line_layer(
        &mut self,
        id: &str,
        lines_json: &str,
        color_hex: &str,
        width: f32,
    ) -> Result<(), JsValue> {
        let lines: Vec<Vec<[f64; 2]>> =
            serde_json::from_str(lines_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let color = parse_hex_color(color_hex);
        let source = LayerSource::Lines {
            lines,
            width: width.max(0.5),
        };
        let (verts, idx) = self.rebuild_mesh_for(&source);
        if verts.is_empty() {
            self.named_layers.remove(id);
            self.layer_order.retain(|x| x != id);
            return Ok(());
        }
        let layer = self.build_layer(&verts, &idx, color, Some(source));
        self.upsert_named(id.to_string(), layer);
        Ok(())
    }

    /// Add or replace a named polygon fill layer. `rings_json` is a JSON array of
    /// rings (outer only for now), each an array of `[lng, lat]`.
    pub fn add_fill_layer(
        &mut self,
        id: &str,
        rings_json: &str,
        color_hex: &str,
        opacity: f32,
    ) -> Result<(), JsValue> {
        let rings: Vec<Vec<[f64; 2]>> =
            serde_json::from_str(rings_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut color = parse_hex_color(color_hex);
        color[3] = opacity.clamp(0.0, 1.0);
        let source = LayerSource::Fill { rings };
        let (verts, idx) = self.rebuild_mesh_for(&source);
        if verts.is_empty() {
            self.named_layers.remove(id);
            self.layer_order.retain(|x| x != id);
            return Ok(());
        }
        let layer = self.build_layer(&verts, &idx, color, Some(source));
        self.upsert_named(id.to_string(), layer);
        Ok(())
    }

    /// Add or replace a named circle (point) layer. `points_json` is a JSON array
    /// of `[lng, lat]`. `radius` is in world pixels at current zoom.
    pub fn add_circle_layer(
        &mut self,
        id: &str,
        points_json: &str,
        color_hex: &str,
        radius: f32,
    ) -> Result<(), JsValue> {
        let points: Vec<[f64; 2]> =
            serde_json::from_str(points_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let color = parse_hex_color(color_hex);
        let source = LayerSource::Circles {
            points,
            radius_world_px: radius.max(1.0),
            segments: 16,
        };
        let (verts, idx) = self.rebuild_mesh_for(&source);
        if verts.is_empty() {
            self.named_layers.remove(id);
            self.layer_order.retain(|x| x != id);
            return Ok(());
        }
        let layer = self.build_layer(&verts, &idx, color, Some(source));
        self.upsert_named(id.to_string(), layer);
        Ok(())
    }

    /// Add or replace a named 3D extrusion layer (building footprints → roof + walls).
    ///
    /// `rings_json`  : `[[[lng,lat], ...], ...]` polygon outer rings
    /// `heights_json`: `[h, ...]` world-space extrusion height per polygon
    /// `color_hex`   : `#rrggbb`
    /// `opacity`     : 0..1
    ///
    /// Extrusions render only when the camera is tilted (pitch > 0). In pure
    /// top-down orthographic view they appear as roof polygons only.
    pub fn add_extrude_layer(
        &mut self,
        id: &str,
        rings_json: &str,
        heights_json: &str,
        color_hex: &str,
        opacity: f32,
    ) -> Result<(), JsValue> {
        let rings: Vec<Vec<[f64; 2]>> =
            serde_json::from_str(rings_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let heights: Vec<f32> =
            serde_json::from_str(heights_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let mut color = parse_hex_color(color_hex);
        color[3] = opacity.clamp(0.0, 1.0);
        let source = LayerSource::Extrude {
            rings,
            heights,
            base: 0.0,
        };
        let (verts, idx) = self.rebuild_mesh_for(&source);
        if verts.is_empty() {
            self.named_layers.remove(id);
            self.layer_order.retain(|x| x != id);
            return Ok(());
        }
        let layer = self.build_layer(&verts, &idx, color, Some(source));
        self.upsert_named(id.to_string(), layer);
        Ok(())
    }

    /// Remove a named layer.
    pub fn remove_layer(&mut self, id: &str) {
        self.named_layers.remove(id);
        self.layer_order.retain(|x| x != id);
    }

    /// Show/hide a named layer.
    pub fn set_layer_visibility(&mut self, id: &str, visible: bool) {
        if let Some(l) = self.named_layers.get_mut(id) {
            l.visible = visible;
        }
    }

    /// Restrict the zoom range over which a layer is drawn.
    pub fn set_layer_zoom_range(&mut self, id: &str, min_zoom: f64, max_zoom: f64) {
        if let Some(l) = self.named_layers.get_mut(id) {
            l.min_zoom = min_zoom;
            l.max_zoom = max_zoom;
        }
    }

    /// Returns true if a named layer exists.
    pub fn has_layer(&self, id: &str) -> bool {
        self.named_layers.contains_key(id)
    }

    /// List named layer ids (JSON array).
    pub fn list_layers(&self) -> String {
        serde_json::to_string(&self.layer_order).unwrap_or_else(|_| "[]".into())
    }

    /// Decode an MVT (Mapbox Vector Tile) PBF blob and return the named layer's
    /// geometry as JSON `{ lines: [...], polygons: [...], points: [...] }` in
    /// geographic coordinates. The bridge accumulates this across visible tiles
    /// and feeds it back into add_line_layer / add_fill_layer / add_circle_layer.
    pub fn decode_mvt_layer(
        &self,
        z: u32,
        x: u32,
        y: u32,
        layer_name: &str,
        pbf: &[u8],
    ) -> String {
        let features = mvt::decode_layer(pbf, kami_geo::projection::TileCoord { z, x, y }, layer_name);
        serde_json::json!({
            "lines": features.lines,
            "polygons": features.polygons,
            "points": features.points,
        })
        .to_string()
    }

    /// Decode an MVT (Mapbox Vector Tile) PBF blob and return the named layer's
    /// features as GeoJSON-like `{ features: [{ geometry, properties }] }`.
    pub fn decode_mvt_layer_features(
        &self,
        z: u32,
        x: u32,
        y: u32,
        layer_name: &str,
        pbf: &[u8],
    ) -> String {
        let features =
            mvt::decode_layer_features(pbf, kami_geo::projection::TileCoord { z, x, y }, layer_name);
        serde_json::to_string(&features).unwrap_or_else(|_| "{\"features\":[]}".into())
    }

    /// Invalidate cached meshes so named layers regenerate at the new zoom/center.
    pub fn invalidate_layers(&mut self) {
        // Collect ids + sources first to avoid borrow conflicts.
        let jobs: Vec<(String, LayerSource, [f32; 4])> = self
            .named_layers
            .iter()
            .filter_map(|(id, l)| {
                l.source
                    .as_ref()
                    .map(|s| (id.clone(), s.clone(), l.color))
            })
            .collect();
        for (id, src, color) in jobs {
            let (verts, idx) = self.rebuild_mesh_for(&src);
            if verts.is_empty() {
                self.named_layers.remove(&id);
                self.layer_order.retain(|x| x != &id);
                continue;
            }
            let new_layer = self.build_layer(&verts, &idx, color, Some(src));
            if let Some(existing) = self.named_layers.get_mut(&id) {
                let prev_visible = existing.visible;
                let prev_min = existing.min_zoom;
                let prev_max = existing.max_zoom;
                *existing = GpuLayer {
                    visible: prev_visible,
                    min_zoom: prev_min,
                    max_zoom: prev_max,
                    ..new_layer
                };
            }
        }
        // Record the zoom/center at which tessellation was performed so frame()
        // can detect coordinate-space drift and re-tessellate as needed.
        self.layers_build_iz = self.zoom.floor();
        self.layers_build_center = self.center;
    }

    /// Set the time-of-day for the atmosphere. `t` in [0, 1) where 0.0 = midnight, 0.5 = noon.
    pub fn set_time_of_day(&mut self, t: f32) {
        self.weather.day_night.time = t.fract().abs();
    }

    /// Switch to a named weather preset: "overcast" or "clear". Unknown names are ignored.
    pub fn set_weather_preset(&mut self, preset: &str) {
        self.weather = match preset {
            "overcast" => Weather::overcast(),
            "clear" => Weather::clear(),
            _ => return,
        };
    }
}

// ── Private helpers ─────────────────────────────────────────────────────

impl KamiMap {
    fn cosmic_blend(&self) -> f32 {
        ((COSMIC_ZOOM_THRESHOLD - self.zoom) as f32 / 2.6).clamp(0.0, 1.0)
    }

    fn cosmic_system_blend(&self, enter_zoom: f64, span: f64) -> f32 {
        ((enter_zoom - self.zoom) as f32 / span as f32).clamp(0.0, 1.0)
    }

    fn orbital_body(&self, body_id: &str) -> Option<&OrbitalBodyConfig> {
        self.orbital_bodies.iter().find(|body| body.body_id == body_id)
    }

    fn celestial_object(&self, object_id: &str) -> Option<&CelestialObjectConfig> {
        self.celestial_objects
            .iter()
            .find(|object| object.object_id == object_id)
    }

    fn tle_scene_position(&mut self, body: &OrbitalBodyConfig, now_ms: f64) -> Option<Vec3> {
        let line1 = body.tle_line1.as_ref()?;
        let line2 = body.tle_line2.as_ref()?;
        let needs_refresh = self
            .tle_cache
            .get(&body.body_id)
            .map(|entry| entry.line1 != *line1 || entry.line2 != *line2)
            .unwrap_or(true);
        if needs_refresh {
            let elements = sgp4::Elements::from_tle(
                Some(body.body_id.clone()),
                line1.as_bytes(),
                line2.as_bytes(),
            )
            .ok()?;
            let constants = sgp4::Constants::from_elements(&elements).ok()?;
            self.tle_cache.insert(
                body.body_id.clone(),
                CachedTle {
                    line1: line1.clone(),
                    line2: line2.clone(),
                    elements,
                    constants,
                },
            );
        }
        let entry = self.tle_cache.get(&body.body_id)?;
        tle_scene_position_from_cache(entry, now_ms)
    }

    fn push_transient_mesh(
        &self,
        meshes: &mut Vec<TransientMesh>,
        verts: Vec<f32>,
        idx: Vec<u32>,
        color: [f32; 4],
    ) {
        if verts.is_empty() || idx.is_empty() {
            return;
        }
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cosmic-vb"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cosmic-ib"),
                contents: bytemuck::cast_slice(&idx),
                usage: wgpu::BufferUsages::INDEX,
            });
        meshes.push(TransientMesh {
            vertex_buffer,
            index_buffer,
            index_count: idx.len() as u32,
            material_bind_group: self.make_unlit_bind_group(color),
        });
    }

    fn active_projection_mode(&self) -> ProjectionMode {
        if self.zoom <= COSMIC_ZOOM_THRESHOLD {
            ProjectionMode::Cosmic
        } else if self.zoom <= GLOBE_ZOOM_THRESHOLD {
            ProjectionMode::Globe
        } else {
            ProjectionMode::Flat
        }
    }

    fn sync_projection_mode(&mut self) {
        let next = self.active_projection_mode();
        if next == self.projection_mode {
            return;
        }
        self.projection_mode = next;
        self.gpu_tiles.clear();
        self.tile_manager = TileManager::new(self.tile_manager.tile_url_template.clone());
        self.invalidate_layers();
    }

    fn visible_tiles_for_current_projection(&self) -> Vec<TileCoord> {
        match self.projection_mode {
            ProjectionMode::Flat => {
                projection::visible_tiles(self.center, self.zoom, self.width, self.height)
            }
            ProjectionMode::Globe => self.visible_globe_tiles(),
            ProjectionMode::Cosmic => self.visible_cosmic_tiles(),
        }
    }

    fn visible_globe_tiles(&self) -> Vec<TileCoord> {
        let detail_z = if self.zoom <= 1.2 {
            1
        } else if self.zoom <= 2.3 {
            2
        } else {
            3
        };
        let eye = self.globe_camera_position();
        let view_dir = (-eye).normalize_or_zero();
        let tile_count = 1_u32 << detail_z;
        let mut tiles = Vec::new();
        for y in 0..tile_count {
            for x in 0..tile_count {
                let coord = TileCoord { z: detail_z, x, y };
                let center = coord.center_lng_lat();
                let normal = globe_normal(center.lng, center.lat);
                let facing = normal.dot(view_dir);
                if facing > -0.22 {
                    tiles.push(coord);
                }
            }
        }
        tiles
    }

    fn visible_cosmic_tiles(&self) -> Vec<TileCoord> {
        let detail_z = if self.zoom <= -0.7 {
            1
        } else if self.zoom <= 0.4 {
            2
        } else {
            3
        };
        let tile_count = 1_u32 << detail_z;
        let mut tiles = Vec::with_capacity((tile_count * tile_count) as usize);
        for y in 0..tile_count {
            for x in 0..tile_count {
                tiles.push(TileCoord { z: detail_z, x, y });
            }
        }
        tiles
    }

    fn update_camera_uniform(&mut self) {
        let cam_uniform = match self.projection_mode {
            ProjectionMode::Flat => {
                let frac_scale = 2.0_f32.powf(self.zoom as f32 - (self.zoom as f32).floor());
                let half_w = (self.width as f32 * 0.5) / frac_scale;
                let half_h = (self.height as f32 * 0.5) / frac_scale;
                let altitude = 10_000.0_f32;

                // Apply pitch + bearing so 3D extrusions are visible. Pitch tilts
                // the camera backward around the map center; bearing rotates
                // around the vertical axis. With pitch = 0, view matches the
                // previous pure top-down orthographic projection.
                let pitch = self.pitch as f32;
                let bearing = self.bearing as f32;
                let base_eye = Vec3::new(0.0, altitude, 0.0);
                let tilt = Mat4::from_rotation_x(-pitch);
                let yaw = Mat4::from_rotation_y(bearing);
                let eye4 = yaw * tilt * base_eye.extend(1.0);
                let up4 = yaw * tilt * Vec3::new(0.0, 0.0, -1.0).extend(0.0);
                self.camera.position = Vec3::new(eye4.x, eye4.y, eye4.z);
                self.camera.target = Vec3::ZERO;
                self.camera.up = Vec3::new(up4.x, up4.y, up4.z).normalize_or_zero();

                let view = Mat4::look_at_rh(self.camera.position, self.camera.target, self.camera.up);
                let projection =
                    // Far plane 1e9 (was 100_000) so wide-extent polygons
                    // at high zoom with pitch aren't depth-clipped. At
                    // zoom 12 Tokyo with pitch 45°, a country-scale polygon
                    // at world-px 35K has view-space z ~30K — fine for 1e9.
                    // 100K was the previous bound and cut off anything
                    // larger than ~70K viewport span with pitch.
                    Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, 0.1, 1.0e9);
                CameraUniform {
                    view: view.to_cols_array_2d(),
                    projection: projection.to_cols_array_2d(),
                    position: self.camera.position.to_array(),
                    _pad: 0.0,
                }
            }
            ProjectionMode::Globe => {
                let target = globe_position(self.center.lng, self.center.lat, GLOBE_RADIUS);
                let normal = target.normalize_or_zero();
                let distance = GLOBE_RADIUS * (1.05 + (4.2 - self.zoom as f32).max(0.0) * 0.45);
                let eye = normal * distance;
                let east_seed = if normal.y.abs() > 0.98 { Vec3::Z } else { Vec3::Y };
                let east = east_seed.cross(normal).normalize_or_zero();
                let north = normal.cross(east).normalize_or_zero();

                self.camera.position = eye;
                self.camera.target = target;
                self.camera.up = north;

                let view = Mat4::look_at_rh(self.camera.position, self.camera.target, self.camera.up);
                let projection = Mat4::perspective_rh(
                    34.0_f32.to_radians(),
                    self.width as f32 / self.height.max(1) as f32,
                    1.0,
                    distance + GLOBE_RADIUS * 2.4,
                );
                CameraUniform {
                    view: view.to_cols_array_2d(),
                    projection: projection.to_cols_array_2d(),
                    position: self.camera.position.to_array(),
                    _pad: 0.0,
                }
            }
            ProjectionMode::Cosmic => {
                let target = Vec3::ZERO;
                let yaw = self.cosmic_phase * 0.08 + self.center.lng.to_radians() as f32 * 0.15;
                let pitch = 0.52 + self.center.lat.to_radians() as f32 * 0.06;
                let cosmic_blend = self.cosmic_blend();
                let solar_blend = self.cosmic_system_blend(-1.2, 1.8);
                let galaxy_blend = self.cosmic_system_blend(-3.8, 2.6);
                let universe_blend = self.cosmic_system_blend(-6.4, 3.6);
                let distance = GLOBE_RADIUS * (
                    5.6
                        + cosmic_blend * 18.0
                        + solar_blend * 54.0
                        + galaxy_blend * 240.0
                        + universe_blend * 740.0
                );
                let eye = Vec3::new(
                    distance * pitch.cos() * yaw.sin(),
                    distance * pitch.sin(),
                    distance * pitch.cos() * yaw.cos(),
                );
                self.camera.position = eye;
                self.camera.target = target;
                self.camera.up = Vec3::Y;
                let view = Mat4::look_at_rh(self.camera.position, self.camera.target, self.camera.up);
                let projection = Mat4::perspective_rh(
                    38.0_f32.to_radians(),
                    self.width as f32 / self.height.max(1) as f32,
                    1.0,
                    distance + UNIVERSE_RADIUS * 1.35,
                );
                CameraUniform {
                    view: view.to_cols_array_2d(),
                    projection: projection.to_cols_array_2d(),
                    position: self.camera.position.to_array(),
                    _pad: 0.0,
                }
            }
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&cam_uniform));
    }

    fn update_flat_tile_transforms(&mut self) {
        if self.projection_mode != ProjectionMode::Flat {
            return;
        }
        let iz = self.zoom.floor();
        let center_px = projection::lng_lat_to_world_px(self.center, iz);
        for (coord, gpu_tile) in self.gpu_tiles.iter_mut() {
            let TileGeometry::Flat { transform } = &mut gpu_tile.geometry else {
                continue;
            };
            if coord.z != iz as u32 {
                *transform = Mat4::from_translation(Vec3::new(1e9, 0.0, 1e9));
                continue;
            }
            let tile_origin = coord.origin_px();
            let tx = (tile_origin.x - center_px.x) as f32;
            let tz = (tile_origin.y - center_px.y) as f32;
            *transform = Mat4::from_translation(Vec3::new(tx, 0.0, tz));
        }
    }

    fn globe_camera_position(&self) -> Vec3 {
        match self.projection_mode {
            ProjectionMode::Globe => {
                let target = globe_position(self.center.lng, self.center.lat, GLOBE_RADIUS);
                let normal = target.normalize_or_zero();
                let distance = GLOBE_RADIUS * (1.05 + (4.2 - self.zoom as f32).max(0.0) * 0.45);
                normal * distance
            }
            ProjectionMode::Cosmic => self.camera.position,
            ProjectionMode::Flat => Vec3::new(0.0, 10_000.0, 0.0),
        }
    }

    fn globe_overlay_width(&self, width: f32) -> f32 {
        width.max(0.5) * (7.5 - self.zoom as f32).clamp(3.0, 7.0)
    }

    fn globe_overlay_radius(&self, radius: f32) -> f32 {
        radius.max(1.0) * (8.5 - self.zoom as f32).clamp(3.5, 8.0)
    }

    fn globe_project(&self, lng: f64, lat: f64) -> Option<(f32, f32)> {
        let point = globe_position(lng, lat, GLOBE_RADIUS);
        let (view, proj, eye) = self.sphere_view_projection();
        let clip = proj * view * Vec4::new(point.x, point.y, point.z, 1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        let surface_normal = point.normalize_or_zero();
        let camera_dir = (eye - point).normalize_or_zero();
        if surface_normal.dot(camera_dir) <= 0.0 {
            return None;
        }
        let x = (ndc.x * 0.5 + 0.5) * self.width as f32;
        let y = (1.0 - (ndc.y * 0.5 + 0.5)) * self.height as f32;
        Some((x, y))
    }

    fn globe_unproject(&self, screen_x: f32, screen_y: f32) -> Option<LngLat> {
        let (view, proj, _eye) = self.sphere_view_projection();
        let inv = (proj * view).inverse();
        let x = (screen_x / self.width.max(1) as f32) * 2.0 - 1.0;
        let y = 1.0 - (screen_y / self.height.max(1) as f32) * 2.0;
        let near = inv * Vec4::new(x, y, -1.0, 1.0);
        let far = inv * Vec4::new(x, y, 1.0, 1.0);
        let near = near.truncate() / near.w;
        let far = far.truncate() / far.w;
        let dir = (far - near).normalize_or_zero();
        ray_sphere_hit(near, dir, GLOBE_RADIUS).map(|hit| globe_lng_lat_from_position(hit))
    }

    fn sphere_view_projection(&self) -> (Mat4, Mat4, Vec3) {
        match self.projection_mode {
            ProjectionMode::Globe => {
                let target = globe_position(self.center.lng, self.center.lat, GLOBE_RADIUS);
                let normal = target.normalize_or_zero();
                let eye = self.globe_camera_position();
                let east_seed = if normal.y.abs() > 0.98 { Vec3::Z } else { Vec3::Y };
                let east = east_seed.cross(normal).normalize_or_zero();
                let north = normal.cross(east).normalize_or_zero();
                let view = Mat4::look_at_rh(eye, target, north);
                let proj = Mat4::perspective_rh(
                    34.0_f32.to_radians(),
                    self.width as f32 / self.height.max(1) as f32,
                    1.0,
                    eye.length() + GLOBE_RADIUS * 2.4,
                );
                (view, proj, eye)
            }
            ProjectionMode::Cosmic => {
                let eye = self.camera.position;
                let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
                let proj = Mat4::perspective_rh(
                    38.0_f32.to_radians(),
                    self.width as f32 / self.height.max(1) as f32,
                    1.0,
                    eye.length() + GLOBE_RADIUS * 18.0,
                );
                (view, proj, eye)
            }
            ProjectionMode::Flat => {
                let view = Mat4::IDENTITY;
                let proj = Mat4::IDENTITY;
                (view, proj, Vec3::ZERO)
            }
        }
    }

    fn build_cosmic_meshes(&mut self) -> Vec<TransientMesh> {
        let mut meshes = Vec::new();
        let cosmic_blend = self.cosmic_blend();
        let lunar_blend = self.cosmic_system_blend(-0.4, 0.9);
        let solar_blend = self.cosmic_system_blend(-1.2, 1.8);
        let galaxy_blend = self.cosmic_system_blend(-3.8, 2.6);
        let universe_blend = self.cosmic_system_blend(-6.4, 3.6);
        let phase = self.cosmic_phase;
        let now_s = js_sys::Date::now() as f32 / 1000.0;
        let earth_pos = Vec3::ZERO;
        let solar_radius = SOLAR_SYSTEM_RADIUS * (0.28 + solar_blend * 0.72);
        let earth_body = self.orbital_body("orbital-body:earth").cloned();
        let earth_a_m = earth_body
            .as_ref()
            .and_then(|body| body.semi_major_axis_m)
            .unwrap_or(149_597_870_700.0);
        let earth_abs = orbital_scene_position(
            scene_solar_orbit_radius(earth_a_m as f32, earth_a_m as f32, solar_radius),
            earth_body.as_ref().and_then(|body| body.eccentricity).unwrap_or(0.0167) as f32,
            earth_body
                .as_ref()
                .and_then(|body| body.inclination_deg)
                .unwrap_or(0.0) as f32,
            orbital_phase_angle(
                earth_body.as_ref().and_then(|body| body.orbital_period_s).unwrap_or(31_558_149.0) as f32,
                earth_body.as_ref().and_then(|body| body.mean_longitude_deg).unwrap_or(100.0) as f32,
                now_s,
            ),
        );
        let sun_pos = -earth_abs;
        let sun_body = self.orbital_body("orbital-body:sun").cloned();
        let sun_radius = display_body_radius(
            sun_body.as_ref(),
            "orbital-body:sun",
            GLOBE_RADIUS * (0.32 + solar_blend * 0.14),
            GLOBE_RADIUS * 0.22,
            GLOBE_RADIUS * 0.54,
        );
        let moon_body = self.orbital_body("orbital-body:moon").cloned();
        let moon_pos = earth_pos
            + orbital_scene_position(
                scene_cislunar_orbit_radius(
                    moon_body.as_ref().and_then(|body| body.semi_major_axis_m).unwrap_or(384_400_000.0)
                        as f32,
                ),
                moon_body.as_ref().and_then(|body| body.eccentricity).unwrap_or(0.0549) as f32,
                moon_body.as_ref().and_then(|body| body.inclination_deg).unwrap_or(5.145) as f32,
                orbital_phase_angle(
                    moon_body.as_ref().and_then(|body| body.orbital_period_s).unwrap_or(2_360_591.0)
                        as f32,
                    moon_body.as_ref().and_then(|body| body.mean_longitude_deg).unwrap_or(218.3) as f32,
                    now_s,
                ),
            );
        let iss_body = self.orbital_body("orbital-body:iss").cloned();
        let now_ms = js_sys::Date::now();
        let iss_pos = iss_body
            .as_ref()
            .and_then(|body| self.tle_scene_position(body, now_ms))
            .unwrap_or_else(|| {
                earth_pos
                    + orbital_scene_position(
                        scene_cislunar_orbit_radius(
                            iss_body.as_ref().and_then(|body| body.semi_major_axis_m).unwrap_or(6_771_000.0)
                                as f32,
                        ),
                        iss_body.as_ref().and_then(|body| body.eccentricity).unwrap_or(0.0005) as f32,
                        iss_body.as_ref().and_then(|body| body.inclination_deg).unwrap_or(51.64) as f32,
                        orbital_phase_angle(
                            iss_body.as_ref().and_then(|body| body.orbital_period_s).unwrap_or(5_570.0) as f32,
                            iss_body.as_ref().and_then(|body| body.mean_longitude_deg).unwrap_or(0.0) as f32,
                            now_s,
                        ),
                    )
            });
        let geo_body = self.orbital_body("orbital-body:geo-ring").cloned();
        let geo_ring_radius = scene_cislunar_orbit_radius(
            geo_body.as_ref().and_then(|body| body.semi_major_axis_m).unwrap_or(42_164_000.0) as f32,
        );
        let mercury_pos = orbital_scene_position(
            scene_solar_orbit_radius(57_909_000_000.0, earth_a_m as f32, solar_radius),
            0.2056,
            7.0,
            orbital_phase_angle(7_600_543.0, 75.0, now_s),
        ) - earth_abs;
        let venus_pos = orbital_scene_position(
            scene_solar_orbit_radius(108_210_000_000.0, earth_a_m as f32, solar_radius),
            0.0068,
            3.39,
            orbital_phase_angle(19_414_149.0, 181.0, now_s),
        ) - earth_abs;
        let mars_pos = orbital_scene_position(
            scene_solar_orbit_radius(227_939_200_000.0, earth_a_m as f32, solar_radius),
            0.0934,
            1.85,
            orbital_phase_angle(59_354_032.0, 355.0, now_s),
        ) - earth_abs;
        let jupiter_pos = orbital_scene_position(
            scene_solar_orbit_radius(778_570_000_000.0, earth_a_m as f32, solar_radius),
            0.0489,
            1.31,
            orbital_phase_angle(374_335_776.0, 34.0, now_s),
        ) - earth_abs;
        let saturn_pos = orbital_scene_position(
            scene_solar_orbit_radius(1_433_530_000_000.0, earth_a_m as f32, solar_radius),
            0.0565,
            2.49,
            orbital_phase_angle(929_596_608.0, 50.0, now_s),
        ) - earth_abs;
        let galaxy_radius = self
            .celestial_object("celestial-object:milky-way")
            .and_then(|object| object.radius_m)
            .map(|_| GALAXY_RADIUS * (0.42 + galaxy_blend * 0.58))
            .unwrap_or(GALAXY_RADIUS * (0.42 + galaxy_blend * 0.58));
        let universe_radius = self
            .celestial_object("celestial-object:observable-universe")
            .and_then(|object| object.radius_m)
            .map(|_| UNIVERSE_RADIUS * (0.26 + universe_blend * 0.74))
            .unwrap_or(UNIVERSE_RADIUS * (0.26 + universe_blend * 0.74));

        let (verts, idx) = sphere_mesh_at(earth_pos, GLOBE_RADIUS * (1.02 + cosmic_blend * 0.04), 24, 32);
        self.push_transient_mesh(&mut meshes, verts, idx, [0.92, 0.98, 1.0, 0.06]);
        let (verts, idx) = sphere_mesh_at(sun_pos, sun_radius, 14, 18);
        self.push_transient_mesh(
            &mut meshes,
            verts,
            idx,
            sun_body
                .as_ref()
                .and_then(|body| body.color_hex.as_deref())
                .map(parse_hex_color)
                .unwrap_or([1.0, 0.78, 0.28, 0.96]),
        );
        let (verts, idx) = sphere_mesh_at(
            moon_pos,
            display_body_radius(
                moon_body.as_ref(),
                "orbital-body:moon",
                GLOBE_RADIUS * 0.27,
                GLOBE_RADIUS * 0.18,
                GLOBE_RADIUS * 0.32,
            ),
            10,
            14,
        );
        self.push_transient_mesh(
            &mut meshes,
            verts,
            idx,
            moon_body
                .as_ref()
                .and_then(|body| body.color_hex.as_deref())
                .map(parse_hex_color)
                .unwrap_or([0.86, 0.88, 0.92, 0.78]),
        );
        let (verts, idx) = sphere_mesh_at(
            iss_pos,
            display_body_radius(
                iss_body.as_ref(),
                "orbital-body:iss",
                GLOBE_RADIUS * 0.034,
                GLOBE_RADIUS * 0.024,
                GLOBE_RADIUS * 0.05,
            ),
            6,
            8,
        );
        self.push_transient_mesh(&mut meshes, verts, idx, [0.92, 0.98, 1.0, 0.95]);
        let tle_bodies: Vec<OrbitalBodyConfig> = self
            .orbital_bodies
            .iter()
            .filter(|body| body.body_id != "orbital-body:iss" && body.tle_line1.is_some() && body.tle_line2.is_some())
            .cloned()
            .collect();
        for body in tle_bodies
            .iter()
        {
            if let Some(pos) = self.tle_scene_position(body, now_ms) {
                let (verts, idx) = sphere_mesh_at(
                    pos,
                    display_body_radius(
                        Some(body),
                        &body.body_id,
                        GLOBE_RADIUS * 0.024,
                        GLOBE_RADIUS * 0.018,
                        GLOBE_RADIUS * 0.045,
                    ),
                    6,
                    8,
                );
                self.push_transient_mesh(
                    &mut meshes,
                    verts,
                    idx,
                    body.color_hex
                        .as_deref()
                        .map(parse_hex_color)
                        .unwrap_or([0.72, 0.96, 1.0, 0.88]),
                );
            }
        }
        let (verts, idx) = ring_mesh(Vec3::ZERO, geo_ring_radius, GLOBE_RADIUS * 0.01, 120);
        self.push_transient_mesh(&mut meshes, verts, idx, [0.5, 0.7, 1.0, 0.24 + lunar_blend * 0.18]);
        let (verts, idx) = ring_ribbon_mesh(earth_pos, moon_pos, 160, GLOBE_RADIUS * 0.008, [0.1, 0.6, 0.2]);
        self.push_transient_mesh(&mut meshes, verts, idx, [0.88, 0.9, 0.98, 0.26]);
        let (verts, idx) = ring_ribbon_mesh(earth_pos, iss_pos, 120, GLOBE_RADIUS * 0.004, [0.4, 0.15, 0.5]);
        self.push_transient_mesh(&mut meshes, verts, idx, [0.72, 0.96, 1.0, 0.32]);
        let (verts, idx) = ring_ribbon_mesh(earth_pos, sun_pos, 320, GLOBE_RADIUS * 0.018, [0.38, 0.6, 1.0]);
        self.push_transient_mesh(&mut meshes, verts, idx, [0.45, 0.72, 1.0, 0.6]);
        for (pos, radius, color) in [
            (mercury_pos, GLOBE_RADIUS * 0.05, [0.82, 0.74, 0.62, 0.92]),
            (venus_pos, GLOBE_RADIUS * 0.08, [0.9, 0.76, 0.46, 0.94]),
            (mars_pos, GLOBE_RADIUS * 0.06, [0.94, 0.46, 0.3, 0.94]),
            (jupiter_pos, GLOBE_RADIUS * 0.16, [0.92, 0.8, 0.62, 0.94]),
            (saturn_pos, GLOBE_RADIUS * 0.14, [0.9, 0.84, 0.66, 0.94]),
        ] {
            let (verts, idx) = sphere_mesh_at(pos, radius, 10, 12);
            self.push_transient_mesh(&mut meshes, verts, idx, color);
        }
        let (verts, idx) = spiral_ring_mesh(Vec3::ZERO, galaxy_radius, GLOBE_RADIUS * 0.22, 3.8, phase * 0.03);
        self.push_transient_mesh(&mut meshes, verts, idx, [0.78, 0.48, 1.0, 0.28]);
        if let Some(andromeda) = self.celestial_object("celestial-object:andromeda") {
            let anchor_pos = equatorial_anchor(
                andromeda.ra_deg.unwrap_or(10.6847083) as f32,
                andromeda.dec_deg.unwrap_or(41.26875) as f32,
                galaxy_radius * (1.18 + universe_blend * 0.62),
            );
            let (verts, idx) = sphere_mesh_at(anchor_pos, GLOBE_RADIUS * 0.24, 10, 12);
            self.push_transient_mesh(&mut meshes, verts, idx, [0.72, 0.84, 1.0, 0.75]);
        }
        if self.celestial_object("celestial-object:sagittarius-a-star").is_some() {
            let (verts, idx) = sphere_mesh_at(Vec3::ZERO, GLOBE_RADIUS * 0.08, 8, 10);
            self.push_transient_mesh(&mut meshes, verts, idx, [1.0, 0.72, 0.34, 0.84]);
        }
        let (verts, idx) = ring_mesh(Vec3::ZERO, universe_radius, GLOBE_RADIUS * 0.32, 220);
        self.push_transient_mesh(&mut meshes, verts, idx, [0.5, 0.84, 1.0, 0.16]);
        meshes
    }

    fn build_atmosphere_meshes(&self) -> Vec<TransientMesh> {
        let mut meshes = Vec::new();
        let cosmic_blend = if self.projection_mode == ProjectionMode::Cosmic {
            ((COSMIC_ZOOM_THRESHOLD - self.zoom) as f32 / 1.7).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let shell_radius = GLOBE_RADIUS * (1.028 + cosmic_blend * 0.03);
        let haze_radius = GLOBE_RADIUS * (1.075 + cosmic_blend * 0.045);
        let horizon_radius = GLOBE_RADIUS * (1.14 + cosmic_blend * 0.05);

        let mut append = |verts: Vec<f32>, idx: Vec<u32>, color: [f32; 4]| {
            if verts.is_empty() || idx.is_empty() {
                return;
            }
            let vertex_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("atmo-vb"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let index_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("atmo-ib"),
                    contents: bytemuck::cast_slice(&idx),
                    usage: wgpu::BufferUsages::INDEX,
                });
            meshes.push(TransientMesh {
                vertex_buffer,
                index_buffer,
                index_count: idx.len() as u32,
                material_bind_group: self.make_unlit_bind_group(color),
            });
        };

        let fog = self.weather.day_night.fog_color();
        let sun = self.weather.day_night.sun_color();
        let haze_color = [fog.x, fog.y, fog.z, 0.09];
        let shell_color = [
            (fog.x + sun.x) * 0.5,
            (fog.y + sun.y) * 0.5,
            (fog.z + sun.z) * 0.5,
            0.18,
        ];
        let horizon_color = [fog.x * 1.05, fog.y * 1.05, fog.z, 0.14];

        let (verts, idx) = sphere_mesh_at(Vec3::ZERO, haze_radius, 20, 28);
        append(verts, idx, haze_color);
        let (verts, idx) = sphere_mesh_at(Vec3::ZERO, shell_radius, 18, 24);
        append(verts, idx, shell_color);
        let (verts, idx) = ring_mesh(Vec3::ZERO, horizon_radius, GLOBE_RADIUS * 0.05, 180);
        append(verts, idx, horizon_color);
        meshes
    }

    fn make_unlit_bind_group(&self, color: [f32; 4]) -> wgpu::BindGroup {
        let mat = MaterialUniform {
            albedo: color,
            roughness: 1.0,
            metallic: 0.0,
            _pad: 1.0,
            ..Default::default()
        };
        let mat_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cosmic-mat"),
                contents: bytemuck::bytes_of(&mat),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cosmic-bg"),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: mat_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.fallback_white.view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.fallback_white.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&self.fallback_normal.view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.fallback_normal.sampler) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&self.fallback_mr.view) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&self.fallback_mr.sampler) },
            ],
        })
    }

    fn add_layer_from_mesh(&mut self, vertices: &[f32], indices: &[u32], color: [f32; 4]) {
        if vertices.is_empty() {
            return;
        }

        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ib = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let mat = MaterialUniform {
            albedo: color,
            roughness: 0.8,
            ..Default::default()
        };
        let mat_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&mat),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: mat_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_white.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_white.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_normal.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_mr.sampler),
                },
            ],
        });

        self.gpu_layers.push(GpuLayer {
            vertex_buffer: vb,
            index_buffer: ib,
            index_count: indices.len() as u32,
            material_bind_group: bind_group,
            source: None,
            color,
            visible: true,
            min_zoom: -4.0,
            max_zoom: 24.0,
        });
    }

    fn build_layer(
        &self,
        vertices: &[f32],
        indices: &[u32],
        color: [f32; 4],
        source: Option<LayerSource>,
    ) -> GpuLayer {
        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ib = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let mat = MaterialUniform {
            albedo: color,
            roughness: 1.0,
            metallic: 0.0,
            _pad: 1.0, // unlit fast-path
            ..Default::default()
        };
        let mat_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&mat),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: mat_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_white.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_white.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_normal.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.fallback_mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.fallback_mr.sampler),
                },
            ],
        });

        GpuLayer {
            vertex_buffer: vb,
            index_buffer: ib,
            index_count: indices.len() as u32,
            material_bind_group: bind_group,
            source,
            color,
            visible: true,
            min_zoom: -4.0,
            max_zoom: 24.0,
        }
    }

    fn rebuild_mesh_for(&self, source: &LayerSource) -> (Vec<f32>, Vec<u32>) {
        let mut vertices: Vec<f32> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut offset: u32 = 0;
        if self.projection_mode == ProjectionMode::Globe {
            match source {
                LayerSource::Lines { lines, width } => {
                    for coords in lines {
                        if coords.len() < 2 {
                            continue;
                        }
                        let m = kami_geo::mesh::globe_line_to_ribbon(
                            coords,
                            GLOBE_RADIUS,
                            self.globe_overlay_width(*width),
                            6.0,
                        );
                        let vcount = (m.vertices.len() / 8) as u32;
                        vertices.extend_from_slice(&m.vertices);
                        indices.extend(m.indices.into_iter().map(|i| i + offset));
                        offset += vcount;
                    }
                }
                LayerSource::Fill { rings } => {
                    for ring in rings {
                        if ring.len() < 3 {
                            continue;
                        }
                        let m =
                            kami_geo::mesh::globe_polygon_to_fill_earcut(ring, GLOBE_RADIUS, 3.0);
                        let vcount = (m.vertices.len() / 8) as u32;
                        vertices.extend_from_slice(&m.vertices);
                        indices.extend(m.indices.into_iter().map(|i| i + offset));
                        offset += vcount;
                    }
                }
                LayerSource::Circles {
                    points,
                    radius_world_px,
                    segments,
                } => {
                    if !points.is_empty() {
                        let m = kami_geo::mesh::globe_points_to_circles(
                            points,
                            GLOBE_RADIUS,
                            self.globe_overlay_radius(*radius_world_px),
                            8.0,
                            *segments,
                        );
                        vertices.extend_from_slice(&m.vertices);
                        indices.extend(m.indices);
                    }
                }
                LayerSource::Extrude { rings, .. } => {
                    // Globe mode: fall back to flat fill on the sphere surface
                    // (proper 3D extrusion on a sphere needs normal computation
                    // per vertex; deferred).
                    for ring in rings {
                        if ring.len() < 3 {
                            continue;
                        }
                        let m =
                            kami_geo::mesh::globe_polygon_to_fill_earcut(ring, GLOBE_RADIUS, 3.0);
                        let vcount = (m.vertices.len() / 8) as u32;
                        vertices.extend_from_slice(&m.vertices);
                        indices.extend(m.indices.into_iter().map(|i| i + offset));
                        offset += vcount;
                    }
                }
            }
            return (vertices, indices);
        }

        let iz = self.zoom.floor();
        let center_px = projection::lng_lat_to_world_px(self.center, iz);
        match source {
            LayerSource::Lines { lines, width } => {
                for coords in lines {
                    if coords.len() < 2 {
                        continue;
                    }
                    let m = kami_geo::mesh::line_to_ribbon(coords, iz, center_px, *width, 0.1);
                    let vcount = (m.vertices.len() / 8) as u32;
                    vertices.extend_from_slice(&m.vertices);
                    indices.extend(m.indices.into_iter().map(|i| i + offset));
                    offset += vcount;
                }
            }
            LayerSource::Fill { rings } => {
                for ring in rings {
                    if ring.len() < 3 {
                        continue;
                    }
                    let m = kami_geo::mesh::polygon_to_fill_earcut(ring, iz, center_px, 0.05);
                    let vcount = (m.vertices.len() / 8) as u32;
                    vertices.extend_from_slice(&m.vertices);
                    indices.extend(m.indices.into_iter().map(|i| i + offset));
                    offset += vcount;
                }
            }
            LayerSource::Circles {
                points,
                radius_world_px,
                segments,
            } => {
                if !points.is_empty() {
                    let m = kami_geo::mesh::points_to_circles(
                        points,
                        iz,
                        center_px,
                        *radius_world_px,
                        0.2,
                        *segments,
                    );
                    vertices.extend_from_slice(&m.vertices);
                    indices.extend(m.indices);
                }
            }
            LayerSource::Extrude { rings, heights, base } => {
                for (i, ring) in rings.iter().enumerate() {
                    if ring.len() < 3 {
                        continue;
                    }
                    let h = heights.get(i).copied().unwrap_or(0.0);
                    if h <= 0.0 {
                        continue;
                    }
                    let m = kami_geo::mesh::polygon_to_extrude_earcut(
                        ring, iz, center_px, *base, h,
                    );
                    let vcount = (m.vertices.len() / 8) as u32;
                    vertices.extend_from_slice(&m.vertices);
                    indices.extend(m.indices.into_iter().map(|i| i + offset));
                    offset += vcount;
                }
            }
        }
        (vertices, indices)
    }

    fn upsert_named(&mut self, id: String, layer: GpuLayer) {
        if !self.named_layers.contains_key(&id) {
            self.layer_order.push(id.clone());
        }
        self.named_layers.insert(id, layer);
    }

    fn build_globe_tile_mesh(&self, coord: TileCoord, segs: u32) -> kami_geo::mesh::GeoMesh {
        if let Some(dem) = self.dem_tiles.get(&coord) {
            return kami_geo::mesh::globe_tile_patch_from_dem(
                coord,
                GLOBE_RADIUS,
                segs,
                &dem.heights_m,
                dem.width,
                dem.height,
                GLOBE_RADIUS / 8_000_000.0,
            );
        }
        kami_geo::mesh::globe_tile_patch(coord, GLOBE_RADIUS, segs)
    }
}

fn parse_hex_color(hex: &str) -> [f32; 4] {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255) as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255) as f32 / 255.0;
    let a = if hex.len() >= 8 {
        u8::from_str_radix(&hex[6..8], 16).unwrap_or(255) as f32 / 255.0
    } else {
        1.0
    };
    [r, g, b, a]
}

fn globe_position(lng: f64, lat: f64, radius: f32) -> Vec3 {
    let lng_rad = lng.to_radians();
    let lat_rad = lat.to_radians();
    let cos_lat = lat_rad.cos() as f32;
    let sin_lat = lat_rad.sin() as f32;
    let sin_lng = lng_rad.sin() as f32;
    let cos_lng = lng_rad.cos() as f32;
    Vec3::new(
        radius * cos_lat * sin_lng,
        radius * sin_lat,
        -radius * cos_lat * cos_lng,
    )
}

fn globe_normal(lng: f64, lat: f64) -> Vec3 {
    globe_position(lng, lat, 1.0).normalize_or_zero()
}

fn globe_lng_lat_from_position(position: Vec3) -> LngLat {
    let n = position.normalize_or_zero();
    let lat = n.y.asin().to_degrees() as f64;
    let lng = n.x.atan2(-n.z).to_degrees() as f64;
    LngLat::new(lng, projection::clamp_lat(lat))
}

fn ray_sphere_hit(origin: Vec3, dir: Vec3, radius: f32) -> Option<Vec3> {
    let a = dir.dot(dir);
    let b = 2.0 * origin.dot(dir);
    let c = origin.dot(origin) - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let sqrt_disc = disc.sqrt();
    let t0 = (-b - sqrt_disc) / (2.0 * a);
    let t1 = (-b + sqrt_disc) / (2.0 * a);
    let t = if t0 >= 0.0 { t0 } else { t1 };
    (t >= 0.0).then(|| origin + dir * t)
}

fn sphere_mesh_at(center: Vec3, radius: f32, stacks: u32, slices: u32) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=stacks {
        let phi = std::f32::consts::PI * i as f32 / stacks as f32;
        let y = phi.cos();
        let rr = phi.sin();
        for j in 0..=slices {
            let theta = std::f32::consts::TAU * j as f32 / slices as f32;
            let x = rr * theta.cos();
            let z = rr * theta.sin();
            vertices.extend_from_slice(&[
                center.x + x * radius,
                center.y + y * radius,
                center.z + z * radius,
                x, y, z,
                j as f32 / slices as f32,
                i as f32 / stacks as f32,
            ]);
        }
    }
    let ring = slices + 1;
    for i in 0..stacks {
        for j in 0..slices {
            let a = i * ring + j;
            let b = a + ring;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

fn ring_mesh(center: Vec3, radius: f32, width: f32, segments: u32) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let half = width * 0.5;
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let ang = t * std::f32::consts::TAU;
        let dir = Vec3::new(ang.cos(), 0.0, ang.sin());
        let tangent = Vec3::new(-ang.sin(), 0.0, ang.cos());
        let left = center + dir * radius + tangent * half;
        let right = center + dir * radius - tangent * half;
        vertices.extend_from_slice(&[left.x, left.y, left.z, 0.0, 1.0, 0.0, t, 0.0]);
        vertices.extend_from_slice(&[right.x, right.y, right.z, 0.0, 1.0, 0.0, t, 1.0]);
    }
    for i in 0..segments {
        let base = i * 2;
        indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }
    (vertices, indices)
}

fn ring_ribbon_mesh(center: Vec3, focus: Vec3, segments: u32, width: f32, tilt: [f32; 3]) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let delta = focus - center;
    let radius_x = delta.length().max(1.0);
    let radius_z = radius_x * 0.55;
    let half = width * 0.5;
    let tilt = Vec3::new(tilt[0], tilt[1], tilt[2]).normalize_or_zero() * GLOBE_RADIUS * 0.09;
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let ang = t * std::f32::consts::TAU;
        let pos = center + Vec3::new(radius_x * ang.cos(), 0.0, radius_z * ang.sin()) + tilt * ang.sin() * 0.4;
        let tangent = Vec3::new(-radius_x * ang.sin(), tilt.y * 0.4 * ang.cos(), radius_z * ang.cos()).normalize_or_zero();
        let side = tangent.cross(Vec3::Y).normalize_or_zero();
        let left = pos + side * half;
        let right = pos - side * half;
        vertices.extend_from_slice(&[left.x, left.y, left.z, 0.0, 1.0, 0.0, t, 0.0]);
        vertices.extend_from_slice(&[right.x, right.y, right.z, 0.0, 1.0, 0.0, t, 1.0]);
    }
    for i in 0..segments {
        let base = i * 2;
        indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }
    (vertices, indices)
}

fn spiral_ring_mesh(center: Vec3, radius: f32, width: f32, turns: f32, phase: f32) -> (Vec<f32>, Vec<u32>) {
    let segments = 280_u32;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let half = width * 0.5;
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let ang = phase + t * std::f32::consts::TAU * turns;
        let local_r = radius * (0.28 + 0.72 * t);
        let pos = center + Vec3::new(local_r * ang.cos(), (t - 0.5) * GLOBE_RADIUS * 0.16, local_r * 0.55 * ang.sin());
        let tangent = Vec3::new(
            -local_r * ang.sin() + radius * 0.72 * ang.cos(),
            GLOBE_RADIUS * 0.16,
            local_r * 0.55 * ang.cos() + radius * 0.4 * ang.sin(),
        )
        .normalize_or_zero();
        let side = tangent.cross(Vec3::Y).normalize_or_zero();
        let left = pos + side * half;
        let right = pos - side * half;
        vertices.extend_from_slice(&[left.x, left.y, left.z, 0.0, 1.0, 0.0, t, 0.0]);
        vertices.extend_from_slice(&[right.x, right.y, right.z, 0.0, 1.0, 0.0, t, 1.0]);
    }
    for i in 0..segments {
        let base = i * 2;
        indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }
    (vertices, indices)
}

fn orbit_xz(radius: f32, phase: f32) -> Vec3 {
    Vec3::new(radius * phase.cos(), 0.0, radius * phase.sin())
}

fn scene_cislunar_orbit_radius(semi_major_axis_m: f32) -> f32 {
    let earth_radius_m = 6_378_137.0_f32;
    let ratio = (semi_major_axis_m / earth_radius_m).max(1.0);
    GLOBE_RADIUS * 0.95 * ratio.powf(0.25)
}

fn scene_solar_orbit_radius(semi_major_axis_m: f32, earth_axis_m: f32, solar_radius: f32) -> f32 {
    let au_ratio = (semi_major_axis_m / earth_axis_m.max(1.0)).max(0.05);
    solar_radius * au_ratio.powf(1.0 / 3.0)
}

fn orbital_phase_angle(period_s: f32, mean_longitude_deg: f32, now_s: f32) -> f32 {
    let phase = if period_s > 1.0 {
        now_s * std::f32::consts::TAU / period_s
    } else {
        0.0
    };
    mean_longitude_deg.to_radians() + phase
}

fn orbital_scene_position(radius: f32, eccentricity: f32, inclination_deg: f32, phase: f32) -> Vec3 {
    let e = eccentricity.clamp(0.0, 0.98);
    let r = radius * (1.0 - e * e) / (1.0 + e * phase.cos()).max(0.2);
    let incl = inclination_deg.to_radians();
    Vec3::new(
        r * phase.cos(),
        r * phase.sin() * incl.sin(),
        r * phase.sin() * incl.cos(),
    )
}

fn equatorial_anchor(ra_deg: f32, dec_deg: f32, radius: f32) -> Vec3 {
    let ra = ra_deg.to_radians();
    let dec = dec_deg.to_radians();
    Vec3::new(
        radius * dec.cos() * ra.cos(),
        radius * dec.sin(),
        radius * dec.cos() * ra.sin(),
    )
}

fn display_body_radius(
    body: Option<&OrbitalBodyConfig>,
    body_id: &str,
    fallback: f32,
    min_radius: f32,
    max_radius: f32,
) -> f32 {
    let Some(body) = body else {
        return fallback;
    };
    let Some(radius_m) = body.render_radius_m else {
        return fallback;
    };
    let earth_radius_m = 6_378_137.0_f32;
    let ratio = (radius_m as f32 / earth_radius_m).max(0.00001);
    let compressed = if body_id == "orbital-body:sun" {
        GLOBE_RADIUS * 0.18 * ratio.powf(0.18)
    } else if body.body_kind == "station" {
        min_radius.max(fallback)
    } else {
        GLOBE_RADIUS * ratio.powf(0.35)
    };
    compressed.clamp(min_radius, max_radius)
}

fn tle_scene_position_from_cache(cache: &CachedTle, now_ms: f64) -> Option<Vec3> {
    let now = DateTime::<Utc>::from_timestamp_millis(now_ms as i64)?.naive_utc();
    let minutes = cache.elements.datetime_to_minutes_since_epoch(&now).ok()?;
    let prediction = cache.constants.propagate(minutes).ok()?;
    let km = prediction.position;
    let eci = Vec3::new(km[0] as f32, km[2] as f32, km[1] as f32);
    let norm = eci.normalize_or_zero();
    if norm.length_squared() <= 0.0 {
        return None;
    }
    let scene_radius = scene_cislunar_orbit_radius((eci.length() * 1000.0).max(6_771_000.0));
    Some(norm * scene_radius)
}
