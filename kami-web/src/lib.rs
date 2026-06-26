//! kami-web: WebGPU browser entry point via wasm-bindgen.
//!
//! Build: wasm-pack build --target web kami-web
//! Loads in browser: import init from './kami_web.js'; await init();
//!
//! Entry points:
//!   - `run(canvas_id)` — demo scene with orbiting camera
//!   - `run_with_scene(canvas_id, scene_json)` — custom scene with WASD first-person controls
//!   - `render_document_frame(canvas_id, slide_json)` — 2D PPTX slide rendering (WebGPU + WebGL2)

#![cfg(target_family = "wasm")] // wasm-only entry crate (uses wasm-only wgpu SurfaceTarget::Canvas + kami-render::for_web_surface); native workspace build skips it

pub mod document;
pub mod entries;
pub mod math_bindings;

// Legacy path alias — kept during migration so existing external references
// to `kami_web::quarry_walk_entry::*` compile. Will be removed once all
// entries have moved under `entries/`.
pub use entries::quarry_walk as quarry_walk_entry;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use glam::Vec3;
use kami_character::params::CharacterDef;
use kami_game::scene::{IslandScene, MeshRef};
use kami_game::terrain::HeightmapTerrain;
use kami_game::voxel::{BlockType, CHUNK_SIZE, VoxelChunk, VoxelWorld};
use kami_game::voxel_mesh;
use kami_render::camera::{Camera, CameraUniform, LightUniform, MaterialUniform};
use kami_render::mesh;
use kami_render::pipeline;
use kami_render::texture;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();
    log::info!("KAMI Engine Web — initializing");
}

/// Detect platform: returns "ios", "android", or "web".
#[wasm_bindgen]
pub fn detect_platform() -> String {
    let window = web_sys::window().unwrap();
    let navigator = window.navigator();
    let ua = navigator.user_agent().unwrap_or_default();
    let platform = kami_game::platform::detect_from_user_agent(&ua);
    match platform {
        kami_game::platform::Platform::Ios => "ios".into(),
        kami_game::platform::Platform::Android => "android".into(),
        kami_game::platform::Platform::Web => "web".into(),
    }
}

/// Check if current platform is mobile (iOS or Android).
#[wasm_bindgen]
pub fn is_mobile() -> bool {
    let window = web_sys::window().unwrap();
    let navigator = window.navigator();
    let ua = navigator.user_agent().unwrap_or_default();
    kami_game::platform::detect_from_user_agent(&ua).is_mobile()
}

/// Get the currently targeted block for JS HUD crosshair highlight.
/// Returns `{x, y, z, block}` or `null` if no block is targeted.
/// Reads from `window.__kami_target_block` closure set by `run_with_scene`.
#[wasm_bindgen]
pub fn get_target_block() -> JsValue {
    let window = web_sys::window().unwrap();
    if let Ok(getter) = js_sys::Reflect::get(&window, &"__kami_target_block".into()) {
        if let Some(func) = getter.dyn_ref::<js_sys::Function>() {
            return func.call0(&JsValue::NULL).unwrap_or(JsValue::NULL);
        }
    }
    JsValue::NULL
}

/// Graph input state: scroll zoom, drag pan, click select.
#[derive(Default)]
struct GraphInputState {
    // Drag pan
    drag_active: bool,
    drag_start_x: f32,
    drag_start_y: f32,
    drag_dx: f32,
    drag_dy: f32,
    total_drag: f32,
    // Scroll zoom
    zoom_delta: f32,
    // Click select
    click_x: f32,
    click_y: f32,
    clicked: bool,
    // Double-click reset
    reset: bool,
    // Keyboard pan (WASD / arrows)
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    // Keyboard zoom (+/-)
    zoom_in: bool,
    zoom_out: bool,
}

/// Keyboard and mouse input state shared between event listeners and render loop.
#[derive(Default)]
struct InputState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    yaw: f32,
    pitch: f32,
    pointer_locked: bool,
    /// Left mouse button: mine (destroy) targeted block.
    mine: bool,
    /// Right mouse button: place block adjacent to targeted face.
    place: bool,
    /// Currently selected block type for placement (default: Stone = 3).
    selected_block: u8,
}

/// Set up keyboard and mouse listeners on the canvas.
fn setup_input(canvas: &web_sys::HtmlCanvasElement) -> Rc<RefCell<InputState>> {
    let input = Rc::new(RefCell::new(InputState {
        pitch: -0.25,      // look slightly down on spawn to see terrain
        selected_block: 3, // default placement: Stone
        ..Default::default()
    }));

    let input_kd = input.clone();
    let keydown =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_kd.borrow_mut();
            match e.code().as_str() {
                "KeyW" | "ArrowUp" => s.forward = true,
                "KeyS" | "ArrowDown" => s.backward = true,
                "KeyA" | "ArrowLeft" => s.left = true,
                "KeyD" | "ArrowRight" => s.right = true,
                "Space" => s.up = true,
                "ShiftLeft" | "ShiftRight" => s.down = true,
                _ => {}
            }
        });
    let window = web_sys::window().unwrap();
    window
        .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
        .ok();
    keydown.forget();

    let input_ku = input.clone();
    let keyup =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_ku.borrow_mut();
            match e.code().as_str() {
                "KeyW" | "ArrowUp" => s.forward = false,
                "KeyS" | "ArrowDown" => s.backward = false,
                "KeyA" | "ArrowLeft" => s.left = false,
                "KeyD" | "ArrowRight" => s.right = false,
                "Space" => s.up = false,
                "ShiftLeft" | "ShiftRight" => s.down = false,
                _ => {}
            }
        });
    window
        .add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref())
        .ok();
    keyup.forget();

    let input_mm = input.clone();
    let mousemove =
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = input_mm.borrow_mut();
            if s.pointer_locked {
                s.yaw += e.movement_x() as f32 * 0.002;
                s.pitch = (s.pitch - e.movement_y() as f32 * 0.002).clamp(-1.4, 1.4);
            }
        });
    let doc = window.document().unwrap();
    doc.add_event_listener_with_callback("mousemove", mousemove.as_ref().unchecked_ref())
        .ok();
    mousemove.forget();

    let canvas_clone = canvas.clone();
    let input_cl = input.clone();
    let click = Closure::<dyn FnMut()>::new(move || {
        canvas_clone.request_pointer_lock();
        input_cl.borrow_mut().pointer_locked = true;
    });
    canvas
        .add_event_listener_with_callback("click", click.as_ref().unchecked_ref())
        .ok();
    click.forget();

    let input_plc = input.clone();
    let plc = Closure::<dyn FnMut()>::new(move || {
        let doc = web_sys::window().unwrap().document().unwrap();
        let locked = doc.pointer_lock_element().is_some();
        input_plc.borrow_mut().pointer_locked = locked;
    });
    doc.add_event_listener_with_callback("pointerlockchange", plc.as_ref().unchecked_ref())
        .ok();
    plc.forget();

    // Mine (left click) — fires only while pointer is locked.
    let input_mine = input.clone();
    let mousedown =
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = input_mine.borrow_mut();
            if s.pointer_locked {
                match e.button() {
                    0 => s.mine = true,  // left
                    2 => s.place = true, // right
                    _ => {}
                }
            }
        });
    doc.add_event_listener_with_callback("mousedown", mousedown.as_ref().unchecked_ref())
        .ok();
    mousedown.forget();

    // Prevent context menu on right-click so place works.
    let contextmenu =
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            e.prevent_default();
        });
    canvas
        .add_event_listener_with_callback("contextmenu", contextmenu.as_ref().unchecked_ref())
        .ok();
    contextmenu.forget();

    input
}

/// Side-scroll input: horizontal movement + jump only (no mouse look).
fn setup_side_scroll_input(canvas: &web_sys::HtmlCanvasElement) -> Rc<RefCell<InputState>> {
    let input = Rc::new(RefCell::new(InputState::default()));

    let input_kd = input.clone();
    let keydown =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_kd.borrow_mut();
            match e.code().as_str() {
                "KeyA" | "ArrowLeft" => s.left = true,
                "KeyD" | "ArrowRight" => s.right = true,
                "Space" | "ArrowUp" => s.up = true,
                "ArrowDown" => s.down = true,
                _ => {}
            }
        });
    let window = web_sys::window().unwrap();
    window
        .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
        .ok();
    keydown.forget();

    let input_ku = input.clone();
    let keyup =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_ku.borrow_mut();
            match e.code().as_str() {
                "KeyA" | "ArrowLeft" => s.left = false,
                "KeyD" | "ArrowRight" => s.right = false,
                "Space" | "ArrowUp" => s.up = false,
                "ArrowDown" => s.down = false,
                _ => {}
            }
        });
    window
        .add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref())
        .ok();
    keyup.forget();

    input
}

/// Graph input: scroll zoom, drag pan, click select, keyboard pan.
fn setup_graph_input(canvas: &web_sys::HtmlCanvasElement) -> Rc<RefCell<GraphInputState>> {
    let input = Rc::new(RefCell::new(GraphInputState::default()));
    let window = web_sys::window().unwrap();

    // Mouse down → start drag
    let input_md = input.clone();
    let mousedown =
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = input_md.borrow_mut();
            s.drag_active = true;
            s.drag_start_x = e.client_x() as f32;
            s.drag_start_y = e.client_y() as f32;
            s.total_drag = 0.0;
        });
    canvas
        .add_event_listener_with_callback("mousedown", mousedown.as_ref().unchecked_ref())
        .ok();
    mousedown.forget();

    // Mouse move → accumulate drag delta
    let input_mm = input.clone();
    let mousemove =
        Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = input_mm.borrow_mut();
            if s.drag_active {
                let dx = e.client_x() as f32 - s.drag_start_x;
                let dy = e.client_y() as f32 - s.drag_start_y;
                s.drag_dx += e.movement_x() as f32;
                s.drag_dy += e.movement_y() as f32;
                s.total_drag = (dx * dx + dy * dy).sqrt();
            }
        });
    canvas
        .add_event_listener_with_callback("mousemove", mousemove.as_ref().unchecked_ref())
        .ok();
    mousemove.forget();

    // Mouse up → end drag, detect click (< 3px movement)
    let input_mu = input.clone();
    let mouseup = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
        let mut s = input_mu.borrow_mut();
        if s.drag_active && s.total_drag < 3.0 {
            s.clicked = true;
            s.click_x = e.client_x() as f32;
            s.click_y = e.client_y() as f32;
        }
        s.drag_active = false;
    });
    canvas
        .add_event_listener_with_callback("mouseup", mouseup.as_ref().unchecked_ref())
        .ok();
    mouseup.forget();

    // Wheel → zoom
    let input_wh = input.clone();
    let wheel = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |e: web_sys::WheelEvent| {
        e.prevent_default();
        let mut s = input_wh.borrow_mut();
        s.zoom_delta += e.delta_y() as f32;
    });
    canvas
        .add_event_listener_with_callback_and_add_event_listener_options(
            "wheel",
            wheel.as_ref().unchecked_ref(),
            web_sys::AddEventListenerOptions::new().passive(false),
        )
        .ok();
    wheel.forget();

    // Double-click → reset view
    let input_dbl = input.clone();
    let dblclick = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |_: web_sys::MouseEvent| {
        input_dbl.borrow_mut().reset = true;
    });
    canvas
        .add_event_listener_with_callback("dblclick", dblclick.as_ref().unchecked_ref())
        .ok();
    dblclick.forget();

    // Keyboard
    let input_kd = input.clone();
    let keydown =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_kd.borrow_mut();
            match e.code().as_str() {
                "KeyW" | "ArrowUp" => s.forward = true,
                "KeyS" | "ArrowDown" => s.backward = true,
                "KeyA" | "ArrowLeft" => s.left = true,
                "KeyD" | "ArrowRight" => s.right = true,
                "Equal" | "NumpadAdd" => s.zoom_in = true,
                "Minus" | "NumpadSubtract" => s.zoom_out = true,
                _ => {}
            }
        });
    window
        .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
        .ok();
    keydown.forget();

    let input_ku = input.clone();
    let keyup =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_ku.borrow_mut();
            match e.code().as_str() {
                "KeyW" | "ArrowUp" => s.forward = false,
                "KeyS" | "ArrowDown" => s.backward = false,
                "KeyA" | "ArrowLeft" => s.left = false,
                "KeyD" | "ArrowRight" => s.right = false,
                "Equal" | "NumpadAdd" => s.zoom_in = false,
                "Minus" | "NumpadSubtract" => s.zoom_out = false,
                _ => {}
            }
        });
    window
        .add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref())
        .ok();
    keyup.forget();

    // Touch: 1-finger drag pan
    let input_ts = input.clone();
    let touchstart =
        Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            if let Some(t) = e.touches().get(0) {
                let mut s = input_ts.borrow_mut();
                s.drag_active = true;
                s.drag_start_x = t.client_x() as f32;
                s.drag_start_y = t.client_y() as f32;
                s.total_drag = 0.0;
            }
        });
    canvas
        .add_event_listener_with_callback("touchstart", touchstart.as_ref().unchecked_ref())
        .ok();
    touchstart.forget();

    let input_tm = input.clone();
    let touchmove =
        Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            if let Some(t) = e.touches().get(0) {
                let mut s = input_tm.borrow_mut();
                if s.drag_active {
                    let cx = t.client_x() as f32;
                    let cy = t.client_y() as f32;
                    s.drag_dx += cx - s.drag_start_x;
                    s.drag_dy += cy - s.drag_start_y;
                    s.total_drag +=
                        ((cx - s.drag_start_x).powi(2) + (cy - s.drag_start_y).powi(2)).sqrt();
                    s.drag_start_x = cx;
                    s.drag_start_y = cy;
                }
            }
        });
    canvas
        .add_event_listener_with_callback("touchmove", touchmove.as_ref().unchecked_ref())
        .ok();
    touchmove.forget();

    let input_te = input.clone();
    let touchend = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |_: web_sys::TouchEvent| {
        let mut s = input_te.borrow_mut();
        if s.drag_active && s.total_drag < 10.0 {
            s.clicked = true;
            s.click_x = s.drag_start_x;
            s.click_y = s.drag_start_y;
        }
        s.drag_active = false;
    });
    canvas
        .add_event_listener_with_callback("touchend", touchend.as_ref().unchecked_ref())
        .ok();
    touchend.forget();

    input
}

/// A draw batch: mesh + material + instance transforms for one group of entities.
struct DrawBatch {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    _material_buffer: wgpu::Buffer,
    material_bind_group: wgpu::BindGroup,
    index_count: u32,
    instance_count: u32,
}

/// Fallback textures for untextured materials.
struct FallbackTextures {
    white: texture::GpuTexture,
    normal: texture::GpuTexture,
    mr: texture::GpuTexture,
}

/// Build GPU resources from scene entities, grouped by mesh type + color.
fn build_scene_batches(
    scene: &IslandScene,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    material_layout: &wgpu::BindGroupLayout,
) -> Vec<DrawBatch> {
    let fallback = FallbackTextures {
        white: texture::default_white_texture(device, queue),
        normal: texture::default_normal_texture(device, queue),
        mr: texture::default_mr_texture(device, queue),
    };
    struct EntityGroup {
        vertices: Vec<f32>,
        indices: Vec<u32>,
        material: MaterialUniform,
        transforms: Vec<f32>,
    }

    let mut groups: Vec<EntityGroup> = Vec::new();
    let mut group_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for entity in &scene.entities {
        let (key, verts, idxs, mat) = match &entity.mesh {
            MeshRef::Cube { color } => {
                let (pos, norm, uv, idx) = mesh::cube();
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "cube:{:.2},{:.2},{:.2},{:.2}",
                    color[0], color[1], color[2], color[3]
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Sphere { color, .. } => {
                let (pos, norm, uv, idx) = mesh::sphere(16, 32);
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "sphere:{:.2},{:.2},{:.2},{:.2}",
                    color[0], color[1], color[2], color[3]
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Plane {
                color,
                width,
                depth,
                subdivisions,
            } => {
                let (pos, norm, uv, idx) = mesh::plane(*width, *depth, *subdivisions);
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "plane:{:.2},{:.2},{:.2},{:.2}:{:.1}:{:.1}:{}",
                    color[0], color[1], color[2], color[3], width, depth, subdivisions
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Asset { .. } => {
                let (pos, norm, uv, idx) = mesh::cube();
                let v = mesh::interleave(&pos, &norm, &uv);
                ("asset:fallback".into(), v, idx, MaterialUniform::default())
            }
            MeshRef::Voxel {
                chunk_data,
                palette,
            } => {
                let chunk = VoxelChunk::from_column(chunk_data);
                let vm = voxel_mesh::greedy_mesh(&chunk, palette);
                // Voxel mesh has 12 floats/vertex (pos3+norm3+uv2+color4).
                // Keep all 12 floats for the per-vertex color pipeline.
                let v = vm.vertices;
                let key = format!("voxel:{}", chunk_data.len());
                (
                    key,
                    v,
                    vm.indices,
                    MaterialUniform {
                        albedo: [1.0, 1.0, 1.0, 1.0],
                        metallic: 0.0,
                        roughness: 0.8,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Terrain {
                heightmap,
                width,
                depth,
                height_scale,
            } => {
                let mut terrain = HeightmapTerrain::new(*width, *depth, *height_scale, 1.0);
                terrain.heights = heightmap.clone();
                let tm = terrain.to_mesh();
                let key = format!("terrain:{}x{}", width, depth);
                (
                    key,
                    tm.vertices,
                    tm.indices,
                    MaterialUniform {
                        albedo: [0.3, 0.5, 0.2, 1.0],
                        metallic: 0.0,
                        roughness: 0.9,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Cylinder { color, h, r1, r2 } => {
                let (pos, norm, uv, idx) = kami_scad::cylinder_mesh(*h, *r1, *r2, 16);
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "cyl:{:.2},{:.2},{:.2},{:.2}:{:.2}:{:.2}:{:.2}",
                    color[0], color[1], color[2], color[3], h, r1, r2
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Scad { code } => {
                // Full pipeline: OpenSCAD → SDF → Volume → Mesh
                let scad_mesh = kami_scad::scad_to_mesh(code, 16, 4.0, 0.5);
                let key = format!("scad:{}", code.len());
                let mat_color = [0.5, 0.5, 0.5, 1.0];
                (
                    key,
                    scad_mesh.vertices,
                    scad_mesh.indices,
                    MaterialUniform {
                        albedo: mat_color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::GaussianSplat { .. } => {
                continue;
            }
            MeshRef::SdfCharacter {
                body_parts,
                resolution,
            } => {
                // Build SDF tree from body parts: smooth union of primitives
                let res = (*resolution).max(32).min(256) as usize;
                let mut sdf_nodes: Vec<kami_sdf::SdfNode> = Vec::new();

                for part in body_parts {
                    let prim = match part.primitive.as_str() {
                        "sphere" => kami_sdf::SdfPrimitive::Sphere {
                            radius: if part.radius > 0.0 { part.radius } else { 0.1 },
                        },
                        "capsule" => kami_sdf::SdfPrimitive::Capsule {
                            h: if part.height > 0.0 { part.height } else { 0.2 },
                            r: if part.radius > 0.0 { part.radius } else { 0.05 },
                        },
                        "cylinder" => kami_sdf::SdfPrimitive::Cylinder {
                            h: if part.height > 0.0 { part.height } else { 0.2 },
                            r: if part.radius > 0.0 { part.radius } else { 0.05 },
                        },
                        "box" => kami_sdf::SdfPrimitive::Box {
                            half_extents: glam::Vec3::from(part.scale) * 0.5,
                        },
                        _ => kami_sdf::SdfPrimitive::Sphere { radius: 0.05 },
                    };

                    // Material color from preset
                    let color = match part.material_preset.as_str() {
                        "skin" => {
                            let tone = part
                                .material_params
                                .get("tone")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.8) as f32;
                            let base = 0.4 + tone * 0.5;
                            [base, base * 0.82, base * 0.72, 1.0]
                        }
                        "hair" => {
                            let lightness = part
                                .material_params
                                .get("lightness")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.5) as f32;
                            let hue = part
                                .material_params
                                .get("hue")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.1) as f32;
                            // Simplified HSL → RGB for blonde/brown
                            let r = (lightness * 0.9 + hue * 0.3).min(1.0);
                            let g = (lightness * 0.75 + hue * 0.1).min(1.0);
                            let b_val = (lightness * 0.5).min(1.0);
                            [r, g, b_val, 1.0]
                        }
                        "eye" => {
                            if let Some(ic) = part
                                .material_params
                                .get("iris_color")
                                .and_then(|v| v.as_array())
                            {
                                let r = ic.first().and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
                                let g = ic.get(1).and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                                let b_val =
                                    ic.get(2).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
                                [r, g, b_val, 1.0]
                            } else {
                                [0.3, 0.5, 0.8, 1.0]
                            }
                        }
                        "lip" => {
                            if let Some(c) =
                                part.material_params.get("color").and_then(|v| v.as_array())
                            {
                                let r = c.first().and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
                                let g = c.get(1).and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
                                let b_val = c.get(2).and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
                                [r, g, b_val, 1.0]
                            } else {
                                [0.8, 0.4, 0.4, 1.0]
                            }
                        }
                        "fabric" => {
                            if let Some(c) =
                                part.material_params.get("color").and_then(|v| v.as_array())
                            {
                                let r = c.first().and_then(|v| v.as_f64()).unwrap_or(0.9) as f32;
                                let g = c.get(1).and_then(|v| v.as_f64()).unwrap_or(0.9) as f32;
                                let b_val = c.get(2).and_then(|v| v.as_f64()).unwrap_or(0.9) as f32;
                                let a = c.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                                [r, g, b_val, a]
                            } else {
                                [0.9, 0.9, 0.9, 1.0]
                            }
                        }
                        _ => [0.7, 0.7, 0.7, 1.0],
                    };

                    // Build transform from position + rotation + scale
                    let pos = glam::Vec3::from(part.position);
                    let rot = glam::Quat::from_array(part.rotation);
                    let scl = glam::Vec3::from(part.scale);
                    let transform = glam::Mat4::from_scale_rotation_translation(scl, rot, pos);

                    sdf_nodes.push(kami_sdf::SdfNode::Primitive {
                        prim,
                        transform,
                        color,
                    });
                }

                // Smooth union all body parts for organic character look
                let sdf_tree = if sdf_nodes.len() == 1 {
                    sdf_nodes.remove(0)
                } else {
                    kami_sdf::SdfNode::SmoothUnion {
                        children: sdf_nodes,
                        k: 0.06, // smooth blend radius for organic body
                    }
                };

                // Evaluate SDF → marching cubes mesh
                let bounds = 1.0_f32;
                let center_y = 1.3_f32; // character center (torso height)
                let sdf_tree_ref = &sdf_tree;
                let loaded = kami_mesher::sdf_to_mesh(
                    |x, y, z| {
                        // Offset sampling to character center
                        let p = glam::Vec3::new(x, y + center_y, z);
                        let s = sdf_tree_ref.sample(p);
                        (s.distance, s.color)
                    },
                    res as u32,
                    bounds,
                );

                // Offset mesh vertices by center_y so SDF body aligns with eye/lip sphere entities
                let mut verts = loaded.vertices;
                // vertices are interleaved: [px, py, pz, nx, ny, nz, u, v, ...] — 8 floats per vertex
                let stride = 8;
                let vert_count = verts.len() / stride;
                for i in 0..vert_count {
                    verts[i * stride + 1] += center_y; // offset py
                }

                let key = format!("sdf_char:{}:{}", body_parts.len(), entity.id);

                // Use first body part's color as MaterialUniform albedo
                let first_color = body_parts
                    .first()
                    .map(|p| match p.material_preset.as_str() {
                        "skin" => {
                            let tone = p
                                .material_params
                                .get("tone")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.8) as f32;
                            let base = 0.4 + tone * 0.5;
                            [base, base * 0.82, base * 0.72, 1.0]
                        }
                        "hair" => {
                            let l = p
                                .material_params
                                .get("lightness")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.5) as f32;
                            let h = p
                                .material_params
                                .get("hue")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.1) as f32;
                            [
                                (l * 0.9 + h * 0.3).min(1.0),
                                (l * 0.75 + h * 0.1).min(1.0),
                                (l * 0.5).min(1.0),
                                1.0,
                            ]
                        }
                        "fabric" => {
                            if let Some(c) =
                                p.material_params.get("color").and_then(|v| v.as_array())
                            {
                                let r = c.first().and_then(|v| v.as_f64()).unwrap_or(0.9) as f32;
                                let g = c.get(1).and_then(|v| v.as_f64()).unwrap_or(0.9) as f32;
                                let b = c.get(2).and_then(|v| v.as_f64()).unwrap_or(0.9) as f32;
                                [r, g, b, 1.0]
                            } else {
                                [0.9, 0.9, 0.9, 1.0]
                            }
                        }
                        "eye" => {
                            if let Some(ic) = p
                                .material_params
                                .get("iris_color")
                                .and_then(|v| v.as_array())
                            {
                                let r = ic.first().and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
                                let g = ic.get(1).and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                                let b = ic.get(2).and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
                                [r, g, b, 1.0]
                            } else {
                                [0.3, 0.5, 0.8, 1.0]
                            }
                        }
                        "lip" => {
                            if let Some(c) =
                                p.material_params.get("color").and_then(|v| v.as_array())
                            {
                                let r = c.first().and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
                                let g = c.get(1).and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
                                let b = c.get(2).and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
                                [r, g, b, 1.0]
                            } else {
                                [0.8, 0.4, 0.4, 1.0]
                            }
                        }
                        _ => [0.7, 0.7, 0.7, 1.0],
                    })
                    .unwrap_or([0.7, 0.7, 0.7, 1.0]);
                let roughness = match body_parts.first().map(|p| p.material_preset.as_str()) {
                    Some("skin") => 0.35,
                    Some("hair") => 0.28,
                    Some("fabric") => 0.65,
                    Some("eye") => 0.05,
                    Some("lip") => 0.25,
                    _ => 0.5,
                };
                (
                    key,
                    verts,
                    loaded.indices,
                    MaterialUniform {
                        albedo: first_color,
                        metallic: 0.0,
                        roughness,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        subsurface_color: [0.8, 0.3, 0.2, 0.5],
                        subsurface_radius: [1.0, 0.4, 0.2],
                        sss_model: 1,
                        ..Default::default()
                    },
                )
            }
            MeshRef::CharacterModel { .. } => {
                // GLB/VRM loading from CDN — fallback to sphere for now
                let (pos, norm, uv, idx) = mesh::sphere(16, 32);
                let v = mesh::interleave(&pos, &norm, &uv);
                (
                    "char_model:fallback".into(),
                    v,
                    idx,
                    MaterialUniform::default(),
                )
            }
            _ => {
                let (pos, norm, uv, idx) = mesh::cube();
                let v = mesh::interleave(&pos, &norm, &uv);
                ("fallback".into(), v, idx, MaterialUniform::default())
            }
        };

        let transform = glam::Mat4::from_scale_rotation_translation(
            Vec3::from(entity.scale),
            glam::Quat::from_array(entity.rotation),
            Vec3::from(entity.position),
        );

        if let Some(&idx) = group_map.get(&key) {
            groups[idx]
                .transforms
                .extend_from_slice(&transform.to_cols_array());
        } else {
            let idx = groups.len();
            group_map.insert(key, idx);
            groups.push(EntityGroup {
                vertices: verts,
                indices: idxs,
                material: mat,
                transforms: transform.to_cols_array().to_vec(),
            });
        }
    }

    groups
        .into_iter()
        .map(|g| {
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("batch-vertex"),
                contents: bytemuck::cast_slice(&g.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("batch-index"),
                contents: bytemuck::cast_slice(&g.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("batch-instance"),
                contents: bytemuck::cast_slice(&g.transforms),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("batch-material"),
                contents: bytemuck::bytes_of(&g.material),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("batch-mat-bg"),
                layout: material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&fallback.white.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&fallback.white.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&fallback.normal.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&fallback.normal.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&fallback.mr.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&fallback.mr.sampler),
                    },
                ],
            });

            DrawBatch {
                vertex_buffer,
                index_buffer,
                instance_buffer,
                _material_buffer: material_buffer,
                material_bind_group,
                index_count: g.indices.len() as u32,
                instance_count: g.transforms.len() as u32 / 16,
            }
        })
        .collect()
}

async fn init_gpu(
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<
    (
        wgpu::Device,
        wgpu::Queue,
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
        wgpu::TextureFormat,
        u32,
        u32,
    ),
    JsValue,
> {
    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    // Delegate to the single kami-render bootstrap. Backends + Limits policy
    // lives there; every kami entry point must go through it.
    let target = wgpu::SurfaceTarget::Canvas(canvas.clone());
    let ctx = kami_render::RenderContext::for_web_surface(target, width, height, "kami-web")
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    web_sys::console::log_1(&format!("[kami-web] backend={:?}", ctx.backend).into());
    Ok((
        ctx.device,
        ctx.queue,
        ctx.surface,
        ctx.config,
        ctx.format,
        ctx.width,
        ctx.height,
    ))
}

#[wasm_bindgen]
pub async fn run(canvas_id: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;
    let scene = IslandScene::demo();

    let mut camera = Camera::new(width as f32 / height as f32);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(
        Vec3::from(scene.sun_direction),
        Vec3::ONE,
        scene.sun_intensity,
    );
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );
    let batches = build_scene_batches(&scene, &device, &queue, &material_layout);

    log::info!(
        "KAMI Engine Web — {} entities in {} batches (orbit mode)",
        scene.entities.len(),
        batches.len()
    );

    let time = Arc::new(std::sync::Mutex::new(0.0f32));
    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let time_clone = time.clone();

    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut t = time_clone.lock().unwrap();
        *t += 1.0 / 60.0;
        camera.orbit(*t * 0.3, 0.5, 30.0);
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));
        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

/// Embed mode: auto-orbit camera, no keyboard input. For iframe/mascot embed.
#[wasm_bindgen]
pub async fn run_embed(canvas_id: &str, scene_json: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let scene: IslandScene = serde_json::from_str(scene_json)
        .map_err(|e| JsValue::from_str(&format!("invalid scene JSON: {}", e)))?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;

    // Compute model center from entity positions (weighted toward character, not floor/backdrop)
    let center_y = {
        let char_entities: Vec<_> = scene
            .entities
            .iter()
            .filter(|e| e.position[1] > 0.1) // skip floor/ground entities
            .collect();
        if char_entities.is_empty() {
            1.0
        } else {
            let sum_y: f32 = char_entities.iter().map(|e| e.position[1]).sum();
            sum_y / char_entities.len() as f32
        }
    };

    let mut camera = Camera::new(width as f32 / height as f32);
    camera.target = Vec3::new(0.0, center_y, 0.0);

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(
        Vec3::from(scene.sun_direction),
        Vec3::ONE,
        scene.sun_intensity,
    );
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );
    let batches = build_scene_batches(&scene, &device, &queue, &material_layout);

    log::info!(
        "KAMI Engine Embed — {} entities in {} batches (orbit)",
        scene.entities.len(),
        batches.len()
    );

    let scene_entity_count = scene.entities.len();
    let time = Arc::new(std::sync::Mutex::new(0.0f32));
    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let time_clone = time.clone();

    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut t = time_clone.lock().unwrap();
        *t += 1.0 / 60.0;
        // Slow orbit + subtle breathing motion (camera pitch oscillation)
        let orbit_dist = if scene_entity_count > 4 { 2.5 } else { 12.0 };
        let breath = (*t * 1.2).sin() * 0.008; // gentle vertical sway
        let sway = (*t * 0.7).sin() * 0.003; // subtle horizontal sway
        camera.orbit(*t * 0.12 + sway, 0.25 + breath, orbit_dist);
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));
        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

#[wasm_bindgen]
pub async fn run_with_scene(canvas_id: &str, scene_json: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let scene: IslandScene = serde_json::from_str(scene_json)
        .map_err(|e| JsValue::from_str(&format!("invalid scene JSON: {}", e)))?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;

    let spawn_pos = scene
        .entities
        .iter()
        .find(|e| {
            e.components
                .iter()
                .any(|c| matches!(c, kami_game::scene::ComponentDef::PlayerSpawn))
        })
        .map(|e| Vec3::from(e.position))
        .unwrap_or(Vec3::new(0.0, 5.0, 10.0));

    let mut camera = Camera::new(width as f32 / height as f32);
    camera.set_position(spawn_pos + Vec3::new(0.0, 2.0, 0.0));

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(
        Vec3::from(scene.sun_direction),
        Vec3::ONE,
        scene.sun_intensity,
    );
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let is_side_scroll = scene.camera_mode.as_deref() == Some("orthographic-side");

    if is_side_scroll {
        camera.mode = kami_render::camera::CameraMode::OrthographicSide;
        // Side-scroll: camera at Z=20 looking at Z=0, following player X/Y.
        let cam_pos = spawn_pos + Vec3::new(0.0, 2.0, 0.0);
        camera.position = Vec3::new(cam_pos.x, cam_pos.y, 20.0);
        camera.target = Vec3::new(cam_pos.x, cam_pos.y, 0.0);
    }

    let (
        camera_light_layout_res,
        material_layout,
        shadow_layout_res,
        camera_light_bg,
        shadow_bg,
        depth_view,
        pbr_pipeline,
    ) = create_render_resources(
        &device,
        format,
        &camera_buffer,
        &light_buffer,
        width,
        height,
    );
    let pbr_color_pipeline = pipeline::create_pbr_color_pipeline(
        &device,
        format,
        &camera_light_layout_res,
        &material_layout,
        &shadow_layout_res,
    );
    let batches = build_scene_batches(&scene, &device, &queue, &material_layout);

    // Build VoxelWorld from all voxel entities for Minecraft-style mining/placement.
    let mut voxel_world = VoxelWorld::new();
    // Track which batch indices are voxel chunks and their world offsets.
    // (batch_index, chunk_key [cx, cy, cz])
    let mut voxel_batch_map: std::collections::HashMap<[i32; 3], usize> =
        std::collections::HashMap::new();
    {
        let cs = CHUNK_SIZE as i32;
        for entity in &scene.entities {
            if let MeshRef::Voxel {
                chunk_data,
                palette,
            } = &entity.mesh
            {
                let pos = Vec3::from(entity.position);
                let cx = (pos.x as i32).div_euclid(cs);
                let cy = (pos.y as i32).div_euclid(cs);
                let cz = (pos.z as i32).div_euclid(cs);
                let chunk = VoxelChunk::from_column(chunk_data);
                // Populate world blocks from this chunk.
                for ly in 0..CHUNK_SIZE {
                    for lz in 0..CHUNK_SIZE {
                        for lx in 0..CHUNK_SIZE {
                            let block = chunk.get(lx, ly, lz);
                            if block != BlockType::Air {
                                voxel_world.set_block(
                                    cx * cs + lx as i32,
                                    cy * cs + ly as i32,
                                    cz * cs + lz as i32,
                                    block,
                                );
                            }
                        }
                    }
                }
                if !palette.is_empty() {
                    voxel_world.palette = palette.clone();
                }
            }
        }
    }

    // Build separate COPY_DST voxel batches for chunks that can be re-meshed.
    // Max vertices per chunk: 16^3 * 6 faces * 4 verts * 12 floats (pos3+norm3+uv2+color4).
    const VOXEL_MAX_VERTS: usize = 16 * 16 * 16 * 6 * 4 * 12;
    const VOXEL_MAX_INDICES: usize = 16 * 16 * 16 * 6 * 6;
    let fallback_for_voxel = FallbackTextures {
        white: texture::default_white_texture(&device, &queue),
        normal: texture::default_normal_texture(&device, &queue),
        mr: texture::default_mr_texture(&device, &queue),
    };
    let mut voxel_batches: Vec<DrawBatch> = Vec::new();
    let mut initial_lod_map: std::collections::HashMap<[i32; 3], u32> =
        std::collections::HashMap::new();
    {
        let cs = CHUNK_SIZE as i32;
        let cam_pos = spawn_pos + Vec3::new(0.0, 2.0, 0.0);
        let cs_f = CHUNK_SIZE as f32;
        for (&chunk_key, chunk) in &voxel_world.chunks {
            let center = Vec3::new(
                chunk_key[0] as f32 * cs_f + cs_f * 0.5,
                chunk_key[1] as f32 * cs_f + cs_f * 0.5,
                chunk_key[2] as f32 * cs_f + cs_f * 0.5,
            );
            let dist = cam_pos.distance(center);
            let init_lod = 0u32; // All chunks at LOD0 — LOD downsample disabled for stability
            initial_lod_map.insert(chunk_key, init_lod);
            let nb = build_chunk_neighbors(&voxel_world, chunk_key);
            let mut vm = voxel_mesh::greedy_mesh_with_neighbors(chunk, &voxel_world.palette, &nb);
            // Bake world-space offset into vertex positions (Minecraft-style).
            // Integer chunk offsets added to small integer local coords keeps full
            // f32 precision and eliminates sub-pixel seams at chunk boundaries.
            vm.offset_positions([
                (chunk_key[0] * cs) as f32,
                (chunk_key[1] * cs) as f32,
                (chunk_key[2] * cs) as f32,
            ]);
            let v = vm.vertices;

            // Pre-allocate max-size vertex/index buffers with COPY_DST for re-meshing.
            let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("voxel-vertex"),
                size: (VOXEL_MAX_VERTS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&v));

            let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("voxel-index"),
                size: (VOXEL_MAX_INDICES * 4) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&vm.indices));

            // Identity transform — world offset already baked into vertices.
            let transform = glam::Mat4::IDENTITY;
            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("voxel-instance"),
                contents: bytemuck::cast_slice(&transform.to_cols_array()),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let material = MaterialUniform {
                albedo: [1.0, 1.0, 1.0, 1.0],
                metallic: 0.0,
                roughness: 0.8,
                has_albedo_tex: 0,
                has_normal_tex: 0,
                ..Default::default()
            };
            let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("voxel-material"),
                contents: bytemuck::bytes_of(&material),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("voxel-mat-bg"),
                layout: &material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &fallback_for_voxel.white.view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&fallback_for_voxel.white.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(
                            &fallback_for_voxel.normal.view,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(
                            &fallback_for_voxel.normal.sampler,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&fallback_for_voxel.mr.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&fallback_for_voxel.mr.sampler),
                    },
                ],
            });

            let batch_idx = voxel_batches.len();
            voxel_batch_map.insert(chunk_key, batch_idx);
            voxel_batches.push(DrawBatch {
                vertex_buffer,
                index_buffer,
                instance_buffer,
                _material_buffer: material_buffer,
                material_bind_group,
                index_count: vm.index_count,
                instance_count: 1,
            });
        }
    }

    let voxel_world = Rc::new(RefCell::new(voxel_world));
    let voxel_batches = Rc::new(RefCell::new(voxel_batches));
    let voxel_batch_map = Rc::new(RefCell::new(voxel_batch_map));

    // Shared target block state for JS HUD (get_target_block).
    // [hit_x, hit_y, hit_z, block_type, has_target]
    let target_block: Rc<RefCell<[i32; 5]>> = Rc::new(RefCell::new([0; 5]));
    {
        // Store in window for get_target_block to read.
        let window = web_sys::window().unwrap();
        let target_clone = target_block.clone();
        let getter = Closure::<dyn Fn() -> JsValue>::new(move || {
            let t = target_clone.borrow();
            if t[4] == 0 {
                JsValue::NULL
            } else {
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &"x".into(), &JsValue::from(t[0])).ok();
                js_sys::Reflect::set(&obj, &"y".into(), &JsValue::from(t[1])).ok();
                js_sys::Reflect::set(&obj, &"z".into(), &JsValue::from(t[2])).ok();
                js_sys::Reflect::set(&obj, &"block".into(), &JsValue::from(t[3])).ok();
                obj.into()
            }
        });
        js_sys::Reflect::set(&window, &"__kami_target_block".into(), getter.as_ref()).ok();
        getter.forget();
    }

    let input = if is_side_scroll {
        setup_side_scroll_input(&canvas)
    } else {
        setup_input(&canvas)
    };

    let mode_label = if is_side_scroll { "side-scroll" } else { "FPS" };
    log::info!(
        "KAMI Engine Web — {} entities in {} batches + {} voxel batches ({} mode)",
        scene.entities.len(),
        batches.len(),
        voxel_batches.borrow().len(),
        mode_label
    );

    let vw_loop = voxel_world.clone();
    let vb_loop = voxel_batches.clone();
    let vbm_loop = voxel_batch_map.clone();
    let tb_loop = target_block.clone();

    // LOD (Level of Detail) state for voxel chunks (seeded from initial build).
    let mut voxel_lod_map = initial_lod_map;
    let mut lod_frame_counter: u32 = 0;
    const LOD0_DIST: f32 = 32.0;
    const LOD1_DIST: f32 = 64.0;
    const LOD2_DIST: f32 = 128.0;

    // Day/night cycle state (600s = 10 min full cycle).
    let mut world_time: f32 = 0.25; // start at dawn (0=midnight, 0.25=dawn, 0.5=noon, 0.75=dusk)
    const DAY_CYCLE_FRAMES: f32 = 36000.0; // 600s * 60fps

    // Player physics state (persistent across frames).
    let mut player_vel_y: f32 = 0.0;
    let mut player_on_ground: bool = false;
    let mut interact_cooldown: u32 = 0; // frames until next mine/place allowed
    const GRAVITY: f32 = 0.018; // ~9.81 * (1/60)^2 scaled for frame-rate
    const JUMP_VEL: f32 = 0.22;
    const MOVE_SPEED: f32 = 0.08; // ~4.8 blocks/s at 60fps (Minecraft walk = 4.3)
    const PLAYER_HEIGHT: f32 = 1.62; // player eye height from feet (Minecraft = 1.62)
    const PLAYER_RADIUS: f32 = 0.3; // AABB half-width (Minecraft = 0.3)

    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();

    *g.lock().unwrap() = Some(Closure::new(move || {
        if is_side_scroll {
            let s = input.borrow();
            let speed = 0.15;
            if s.right {
                camera.position.x += speed;
            }
            if s.left {
                camera.position.x -= speed;
            }
            if s.up {
                camera.position.y += speed;
            }
            if s.down {
                camera.position.y -= speed;
            }
            camera.target = Vec3::new(camera.position.x, camera.position.y, 0.0);
        } else {
            let mut s = input.borrow_mut();
            let yaw = s.yaw;
            let pitch = s.pitch;
            let forward = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
            let right_dir = Vec3::new(yaw.cos(), 0.0, yaw.sin());

            // Horizontal movement intent.
            let mut move_dir = Vec3::ZERO;
            if s.forward {
                move_dir += forward;
            }
            if s.backward {
                move_dir -= forward;
            }
            if s.right {
                move_dir += right_dir;
            }
            if s.left {
                move_dir -= right_dir;
            }
            if move_dir.length_squared() > 0.001 {
                move_dir = move_dir.normalize() * MOVE_SPEED;
            }

            // Jump.
            if s.up && player_on_ground {
                player_vel_y = JUMP_VEL;
                player_on_ground = false;
            }

            // Gravity.
            player_vel_y -= GRAVITY;

            // Player feet position (camera is at eye level).
            let feet_y = camera.position.y - PLAYER_HEIGHT;

            // Helper: check if a world position is inside a solid voxel.
            let vw_phys = vw_loop.borrow();
            let is_solid = |wx: f32, wy: f32, wz: f32| -> bool {
                let bx = wx.floor() as i32;
                let by = wy.floor() as i32;
                let bz = wz.floor() as i32;
                vw_phys.get_block(bx, by, bz).is_solid()
            };

            // Resolve X axis.
            let new_x = camera.position.x + move_dir.x;
            let x_blocked = is_solid(new_x + PLAYER_RADIUS, feet_y + 0.1, camera.position.z)
                || is_solid(new_x - PLAYER_RADIUS, feet_y + 0.1, camera.position.z)
                || is_solid(new_x + PLAYER_RADIUS, feet_y + 1.0, camera.position.z)
                || is_solid(new_x - PLAYER_RADIUS, feet_y + 1.0, camera.position.z);
            if !x_blocked {
                camera.position.x = new_x;
            }

            // Resolve Z axis.
            let new_z = camera.position.z + move_dir.z;
            let z_blocked = is_solid(camera.position.x, feet_y + 0.1, new_z + PLAYER_RADIUS)
                || is_solid(camera.position.x, feet_y + 0.1, new_z - PLAYER_RADIUS)
                || is_solid(camera.position.x, feet_y + 1.0, new_z + PLAYER_RADIUS)
                || is_solid(camera.position.x, feet_y + 1.0, new_z - PLAYER_RADIUS);
            if !z_blocked {
                camera.position.z = new_z;
            }

            // Resolve Y axis (gravity + jump).
            let new_y = camera.position.y + player_vel_y;
            let new_feet = new_y - PLAYER_HEIGHT;
            if player_vel_y <= 0.0 {
                // Falling: check block below feet.
                let ground_solid = is_solid(camera.position.x, new_feet, camera.position.z)
                    || is_solid(
                        camera.position.x + PLAYER_RADIUS * 0.5,
                        new_feet,
                        camera.position.z,
                    )
                    || is_solid(
                        camera.position.x - PLAYER_RADIUS * 0.5,
                        new_feet,
                        camera.position.z,
                    );
                if ground_solid {
                    // Snap to top of block.
                    let block_top = new_feet.floor() + 1.0;
                    camera.position.y = block_top + PLAYER_HEIGHT;
                    player_vel_y = 0.0;
                    player_on_ground = true;
                } else {
                    camera.position.y = new_y;
                    player_on_ground = false;
                }
            } else {
                // Rising: check block above head.
                let head_y = new_y + 0.1;
                let ceiling_solid = is_solid(camera.position.x, head_y, camera.position.z);
                if ceiling_solid {
                    player_vel_y = 0.0;
                } else {
                    camera.position.y = new_y;
                    player_on_ground = false;
                }
            }

            // World boundary clamp (horizontal + vertical void respawn).
            let world_edge = 62.0; // slightly inside chunk grid edge
            camera.position.x = camera.position.x.clamp(-world_edge, world_edge);
            camera.position.z = camera.position.z.clamp(-world_edge, world_edge);
            if camera.position.y < -20.0 {
                camera.position.y = 30.0;
                player_vel_y = 0.0;
            }

            drop(vw_phys);

            // Update camera look direction.
            camera.target = camera.position
                + Vec3::new(
                    yaw.sin() * pitch.cos(),
                    pitch.sin(),
                    -yaw.cos() * pitch.cos(),
                );

            // DDA voxel raycast from camera position along look direction.
            let ray_origin = camera.position;
            let ray_dir = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            );
            let max_dist: f32 = 5.0; // reach distance in blocks (Minecraft = 4.5)

            let vw = vw_loop.borrow();
            let hit = dda_raycast(&vw, ray_origin, ray_dir, max_dist);

            // Update target block state for JS HUD.
            {
                let mut tb = tb_loop.borrow_mut();
                if let Some((hx, hy, hz, _nx, _ny, _nz)) = hit {
                    tb[0] = hx;
                    tb[1] = hy;
                    tb[2] = hz;
                    tb[3] = vw.get_block(hx, hy, hz) as i32;
                    tb[4] = 1;
                } else {
                    tb[4] = 0;
                }
            }
            drop(vw);

            // Handle mine/place actions with cooldown.
            let do_mine = s.mine;
            let do_place = s.place;
            let sel_block = if s.selected_block == 0 {
                3u8
            } else {
                s.selected_block
            };
            s.mine = false;
            s.place = false;
            drop(s);

            if interact_cooldown > 0 {
                interact_cooldown -= 1;
            }

            if let Some((hx, hy, hz, nx, ny, nz)) = hit {
                let cs = CHUNK_SIZE as i32;
                if do_mine && interact_cooldown == 0 {
                    interact_cooldown = 6; // ~100ms at 60fps
                    let mut vw = vw_loop.borrow_mut();
                    vw.set_block(hx, hy, hz, BlockType::Air);
                    let cx = hx.div_euclid(cs);
                    let cy = hy.div_euclid(cs);
                    let cz = hz.div_euclid(cs);
                    remesh_chunk(&vw, [cx, cy, cz], &queue, &vb_loop, &vbm_loop);
                }
                if do_place && interact_cooldown == 0 {
                    let px = hx + nx;
                    let py = hy + ny;
                    let pz = hz + nz;
                    // Prevent placing block inside the player (check player AABB).
                    let feet_y = camera.position.y - PLAYER_HEIGHT;
                    let player_collides = (px as f32 - camera.position.x).abs() < 1.0
                        && (pz as f32 - camera.position.z).abs() < 1.0
                        && (py as f32) >= feet_y - 0.5
                        && (py as f32) <= camera.position.y + 0.5;
                    if !player_collides {
                        interact_cooldown = 6;
                        let mut vw = vw_loop.borrow_mut();
                        vw.set_block(px, py, pz, BlockType::from_u8(sel_block));
                        let cx = px.div_euclid(cs);
                        let cy = py.div_euclid(cs);
                        let cz = pz.div_euclid(cs);
                        remesh_chunk(&vw, [cx, cy, cz], &queue, &vb_loop, &vbm_loop);
                    }
                }
            }
        }
        // LOD update: every 10 frames, check chunk distances and remesh if LOD changed.
        lod_frame_counter += 1;
        if false && lod_frame_counter % 30 == 0 {
            // LOD disabled — chunk boundary artifacts
            let cam = camera.position;
            let cs = CHUNK_SIZE as f32;
            let vw = vw_loop.borrow();
            let keys: Vec<[i32; 3]> = vw.chunks.keys().copied().collect();
            drop(vw);
            for key in keys {
                let center = Vec3::new(
                    key[0] as f32 * cs + cs * 0.5,
                    key[1] as f32 * cs + cs * 0.5,
                    key[2] as f32 * cs + cs * 0.5,
                );
                let dist = cam.distance(center);
                let cur = voxel_lod_map.get(&key).copied().unwrap_or(255);
                // Hysteresis: 20% margin to prevent rapid LOD switching at boundaries.
                let hyst = if cur == 255 { 0.0 } else { 6.0 };
                let new_lod = if cur <= 0 && dist < LOD0_DIST + hyst {
                    0
                } else if dist < LOD0_DIST {
                    0
                } else if cur <= 1 && dist < LOD1_DIST + hyst {
                    1
                } else if dist < LOD1_DIST {
                    1
                } else if cur <= 2 && dist < LOD2_DIST + hyst {
                    2
                } else if dist < LOD2_DIST {
                    2
                } else {
                    3
                };
                if new_lod != cur {
                    let vw = vw_loop.borrow();
                    remesh_chunk_lod(&vw, key, new_lod, &queue, &vb_loop, &vbm_loop);
                    drop(vw);
                    voxel_lod_map.insert(key, new_lod);
                }
            }
        }

        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));

        // Advance day/night cycle.
        world_time += 1.0 / DAY_CYCLE_FRAMES;
        if world_time >= 1.0 {
            world_time -= 1.0;
        }

        // Compute sky color from world_time (0=midnight → 0.5=noon → 1.0=midnight).
        let sky_color = {
            let t = world_time;
            // Smooth transitions: night(dark blue) → dawn(orange) → day(sky blue) → dusk(red) → night
            let (r, g, b) = if t < 0.2 {
                // Night: deep blue
                (0.04, 0.04, 0.12)
            } else if t < 0.3 {
                // Dawn transition: dark → orange/pink
                let f = (t - 0.2) * 10.0;
                (0.04 + f * 0.88, 0.04 + f * 0.42, 0.12 + f * 0.30)
            } else if t < 0.4 {
                // Dawn → day: orange → sky blue
                let f = (t - 0.3) * 10.0;
                (0.92 - f * 0.39, 0.46 + f * 0.35, 0.42 + f * 0.50)
            } else if t < 0.65 {
                // Day: sky blue
                (0.53, 0.81, 0.92)
            } else if t < 0.75 {
                // Dusk: sky blue → orange/red
                let f = (t - 0.65) * 10.0;
                (0.53 + f * 0.47, 0.81 - f * 0.46, 0.92 - f * 0.60)
            } else if t < 0.85 {
                // Dusk → night: red → dark
                let f = (t - 0.75) * 10.0;
                (1.0 - f * 0.96, 0.35 - f * 0.31, 0.32 - f * 0.20)
            } else {
                // Night
                (0.04, 0.04, 0.12)
            };
            [r as f32, g as f32, b as f32]
        };

        // Export world_time + sky state to JS for HUD.
        {
            let window = web_sys::window().unwrap();
            let state = js_sys::Object::new();
            let phase = if world_time < 0.2 || world_time >= 0.85 {
                "NIGHT"
            } else if world_time < 0.35 {
                "DAWN"
            } else if world_time < 0.65 {
                "DAY"
            } else {
                "DUSK"
            };
            js_sys::Reflect::set(&state, &"timePhase".into(), &JsValue::from_str(phase)).ok();
            js_sys::Reflect::set(
                &state,
                &"worldTime".into(),
                &JsValue::from_f64(world_time as f64),
            )
            .ok();
            let pos = js_sys::Array::new();
            pos.push(&JsValue::from_f64(camera.position.x as f64));
            pos.push(&JsValue::from_f64(camera.position.y as f64));
            pos.push(&JsValue::from_f64(camera.position.z as f64));
            js_sys::Reflect::set(&state, &"position".into(), &pos).ok();
            js_sys::Reflect::set(
                &state,
                &"velY".into(),
                &JsValue::from_f64(player_vel_y as f64),
            )
            .ok();
            js_sys::Reflect::set(
                &state,
                &"onGround".into(),
                &JsValue::from_bool(player_on_ground),
            )
            .ok();

            // LOD distribution counts.
            let mut lod_counts = [0u32; 4];
            for &lod in voxel_lod_map.values() {
                if (lod as usize) < 4 {
                    lod_counts[lod as usize] += 1;
                }
            }
            js_sys::Reflect::set(&state, &"lod0".into(), &JsValue::from(lod_counts[0])).ok();
            js_sys::Reflect::set(&state, &"lod1".into(), &JsValue::from(lod_counts[1])).ok();
            js_sys::Reflect::set(&state, &"lod2".into(), &JsValue::from(lod_counts[2])).ok();
            js_sys::Reflect::set(&state, &"lod3".into(), &JsValue::from(lod_counts[3])).ok();

            // Biome detection from player position (nearest biome center from scene entities).
            {
                let vw = vw_loop.borrow();
                let bx = camera.position.x.floor() as i32;
                let by = (camera.position.y - 1.7).floor() as i32;
                let bz = camera.position.z.floor() as i32;
                let block_below = vw.get_block(bx, by, bz);
                let biome_name = match block_below as u8 {
                    2 => "plains",  // Grass
                    5 => "desert",  // Sand
                    12 => "tundra", // Snow
                    15 => "nether", // Gravel
                    3 => "cave",    // Stone
                    _ => "plains",
                };
                js_sys::Reflect::set(&state, &"biome".into(), &JsValue::from_str(biome_name)).ok();
            }

            js_sys::Reflect::set(&window, &"__kami_isekai_state".into(), &state).ok();
        }

        // Render static batches with standard PBR pipeline, then voxel batches with
        // per-vertex color pipeline — pipeline switch within a single render pass.
        let vb = vb_loop.borrow();
        let static_refs: Vec<&DrawBatch> = batches.iter().collect();
        let voxel_refs: Vec<&DrawBatch> = vb.iter().collect();
        render_frame_dual_pipeline(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &pbr_color_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &static_refs,
            &voxel_refs,
            sky_color,
        );
        drop(vb);

        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

/// System graph visualizer — renders haisen/SoS JSON as PCB-style graph via WebGPU.
/// PCB layout (grid + bus). Orthographic top-down camera. WASD pan, Space/Shift zoom.
#[wasm_bindgen]
pub async fn run_with_graph(canvas_id: &str, graph_json: &str, mode: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;

    // Parse graph data and compute PCB layout (deterministic, O(n))
    let graph_data = if mode == "sos" {
        let sos: kami_graph::SoSData = serde_json::from_str(graph_json)
            .map_err(|e| JsValue::from_str(&format!("invalid SoS JSON: {}", e)))?;
        sos.to_graph()
    } else {
        let haisen: kami_graph::HaisenData = serde_json::from_str(graph_json)
            .map_err(|e| JsValue::from_str(&format!("invalid haisen JSON: {}", e)))?;
        haisen.to_graph()
    };

    let layout = kami_graph::PcbLayout::new(&graph_data);

    let (extent_w, extent_h, center) = kami_graph::graph_camera_extent_pcb(&layout, 20.0);
    let aspect = width as f32 / height as f32;
    // Fit-to-view: ortho_height = max(extent_h, extent_w/aspect)
    let fit_h = extent_h.max(extent_w / aspect) * 0.7;

    let mut camera = Camera::new(aspect);
    camera.mode = kami_render::camera::CameraMode::OrthographicTop;
    // position.y = ortho extent (OrthographicTop uses position.y as ortho_height)
    camera.position = Vec3::new(center.x, fit_h, center.y);
    camera.target = Vec3::new(center.x, 0.0, center.y);
    camera.up = Vec3::new(0.0, 0.0, -1.0);
    camera.far = fit_h * 4.0;

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    // Nintendo-style bright flat lighting (top-down, vivid colors)
    let light = LightUniform::directional(Vec3::new(-0.2, -1.0, -0.3), Vec3::ONE, 3.5);
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );

    // Build batches using existing pattern
    let fallback = FallbackTextures {
        white: texture::default_white_texture(&device, &queue),
        normal: texture::default_normal_texture(&device, &queue),
        mr: texture::default_mr_texture(&device, &queue),
    };

    let mut batches: Vec<DrawBatch> = Vec::new();

    // Helper: create a DrawBatch from mesh + color + instance transforms
    let make_batch = |verts: &[f32],
                      indices: &[u32],
                      color: [f32; 4],
                      transforms: &[f32],
                      device: &wgpu::Device,
                      material_layout: &wgpu::BindGroupLayout,
                      fallback: &FallbackTextures|
     -> DrawBatch {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("graph-vb"),
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("graph-ib"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("graph-inst"),
            contents: bytemuck::cast_slice(transforms),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mat_uniform = MaterialUniform {
            albedo: color,
            metallic: 0.0,
            roughness: 1.0,
            has_albedo_tex: 0,
            has_normal_tex: 0,
            ..Default::default()
        };
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("graph-mat"),
            contents: bytemuck::bytes_of(&mat_uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("graph-mat-bg"),
            layout: material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&fallback.white.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&fallback.white.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&fallback.normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&fallback.normal.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&fallback.mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&fallback.mr.sampler),
                },
            ],
        });
        DrawBatch {
            vertex_buffer,
            index_buffer,
            instance_buffer,
            _material_buffer: material_buffer,
            material_bind_group,
            index_count: indices.len() as u32,
            instance_count: transforms.len() as u32 / 16,
        }
    };

    // Node cubes — PCB component style. Scale to be visible relative to graph extent.
    let (positions, normals, uvs, cube_indices) = mesh::cube();
    let cube_verts = mesh::interleave(&positions, &normals, &uvs);

    // Node size proportional to graph extent (ensure ~1% of view height)
    let node_scale = fit_h * 0.02; // ~2% of view = clearly visible
    let mut group_transforms: std::collections::HashMap<usize, Vec<f32>> =
        std::collections::HashMap::new();
    for node in &layout.nodes {
        if node.radius <= 0.0 {
            continue;
        } // skip bus/collection nodes
        let s = node_scale * (node.radius / 6.0);
        let t = glam::Mat4::from_scale_rotation_translation(
            Vec3::new(s, s * 0.3, s),
            glam::Quat::IDENTITY,
            Vec3::new(node.x, 0.0, node.y),
        );
        group_transforms
            .entry(node.group_index)
            .or_default()
            .extend_from_slice(&t.to_cols_array());
    }
    for (gi, transforms) in &group_transforms {
        batches.push(make_batch(
            &cube_verts,
            &cube_indices,
            kami_graph::group_color(*gi),
            transforms,
            &device,
            &material_layout,
            &fallback,
        ));
    }

    // Edge traces — PCB-style orthogonal routing (L-shaped: horizontal then vertical).
    // Use flat boxes instead of cylinders for PCB trace appearance.
    let (trace_pos, trace_norm, trace_uv, trace_indices) = mesh::cube();
    let trace_verts = mesh::interleave(&trace_pos, &trace_norm, &trace_uv);
    let trace_thickness = fit_h * 0.003;

    // Bus lines: horizontal traces at bus Y positions (full width of graph)
    let mut bus_transforms: Vec<f32> = Vec::new();
    for bus in &layout.buses {
        let t = glam::Mat4::from_scale_rotation_translation(
            Vec3::new(extent_w * 0.5, trace_thickness * 0.5, trace_thickness * 0.5),
            glam::Quat::IDENTITY,
            Vec3::new(center.x, 0.0, bus.y),
        );
        bus_transforms.extend_from_slice(&t.to_cols_array());
    }
    if !bus_transforms.is_empty() {
        batches.push(make_batch(
            &trace_verts,
            &trace_indices,
            [0.75, 0.78, 0.82, 1.0], // light gray bus
            &bus_transforms,
            &device,
            &material_layout,
            &fallback,
        ));
    }

    // Stubs: vertical traces from each app node down to bus lines it connects to
    let mut stub_transforms: std::collections::HashMap<String, Vec<f32>> =
        std::collections::HashMap::new();
    for edge in &layout.edges {
        let from = &layout.nodes[edge.from_idx];
        let to = &layout.nodes[edge.to_idx];
        // App → bus stub: from is app (radius>0), to is bus (radius=0)
        if from.radius > 0.0 && to.radius <= 0.0 {
            let dz = to.y - from.y;
            if dz.abs() > 0.01 {
                let mid_z = (from.y + to.y) * 0.5;
                let t = glam::Mat4::from_scale_rotation_translation(
                    Vec3::new(trace_thickness * 0.3, trace_thickness * 0.3, dz.abs() * 0.5),
                    glam::Quat::IDENTITY,
                    Vec3::new(from.x, 0.0, mid_z),
                );
                stub_transforms
                    .entry(edge.edge_type.clone())
                    .or_default()
                    .extend_from_slice(&t.to_cols_array());
            }
        }
        // App ↔ App direct edge (invoke, etc.)
        if from.radius > 0.0 && to.radius > 0.0 {
            let fx = from.x;
            let fz = from.y;
            let tx = to.x;
            let tz = to.y;
            let dx = tx - fx;
            let dz = tz - fz;
            if dx.abs() > 0.01 {
                let mid_x = (fx + tx) * 0.5;
                let t = glam::Mat4::from_scale_rotation_translation(
                    Vec3::new(dx.abs() * 0.5, trace_thickness, trace_thickness),
                    glam::Quat::IDENTITY,
                    Vec3::new(mid_x, 0.0, fz),
                );
                stub_transforms
                    .entry(edge.edge_type.clone())
                    .or_default()
                    .extend_from_slice(&t.to_cols_array());
            }
            if dz.abs() > 0.01 {
                let mid_z = (fz + tz) * 0.5;
                let t = glam::Mat4::from_scale_rotation_translation(
                    Vec3::new(trace_thickness, trace_thickness, dz.abs() * 0.5),
                    glam::Quat::IDENTITY,
                    Vec3::new(tx, 0.0, mid_z),
                );
                stub_transforms
                    .entry(edge.edge_type.clone())
                    .or_default()
                    .extend_from_slice(&t.to_cols_array());
            }
        }
    }
    for (etype, transforms) in &stub_transforms {
        batches.push(make_batch(
            &trace_verts,
            &trace_indices,
            kami_graph::edge_color(etype),
            transforms,
            &device,
            &material_layout,
            &fallback,
        ));
    }

    let n_nodes = layout.nodes.len();
    let n_edges = layout.edges.len();
    log::info!(
        "KAMI Graph — {} nodes, {} edges in {} batches | extent: {:.0}x{:.0} center: ({:.0},{:.0}) fit_h: {:.0} node_scale: {:.2}",
        n_nodes,
        n_edges,
        batches.len(),
        extent_w,
        extent_h,
        center.x,
        center.y,
        fit_h,
        node_scale
    );

    // Graph input: scroll zoom, drag pan, click select
    let input = setup_graph_input(&canvas);

    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();

    let mut zoom_level = fit_h;
    let initial_pos = camera.position;
    let initial_target = camera.target;
    let canvas_h = height as f32;

    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut s = input.borrow_mut();

        // Drag pan: pixels → world units
        if s.drag_dx.abs() > 0.1 || s.drag_dy.abs() > 0.1 {
            let scale = zoom_level * 2.0 / canvas_h; // ortho_height = position.y, view = 2*position.y
            camera.position.x -= s.drag_dx * scale;
            camera.target.x -= s.drag_dx * scale;
            camera.position.z -= s.drag_dy * scale;
            camera.target.z -= s.drag_dy * scale;
            s.drag_dx = 0.0;
            s.drag_dy = 0.0;
        }

        // Scroll zoom
        if s.zoom_delta.abs() > 0.1 {
            // Per-frame zoom: clamp accumulated delta to ±1 step, then apply 5% change
            let sign = s.zoom_delta.signum();
            let factor = if sign > 0.0 { 1.05 } else { 0.95 }; // 5% per frame
            zoom_level = (zoom_level * factor).clamp(5.0, 50000.0);
            // Consume one step worth of delta
            s.zoom_delta -= sign * 120.0;
            if s.zoom_delta.signum() != sign {
                s.zoom_delta = 0.0;
            }
        }

        // Keyboard pan
        let speed = zoom_level * 0.015;
        if s.right {
            camera.position.x += speed;
            camera.target.x += speed;
        }
        if s.left {
            camera.position.x -= speed;
            camera.target.x -= speed;
        }
        if s.forward {
            camera.position.z -= speed;
            camera.target.z -= speed;
        }
        if s.backward {
            camera.position.z += speed;
            camera.target.z += speed;
        }

        // Keyboard zoom
        if s.zoom_in {
            zoom_level = (zoom_level * 0.97).max(5.0);
        }
        if s.zoom_out {
            zoom_level = (zoom_level * 1.03).min(10000.0);
        }

        // Double-click reset
        if s.reset {
            camera.position = initial_pos;
            camera.target = initial_target;
            zoom_level = fit_h;
            s.reset = false;
        }

        // Click select (clear after read)
        if s.clicked {
            // TODO: ray pick nearest node from click_x, click_y
            s.clicked = false;
        }

        // Apply zoom to camera
        camera.far = zoom_level * 4.0;
        camera.position.y = zoom_level;

        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));
        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );

        // Export camera state for JS label overlay
        let w = web_sys::window().unwrap();
        let _ = js_sys::Reflect::set(
            &w,
            &"__kami_cam_x".into(),
            &(camera.position.x as f64).into(),
        );
        let _ = js_sys::Reflect::set(
            &w,
            &"__kami_cam_z".into(),
            &(camera.position.z as f64).into(),
        );
        let _ = js_sys::Reflect::set(&w, &"__kami_cam_zoom".into(), &(zoom_level as f64).into());

        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    // Export node list for JS label overlay
    let mut node_json = String::from("[");
    let mut first = true;
    for (i, node) in layout.nodes.iter().enumerate() {
        if node.radius <= 0.0 {
            continue;
        }
        if i >= graph_data.nodes.len() {
            continue;
        }
        if !first {
            node_json.push(',');
        }
        first = false;
        let name = &graph_data.nodes[i].label;
        let group = &graph_data.nodes[i].group;
        node_json.push_str(&format!(
            "{{\"n\":\"{}\",\"g\":\"{}\",\"x\":{:.1},\"z\":{:.1}}}",
            name.replace('"', ""),
            group.replace('"', ""),
            node.x,
            node.y
        ));
    }
    node_json.push(']');

    let w = web_sys::window().unwrap();
    let _ = js_sys::Reflect::set(&w, &"__kami_nodes".into(), &node_json.into());
    let _ = js_sys::Reflect::set(&w, &"__kami_cam_w".into(), &(width as f64).into());
    let _ = js_sys::Reflect::set(&w, &"__kami_cam_h".into(), &(height as f64).into());
    let _ = js_sys::Reflect::set(&w, &"__kami_cam_zoom".into(), &(fit_h as f64).into());
    let _ = js_sys::Reflect::set(&w, &"__kami_cam_x".into(), &(initial_pos.x as f64).into());
    let _ = js_sys::Reflect::set(&w, &"__kami_cam_z".into(), &(initial_pos.z as f64).into());

    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

/// Goriketsu Dash!! — chase game on KAMI Engine.
/// Loads scene JSON-LD + runs GoriketsuGame logic each frame.
/// Top-down camera follows player. WASD move, E slap, Space sprint.
#[wasm_bindgen]
pub async fn run_with_game(
    canvas_id: &str,
    scene_json: &str,
    game_id: &str,
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let scene: IslandScene = serde_json::from_str(scene_json)
        .map_err(|e| JsValue::from_str(&format!("invalid scene JSON: {}", e)))?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;

    // Top-down chase camera
    let mut camera = Camera::new(width as f32 / height as f32);

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(
        Vec3::from(scene.sun_direction),
        Vec3::ONE,
        scene.sun_intensity,
    );
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );

    // Build scene batches with COPY_DST on instance buffers for dynamic updates
    let batches = build_game_batches(&scene, &device, &queue, &material_layout);

    // Build entity ID → (batch_index, instance_index) lookup
    let entity_lookup = build_entity_lookup(&scene);

    // Game input: WASD + E (interact) + Space (sprint)
    let input = setup_game_input(&canvas);

    // Create game instance
    let mut game = kami_game::ketsu::GoriketsuGame::new();

    log::info!(
        "KAMI Engine — Goriketsu Dash!! ({} entities, {} batches, game_id={})",
        scene.entities.len(),
        batches.len(),
        game_id
    );

    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let window_for_state = window.clone();

    *g.lock().unwrap() = Some(Closure::new(move || {
        let dt = 1.0 / 60.0f32;

        // Convert web input → game input
        let s = input.borrow();
        let virtual_input = js_sys::Reflect::get(
            &window_for_state,
            &JsValue::from_str("__kami_ketsu_virtual_input"),
        )
        .ok()
        .unwrap_or(JsValue::UNDEFINED);
        let game_input = kami_game::input::InputState {
            forward: s.forward || js_bool_prop(&virtual_input, "forward"),
            backward: s.backward || js_bool_prop(&virtual_input, "backward"),
            left: s.left || js_bool_prop(&virtual_input, "left"),
            right: s.right || js_bool_prop(&virtual_input, "right"),
            jump: s.up || js_bool_prop(&virtual_input, "jump"), // Space = sprint in ketsu
            interact: s.interact || js_bool_prop(&virtual_input, "interact"),
            chat: false,
        };
        drop(s);

        // Game tick
        game.update(&game_input, dt);

        if let Ok(state_json) = serde_json::to_string(&game.snapshot()) {
            if let Ok(state_value) = js_sys::JSON::parse(&state_json) {
                let _ = js_sys::Reflect::set(
                    &window_for_state,
                    &JsValue::from_str("__kami_ketsu_state"),
                    &state_value,
                );
            }
        }

        // Clear interact after consume
        input.borrow_mut().interact = false;
        if !virtual_input.is_undefined() && !virtual_input.is_null() {
            let _ = js_sys::Reflect::set(
                &virtual_input,
                &JsValue::from_str("interact"),
                &JsValue::FALSE,
            );
        }

        // Update camera (top-down follow player with look-ahead)
        let player_pos = game.player_pos;
        let look_ahead = game.player_vel * 2.0;
        let cam_target = player_pos + look_ahead;
        let cam_height = 45.0 + game.screen_shake * 5.0;
        let shake_x = if game.screen_shake > 0.0 {
            (game.tick as f32 * 17.3).sin() * game.screen_shake * 2.0
        } else {
            0.0
        };
        let shake_z = if game.screen_shake > 0.0 {
            (game.tick as f32 * 23.7).cos() * game.screen_shake * 2.0
        } else {
            0.0
        };
        camera.position = Vec3::new(
            cam_target.x + shake_x,
            cam_height,
            cam_target.z + 15.0 + shake_z,
        );
        camera.target = Vec3::new(cam_target.x, 0.0, cam_target.z);

        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));

        // Apply game entity positions to scene batches
        let updates = game.entity_positions();
        for upd in &updates {
            if let Some(&(batch_idx, inst_idx)) = entity_lookup.get(upd.id.as_str()) {
                if batch_idx < batches.len() {
                    let scale = if upd.visible { upd.scale } else { Vec3::ZERO };
                    let transform = glam::Mat4::from_scale_rotation_translation(
                        scale,
                        glam::Quat::IDENTITY,
                        upd.position,
                    );
                    let offset = (inst_idx * 16 * 4) as u64; // 16 floats * 4 bytes
                    queue.write_buffer(
                        &batches[batch_idx].instance_buffer,
                        offset,
                        bytemuck::cast_slice(&transform.to_cols_array()),
                    );
                }
            }
        }

        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

// ── Turntable Input State (Sabiotoshi) ──

#[derive(Default)]
struct TurntableInputState {
    drag_active: bool,
    drag_dx: f32,
    drag_dy: f32,
    zoom_delta: f32,
    pointer_x: f32,
    pointer_y: f32,
    applying_tool: bool,
    interact: bool,
    tool_select: Option<u8>,
}

fn setup_turntable_input(canvas: &web_sys::HtmlCanvasElement) -> Rc<RefCell<TurntableInputState>> {
    let input = Rc::new(RefCell::new(TurntableInputState::default()));
    let window = web_sys::window().unwrap();

    // Mouse drag for rotation
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = input_c.borrow_mut();
            s.drag_active = true;
            s.pointer_x = e.client_x() as f32;
            s.pointer_y = e.client_y() as f32;
        });
        canvas
            .add_event_listener_with_callback("mousedown", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = input_c.borrow_mut();
            if s.drag_active {
                let nx = e.client_x() as f32;
                let ny = e.client_y() as f32;
                s.drag_dx += nx - s.pointer_x;
                s.drag_dy += ny - s.pointer_y;
                s.pointer_x = nx;
                s.pointer_y = ny;
            }
        });
        window
            .add_event_listener_with_callback("mousemove", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |_e: web_sys::MouseEvent| {
            input_c.borrow_mut().drag_active = false;
        });
        window
            .add_event_listener_with_callback("mouseup", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    // Scroll zoom
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |e: web_sys::WheelEvent| {
            e.prevent_default();
            input_c.borrow_mut().zoom_delta += if e.delta_y() > 0.0 { -1.0 } else { 1.0 };
        });
        let opts = web_sys::AddEventListenerOptions::new();
        opts.set_passive(false);
        canvas
            .add_event_listener_with_callback_and_add_event_listener_options(
                "wheel",
                cb.as_ref().unchecked_ref(),
                &opts,
            )
            .ok();
        cb.forget();
    }
    // Touch drag + pinch
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let mut s = input_c.borrow_mut();
            if let Some(t) = e.touches().item(0) {
                s.drag_active = true;
                s.pointer_x = t.client_x() as f32;
                s.pointer_y = t.client_y() as f32;
            }
            if e.touches().length() >= 2 {
                s.applying_tool = true;
            }
        });
        canvas
            .add_event_listener_with_callback("touchstart", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let mut s = input_c.borrow_mut();
            if let Some(t) = e.touches().item(0) {
                let nx = t.client_x() as f32;
                let ny = t.client_y() as f32;
                if s.drag_active {
                    s.drag_dx += nx - s.pointer_x;
                    s.drag_dy += ny - s.pointer_y;
                }
                s.pointer_x = nx;
                s.pointer_y = ny;
            }
        });
        canvas
            .add_event_listener_with_callback("touchmove", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let mut s = input_c.borrow_mut();
            s.drag_active = false;
            s.applying_tool = false;
        });
        canvas
            .add_event_listener_with_callback("touchend", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    // Keyboard: E=interact, 1-6=tool select
    {
        let input_c = input.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                let mut s = input_c.borrow_mut();
                match e.code().as_str() {
                    "KeyE" => s.interact = true,
                    "Space" => s.applying_tool = true,
                    "Digit1" => s.tool_select = Some(0),
                    "Digit2" => s.tool_select = Some(1),
                    "Digit3" => s.tool_select = Some(2),
                    "Digit4" => s.tool_select = Some(3),
                    "Digit5" => s.tool_select = Some(4),
                    "Digit6" => s.tool_select = Some(5),
                    _ => {}
                }
            });
        window
            .add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    {
        let input_c = input.clone();
        let cb =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                let mut s = input_c.borrow_mut();
                match e.code().as_str() {
                    "Space" => s.applying_tool = false,
                    _ => {}
                }
            });
        window
            .add_event_listener_with_callback("keyup", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }
    // Right-click = apply tool
    {
        let input_c = input.clone();
        let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            e.prevent_default();
            if e.button() == 2 {
                // right click
                input_c.borrow_mut().applying_tool = true;
            }
        });
        canvas
            .add_event_listener_with_callback("contextmenu", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    }

    input
}

/// Sabi-Otoshi!! — 3D rust restoration game on KAMI Engine.
/// Turntable camera orbit, SDF items, NeRF rust, step-by-step disassembly.
/// Drag to rotate, scroll to zoom, Space/right-click to apply tool, E to disassemble, 1-6 tool select.
#[wasm_bindgen]
pub async fn run_with_sabiotoshi(canvas_id: &str, scene_json: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let scene: IslandScene = serde_json::from_str(scene_json)
        .map_err(|e| JsValue::from_str(&format!("invalid scene JSON: {}", e)))?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;

    let mut camera = Camera::new(width as f32 / height as f32);

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(
        Vec3::from(scene.sun_direction),
        Vec3::ONE,
        scene.sun_intensity,
    );
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );

    let batches = build_game_batches(&scene, &device, &queue, &material_layout);
    let entity_lookup = build_entity_lookup(&scene);
    let input = setup_turntable_input(&canvas);

    // Create Sabiotoshi game with default item catalog
    let items = kami_game::sabiotoshi::default_item_catalog();
    let total = items.len().min(8) as u32;
    let mut game = kami_game::sabiotoshi::SabiotoshiGame::new(items, total);
    game.start_game(); // Auto-start: skip title, go to Inspecting

    log::info!(
        "KAMI Engine — Sabi-Otoshi!! ({} entities, {} batches, {} items)",
        scene.entities.len(),
        batches.len(),
        total
    );

    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();

    *g.lock().unwrap() = Some(Closure::new(move || {
        let dt = 1.0 / 60.0f32;

        // Read turntable input
        let mut s = input.borrow_mut();
        let drag_dx = s.drag_dx * 0.005;
        let drag_dy = s.drag_dy * 0.005;
        let zoom = s.zoom_delta;
        s.drag_dx = 0.0;
        s.drag_dy = 0.0;
        s.zoom_delta = 0.0;

        // Update turntable camera
        game.camera.update(drag_dx, drag_dy, zoom, dt);

        // Tool selection
        if let Some(tool_idx) = s.tool_select.take() {
            let tools = kami_game::sabiotoshi::ToolKind::all();
            if (tool_idx as usize) < tools.len() {
                game.select_tool(tools[tool_idx as usize]);
            }
        }

        // Apply tool (Space held or right-click)
        game.is_applying_tool = s.applying_tool;

        // Raycast contact point from pointer (simplified: center of item)
        if game.is_applying_tool {
            let eye = game.camera.eye_position();
            let fwd = (game.camera.target - eye).normalize();
            game.contact_point = Some(game.camera.target + fwd * 0.5);
        } else {
            game.contact_point = None;
        }

        // Convert to game input
        let game_input = kami_game::input::InputState {
            forward: false,
            backward: false,
            left: false,
            right: false,
            jump: false,
            interact: s.interact,
            chat: false,
        };
        s.interact = false;
        drop(s);

        // Game tick
        game.update(&game_input, dt);

        // Update camera from turntable
        let eye = game.camera.eye_position();
        camera.position = eye;
        camera.target = game.camera.target;
        let shake = game.screen_shake;
        if shake > 0.0 {
            camera.position.x += (game.tick as f32 * 17.3).sin() * shake * 0.5;
            camera.position.y += (game.tick as f32 * 23.7).cos() * shake * 0.5;
        }
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));

        // Apply entity position updates from game to GPU batches
        let updates = game.entity_positions();
        for upd in &updates {
            if let Some(&(batch_idx, inst_idx)) = entity_lookup.get(upd.id.as_str()) {
                if batch_idx < batches.len() {
                    let scale = if upd.visible { upd.scale } else { Vec3::ZERO };
                    let transform = glam::Mat4::from_scale_rotation_translation(
                        scale,
                        upd.rotation,
                        upd.position,
                    );
                    let offset = (inst_idx * 16 * 4) as u64;
                    queue.write_buffer(
                        &batches[batch_idx].instance_buffer,
                        offset,
                        bytemuck::cast_slice(&transform.to_cols_array()),
                    );
                }
            }
        }

        // Export game state to JS (window.__kami_sabiotoshi)
        {
            let w = web_sys::window().unwrap();
            let state = js_sys::Object::new();
            let phase_str = match game.phase {
                kami_game::sabiotoshi::Phase::Title => "title",
                kami_game::sabiotoshi::Phase::Inspecting => "inspecting",
                kami_game::sabiotoshi::Phase::Restoring => "restoring",
                kami_game::sabiotoshi::Phase::Disassembling => "disassembling",
                kami_game::sabiotoshi::Phase::ItemClear => "item_clear",
                kami_game::sabiotoshi::Phase::AllClear => "all_clear",
                kami_game::sabiotoshi::Phase::Timeout => "timeout",
            };
            js_sys::Reflect::set(&state, &"phase".into(), &phase_str.into()).ok();
            js_sys::Reflect::set(&state, &"score".into(), &(game.score as f64).into()).ok();
            js_sys::Reflect::set(&state, &"combo".into(), &(game.combo as f64).into()).ok();
            js_sys::Reflect::set(&state, &"maxCombo".into(), &(game.max_combo as f64).into()).ok();
            js_sys::Reflect::set(
                &state,
                &"itemsCleared".into(),
                &(game.items_cleared as f64).into(),
            )
            .ok();
            js_sys::Reflect::set(
                &state,
                &"totalItems".into(),
                &(game.total_items as f64).into(),
            )
            .ok();
            js_sys::Reflect::set(
                &state,
                &"timeLeft".into(),
                &(game.time_remaining as f64).into(),
            )
            .ok();
            js_sys::Reflect::set(&state, &"perfects".into(), &(game.perfects as f64).into()).ok();
            let tool_name = game.current_tool.name();
            js_sys::Reflect::set(&state, &"tool".into(), &tool_name.into()).ok();
            js_sys::Reflect::set(
                &state,
                &"toolJa".into(),
                &game.current_tool.name_ja().into(),
            )
            .ok();
            js_sys::Reflect::set(&state, &"grade".into(), &game.grade().to_string().into()).ok();
            if let Some(item) = game.current_item() {
                js_sys::Reflect::set(&state, &"itemName".into(), &item.name.as_str().into()).ok();
                js_sys::Reflect::set(&state, &"itemNameJa".into(), &item.name_ja.as_str().into())
                    .ok();
                js_sys::Reflect::set(&state, &"itemDiff".into(), &(item.difficulty as f64).into())
                    .ok();
                // rust progress
                let total_rust: f32 = item.zones.iter().map(|z| z.initial_level).sum();
                let remaining_rust: f32 = item.zones.iter().map(|z| z.current_level).sum();
                let pct = if total_rust > 0.0 {
                    1.0 - remaining_rust / total_rust
                } else {
                    1.0
                };
                js_sys::Reflect::set(&state, &"rustProgress".into(), &(pct as f64).into()).ok();
            }
            // SFX requests
            let sfx_arr = js_sys::Array::new();
            for s in &game.sfx_request {
                sfx_arr.push(&s.as_str().into());
            }
            js_sys::Reflect::set(&state, &"sfx".into(), &sfx_arr).ok();
            // Haptic request
            if let Some(h) = game.haptic_request {
                js_sys::Reflect::set(&state, &"haptic".into(), &(h as f64).into()).ok();
            }
            // Message
            if let Some((ref msg, _)) = game.message {
                js_sys::Reflect::set(&state, &"message".into(), &msg.as_str().into()).ok();
            }
            js_sys::Reflect::set(&w, &"__kami_sabiotoshi".into(), &state).ok();
        }

        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

/// Game input: WASD + E (interact/slap) + Space (sprint) + touch (mobile).
fn setup_game_input(canvas: &web_sys::HtmlCanvasElement) -> Rc<RefCell<GameInputState>> {
    let input = Rc::new(RefCell::new(GameInputState::default()));

    // Keyboard input (desktop)
    let input_kd = input.clone();
    let keydown =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_kd.borrow_mut();
            match e.code().as_str() {
                "KeyW" | "ArrowUp" => s.forward = true,
                "KeyS" | "ArrowDown" => s.backward = true,
                "KeyA" | "ArrowLeft" => s.left = true,
                "KeyD" | "ArrowRight" => s.right = true,
                "Space" => s.up = true,
                "KeyE" => s.interact = true,
                _ => {}
            }
        });
    let window = web_sys::window().unwrap();
    window
        .add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())
        .ok();
    keydown.forget();

    let input_ku = input.clone();
    let keyup =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let mut s = input_ku.borrow_mut();
            match e.code().as_str() {
                "KeyW" | "ArrowUp" => s.forward = false,
                "KeyS" | "ArrowDown" => s.backward = false,
                "KeyA" | "ArrowLeft" => s.left = false,
                "KeyD" | "ArrowRight" => s.right = false,
                "Space" => s.up = false,
                _ => {} // interact is consumed per-frame, not on keyup
            }
        });
    window
        .add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref())
        .ok();
    keyup.forget();

    // Touch input (mobile: virtual joystick left half, buttons right half)
    let canvas_w = canvas.client_width() as f32;

    let input_ts = input.clone();
    let canvas_w_ts = canvas_w;
    let touchstart =
        Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let touches = e.changed_touches();
            for i in 0..touches.length() {
                if let Some(t) = touches.item(i) {
                    let x = t.client_x() as f32;
                    let y = t.client_y() as f32;
                    let mut s = input_ts.borrow_mut();
                    if x > canvas_w_ts * 0.5 {
                        // Right half: top = slap, bottom = sprint
                        if y < web_sys::window()
                            .unwrap()
                            .inner_height()
                            .unwrap()
                            .as_f64()
                            .unwrap() as f32
                            * 0.5
                        {
                            s.interact = true;
                        } else {
                            s.up = true; // sprint
                        }
                    } else {
                        // Left half: store joystick origin
                        s.touch_origin_x = x;
                        s.touch_origin_y = y;
                        s.touch_active = true;
                    }
                }
            }
        });
    canvas
        .add_event_listener_with_callback("touchstart", touchstart.as_ref().unchecked_ref())
        .ok();
    touchstart.forget();

    let input_tm = input.clone();
    let touchmove =
        Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let touches = e.touches();
            let mut s = input_tm.borrow_mut();
            if !s.touch_active {
                return;
            }
            // Find leftmost touch for joystick
            for i in 0..touches.length() {
                if let Some(t) = touches.item(i) {
                    let x = t.client_x() as f32;
                    if x <= web_sys::window()
                        .unwrap()
                        .inner_width()
                        .unwrap()
                        .as_f64()
                        .unwrap() as f32
                        * 0.5
                    {
                        let y = t.client_y() as f32;
                        let dx = x - s.touch_origin_x;
                        let dy = y - s.touch_origin_y;
                        let deadzone = 15.0;
                        s.left = dx < -deadzone;
                        s.right = dx > deadzone;
                        s.forward = dy < -deadzone;
                        s.backward = dy > deadzone;
                        break;
                    }
                }
            }
        });
    canvas
        .add_event_listener_with_callback("touchmove", touchmove.as_ref().unchecked_ref())
        .ok();
    touchmove.forget();

    let input_te = input.clone();
    let touchend = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
        e.prevent_default();
        let mut s = input_te.borrow_mut();
        // If no more touches, reset all
        let remaining = e.touches().length();
        if remaining == 0 {
            s.forward = false;
            s.backward = false;
            s.left = false;
            s.right = false;
            s.up = false;
            s.touch_active = false;
        } else {
            // Check if joystick touch ended
            let mut has_left = false;
            let touches = e.touches();
            let half_w = web_sys::window()
                .unwrap()
                .inner_width()
                .unwrap()
                .as_f64()
                .unwrap() as f32
                * 0.5;
            for i in 0..touches.length() {
                if let Some(t) = touches.item(i) {
                    if (t.client_x() as f32) <= half_w {
                        has_left = true;
                    }
                }
            }
            if !has_left {
                s.forward = false;
                s.backward = false;
                s.left = false;
                s.right = false;
                s.touch_active = false;
            }
            // Check if right-half touches ended → stop sprint
            let mut has_right_bottom = false;
            let half_h = web_sys::window()
                .unwrap()
                .inner_height()
                .unwrap()
                .as_f64()
                .unwrap() as f32
                * 0.5;
            for i in 0..touches.length() {
                if let Some(t) = touches.item(i) {
                    if t.client_x() as f32 > half_w && t.client_y() as f32 >= half_h {
                        has_right_bottom = true;
                    }
                }
            }
            if !has_right_bottom {
                s.up = false;
            }
        }
    });
    canvas
        .add_event_listener_with_callback("touchend", touchend.as_ref().unchecked_ref())
        .ok();
    touchend.forget();

    input
}

#[derive(Default)]
struct GameInputState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    interact: bool,
    // Touch joystick state
    touch_origin_x: f32,
    touch_origin_y: f32,
    touch_active: bool,
}

fn js_bool_prop(obj: &JsValue, key: &str) -> bool {
    js_sys::Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Build scene batches with COPY_DST on instance buffers for per-frame transform updates.
fn build_game_batches(
    scene: &IslandScene,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    material_layout: &wgpu::BindGroupLayout,
) -> Vec<DrawBatch> {
    // Same as build_scene_batches but with VERTEX | COPY_DST on instance buffers
    let fallback = FallbackTextures {
        white: texture::default_white_texture(device, queue),
        normal: texture::default_normal_texture(device, queue),
        mr: texture::default_mr_texture(device, queue),
    };
    struct EntityGroup {
        vertices: Vec<f32>,
        indices: Vec<u32>,
        material: MaterialUniform,
        transforms: Vec<f32>,
    }

    let mut groups: Vec<EntityGroup> = Vec::new();
    let mut group_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for entity in &scene.entities {
        let (key, verts, idxs, mat) = match &entity.mesh {
            MeshRef::Cube { color } => {
                let (pos, norm, uv, idx) = mesh::cube();
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "cube:{:.2},{:.2},{:.2},{:.2}",
                    color[0], color[1], color[2], color[3]
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Sphere { color, .. } => {
                let (pos, norm, uv, idx) = mesh::sphere(16, 32);
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "sphere:{:.2},{:.2},{:.2},{:.2}",
                    color[0], color[1], color[2], color[3]
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Plane {
                color,
                width,
                depth,
                subdivisions,
            } => {
                let (pos, norm, uv, idx) = mesh::plane(*width, *depth, *subdivisions);
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "plane:{:.2},{:.2},{:.2},{:.2}:{:.1}:{:.1}:{}",
                    color[0], color[1], color[2], color[3], width, depth, subdivisions
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::Cylinder { color, h, r1, r2 } => {
                let (pos, norm, uv, idx) = kami_scad::cylinder_mesh(*h, *r1, *r2, 16);
                let v = mesh::interleave(&pos, &norm, &uv);
                let key = format!(
                    "cyl:{:.2},{:.2},{:.2},{:.2}:{:.2}:{:.2}:{:.2}",
                    color[0], color[1], color[2], color[3], h, r1, r2
                );
                (
                    key,
                    v,
                    idx,
                    MaterialUniform {
                        albedo: *color,
                        metallic: 0.0,
                        roughness: 0.5,
                        has_albedo_tex: 0,
                        has_normal_tex: 0,
                        ..Default::default()
                    },
                )
            }
            MeshRef::GaussianSplat { .. } => continue,
            _ => {
                let (pos, norm, uv, idx) = mesh::cube();
                let v = mesh::interleave(&pos, &norm, &uv);
                ("fallback".into(), v, idx, MaterialUniform::default())
            }
        };

        let transform = glam::Mat4::from_scale_rotation_translation(
            Vec3::from(entity.scale),
            glam::Quat::from_array(entity.rotation),
            Vec3::from(entity.position),
        );

        if let Some(&idx) = group_map.get(&key) {
            groups[idx]
                .transforms
                .extend_from_slice(&transform.to_cols_array());
        } else {
            let idx = groups.len();
            group_map.insert(key, idx);
            groups.push(EntityGroup {
                vertices: verts,
                indices: idxs,
                material: mat,
                transforms: transform.to_cols_array().to_vec(),
            });
        }
    }

    groups
        .into_iter()
        .map(|g| {
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("game-vertex"),
                contents: bytemuck::cast_slice(&g.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("game-index"),
                contents: bytemuck::cast_slice(&g.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("game-instance"),
                contents: bytemuck::cast_slice(&g.transforms),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, // COPY_DST for dynamic updates
            });
            let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("game-material"),
                contents: bytemuck::bytes_of(&g.material),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("game-mat-bg"),
                layout: material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&fallback.white.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&fallback.white.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&fallback.normal.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&fallback.normal.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&fallback.mr.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&fallback.mr.sampler),
                    },
                ],
            });

            DrawBatch {
                vertex_buffer,
                index_buffer,
                instance_buffer,
                _material_buffer: material_buffer,
                material_bind_group,
                index_count: g.indices.len() as u32,
                instance_count: g.transforms.len() as u32 / 16,
            }
        })
        .collect()
}

/// Build entity ID → (batch_index, instance_index) lookup for dynamic transform updates.
fn build_entity_lookup(scene: &IslandScene) -> std::collections::HashMap<String, (usize, usize)> {
    let mut lookup = std::collections::HashMap::new();
    let mut group_map: std::collections::HashMap<String, (usize, usize)> =
        std::collections::HashMap::new(); // key → (batch_idx, next_instance)
    let mut batch_idx_counter = 0usize;

    for entity in &scene.entities {
        let key = match &entity.mesh {
            MeshRef::Cube { color } => format!(
                "cube:{:.2},{:.2},{:.2},{:.2}",
                color[0], color[1], color[2], color[3]
            ),
            MeshRef::Sphere { color, .. } => format!(
                "sphere:{:.2},{:.2},{:.2},{:.2}",
                color[0], color[1], color[2], color[3]
            ),
            MeshRef::Plane {
                color,
                width,
                depth,
                subdivisions,
            } => format!(
                "plane:{:.2},{:.2},{:.2},{:.2}:{:.1}:{:.1}:{}",
                color[0], color[1], color[2], color[3], width, depth, subdivisions
            ),
            MeshRef::Cylinder { color, h, r1, r2 } => format!(
                "cyl:{:.2},{:.2},{:.2},{:.2}:{:.2}:{:.2}:{:.2}",
                color[0], color[1], color[2], color[3], h, r1, r2
            ),
            MeshRef::GaussianSplat { .. } => continue,
            _ => "fallback".into(),
        };

        if let Some((bi, inst)) = group_map.get_mut(&key) {
            lookup.insert(entity.id.clone(), (*bi, *inst));
            *inst += 1;
        } else {
            let bi = batch_idx_counter;
            batch_idx_counter += 1;
            lookup.insert(entity.id.clone(), (bi, 0));
            group_map.insert(key, (bi, 1));
        }
    }

    lookup
}

/// Embed mode with OpenSCAD code: parse → SDF → voxelize → mesh → render.
/// Supports volume_type: "dense" | "sparse" | "octree".
#[wasm_bindgen]
/// SCAD: OpenSCAD text → parser → evaluator → per-entity SDF → mesh.
/// Each primitive is a separate entity (union = no fusion between parts).
/// This demonstrates what an LLM generates: human-readable CSG text.
pub async fn run_embed_scad(
    canvas_id: &str,
    scad_code: &str,
    resolution: u32,
    volume_type: &str,
) -> Result<(), JsValue> {
    let entities = kami_scad::evaluate(scad_code);
    let bounds = 5.0f32;
    let res = resolution.max(8).min(64);

    if volume_type == "dense" {
        // Mesh entire SCAD SDF tree at once, then split by per-vertex color
        let sdf_tree = kami_scad::entities_to_sdf(&entities);
        let (mesh, vertex_colors) = kami_mesher::sdf_to_colored_mesh(
            |x, y, z| {
                let s = sdf_tree.sample(Vec3::new(x, y, z));
                (s.distance, s.color)
            },
            res,
            bounds,
        );
        let meshes = kami_mesher::split_mesh_by_color(&mesh, &vertex_colors);
        log::info!("SCAD/colored — res={} groups={}", res, meshes.len());
        return render_multi_mesh_orbit(canvas_id, meshes).await;
    }

    // Voxel paths: combine via hard union
    let sdf_tree = kami_scad::entities_to_sdf(&entities);
    let dense_vol = kami_sdf::sample_sdf(&sdf_tree, res, bounds);
    let volume = convert_volume(dense_vol, res, volume_type);
    let scale = bounds * 2.0 / res as f32;
    render_volume_smooth_orbit(canvas_id, &volume, bounds, res, [0.34, 0.80, 0.01, 1.0]).await
}

/// YORO as separate colored part groups for multi-material rendering.
/// Returns Vec<(SdfNode, color)> — each part is meshed + rendered individually.
fn yoro_parts() -> Vec<(kami_sdf::SdfNode, [f32; 4])> {
    use glam::{Mat4, Vec3};
    use kami_sdf::{SdfNode, SdfPrimitive};
    let p = |prim, t, c| SdfNode::Primitive {
        prim,
        transform: t,
        color: c,
    };
    let green = [0.34, 0.80, 0.01, 1.0];
    let dark_green = [0.27, 0.65, 0.01, 1.0];
    let white = [1.0, 1.0, 1.0, 1.0];
    let blue = [0.07, 0.69, 0.96, 1.0];
    let dark = [0.1, 0.1, 0.18, 1.0];
    let hat_color = [0.90, 0.90, 0.92, 1.0];
    let pink = [1.0, 0.6, 0.6, 0.5];

    vec![
        // Body + head (smooth union for organic shape)
        (
            SdfNode::SmoothUnion {
                children: vec![
                    p(
                        SdfPrimitive::Sphere { radius: 1.5 },
                        Mat4::from_translation(Vec3::new(0.0, 1.2, 0.0)),
                        green,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 1.4 },
                        Mat4::from_translation(Vec3::new(0.0, 2.8, 0.0)),
                        green,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 1.1 },
                        Mat4::from_translation(Vec3::new(0.0, 0.9, 0.3))
                            * Mat4::from_scale(Vec3::new(0.8, 0.6, 0.5)),
                        green,
                    ),
                ],
                k: 0.3,
            },
            green,
        ),
        // Arms
        (
            SdfNode::SmoothUnion {
                children: vec![
                    p(
                        SdfPrimitive::Capsule { h: 1.0, r: 0.25 },
                        Mat4::from_translation(Vec3::new(-1.6, 1.3, 0.0))
                            * Mat4::from_rotation_z(0.5),
                        dark_green,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 0.22 },
                        Mat4::from_translation(Vec3::new(-2.0, 0.8, 0.0)),
                        dark_green,
                    ),
                    p(
                        SdfPrimitive::Capsule { h: 1.0, r: 0.25 },
                        Mat4::from_translation(Vec3::new(1.6, 1.3, 0.0))
                            * Mat4::from_rotation_z(-0.5),
                        dark_green,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 0.22 },
                        Mat4::from_translation(Vec3::new(2.0, 0.8, 0.0)),
                        dark_green,
                    ),
                ],
                k: 0.15,
            },
            dark_green,
        ),
        // Feet
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Sphere { radius: 0.4 },
                    Mat4::from_translation(Vec3::new(-0.55, 0.15, 0.2))
                        * Mat4::from_scale(Vec3::new(1.0, 0.45, 1.3)),
                    dark_green,
                ),
                p(
                    SdfPrimitive::Sphere { radius: 0.4 },
                    Mat4::from_translation(Vec3::new(0.55, 0.15, 0.2))
                        * Mat4::from_scale(Vec3::new(1.0, 0.45, 1.3)),
                    dark_green,
                ),
            ]),
            dark_green,
        ),
        // Eye whites
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Sphere { radius: 0.48 },
                    Mat4::from_translation(Vec3::new(-0.6, 2.9, 1.05))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.55)),
                    white,
                ),
                p(
                    SdfPrimitive::Sphere { radius: 0.48 },
                    Mat4::from_translation(Vec3::new(0.6, 2.9, 1.05))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.55)),
                    white,
                ),
            ]),
            white,
        ),
        // Irises
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Sphere { radius: 0.30 },
                    Mat4::from_translation(Vec3::new(-0.6, 2.95, 1.3))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.45)),
                    blue,
                ),
                p(
                    SdfPrimitive::Sphere { radius: 0.30 },
                    Mat4::from_translation(Vec3::new(0.6, 2.95, 1.3))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.45)),
                    blue,
                ),
            ]),
            blue,
        ),
        // Pupils
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Sphere { radius: 0.15 },
                    Mat4::from_translation(Vec3::new(-0.6, 2.95, 1.42))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.4)),
                    dark,
                ),
                p(
                    SdfPrimitive::Sphere { radius: 0.15 },
                    Mat4::from_translation(Vec3::new(0.6, 2.95, 1.42))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.4)),
                    dark,
                ),
            ]),
            dark,
        ),
        // Cheeks
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Sphere { radius: 0.25 },
                    Mat4::from_translation(Vec3::new(-1.05, 2.55, 0.85))
                        * Mat4::from_scale(Vec3::new(1.0, 0.5, 0.4)),
                    pink,
                ),
                p(
                    SdfPrimitive::Sphere { radius: 0.25 },
                    Mat4::from_translation(Vec3::new(1.05, 2.55, 0.85))
                        * Mat4::from_scale(Vec3::new(1.0, 0.5, 0.4)),
                    pink,
                ),
            ]),
            pink,
        ),
        // Hat
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Box {
                        half_extents: Vec3::new(0.7, 0.06, 0.7),
                    },
                    Mat4::from_translation(Vec3::new(0.0, 3.95, 0.0)),
                    hat_color,
                ),
                p(
                    SdfPrimitive::Cylinder { h: 0.7, r: 0.5 },
                    Mat4::from_translation(Vec3::new(0.0, 4.35, 0.0)),
                    hat_color,
                ),
                p(
                    SdfPrimitive::Box {
                        half_extents: Vec3::new(0.55, 0.04, 0.55),
                    },
                    Mat4::from_translation(Vec3::new(0.0, 4.72, 0.0)),
                    hat_color,
                ),
            ]),
            hat_color,
        ),
    ]
}

/// SDF-style parts: smooth union with high k → organic/clay fusion.
/// Different from SCAD (which uses hard union via OpenSCAD parser).
fn yoro_parts_sdf_style() -> Vec<(kami_sdf::SdfNode, [f32; 4])> {
    use glam::{Mat4, Vec3};
    use kami_sdf::{SdfNode, SdfPrimitive};
    let p = |prim, t, c| SdfNode::Primitive {
        prim,
        transform: t,
        color: c,
    };
    let green = [0.34, 0.80, 0.01, 1.0];
    let dark_green = [0.27, 0.65, 0.01, 1.0];
    let white = [1.0, 1.0, 1.0, 1.0];
    let blue = [0.07, 0.69, 0.96, 1.0];
    let hat_color = [0.90, 0.90, 0.92, 1.0];

    vec![
        // Body+head+belly — VERY smooth union (k=0.5) → single organic blob
        (
            SdfNode::SmoothUnion {
                children: vec![
                    p(
                        SdfPrimitive::Sphere { radius: 1.6 },
                        Mat4::from_translation(Vec3::new(0.0, 1.2, 0.0)),
                        green,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 1.5 },
                        Mat4::from_translation(Vec3::new(0.0, 2.9, 0.0)),
                        green,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 1.0 },
                        Mat4::from_translation(Vec3::new(0.0, 0.8, 0.3)),
                        green,
                    ),
                    // Arms fused into body
                    p(
                        SdfPrimitive::Capsule { h: 1.2, r: 0.3 },
                        Mat4::from_translation(Vec3::new(-1.5, 1.3, 0.0))
                            * Mat4::from_rotation_z(0.6),
                        green,
                    ),
                    p(
                        SdfPrimitive::Capsule { h: 1.2, r: 0.3 },
                        Mat4::from_translation(Vec3::new(1.5, 1.3, 0.0))
                            * Mat4::from_rotation_z(-0.6),
                        green,
                    ),
                    // Feet fused into body
                    p(
                        SdfPrimitive::Sphere { radius: 0.45 },
                        Mat4::from_translation(Vec3::new(-0.55, 0.1, 0.2))
                            * Mat4::from_scale(Vec3::new(1.0, 0.5, 1.3)),
                        dark_green,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 0.45 },
                        Mat4::from_translation(Vec3::new(0.55, 0.1, 0.2))
                            * Mat4::from_scale(Vec3::new(1.0, 0.5, 1.3)),
                        dark_green,
                    ),
                ],
                k: 0.5,
            },
            green,
        ),
        // Eyes (sharp, not fused)
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Sphere { radius: 0.5 },
                    Mat4::from_translation(Vec3::new(-0.6, 2.95, 1.1))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.55)),
                    white,
                ),
                p(
                    SdfPrimitive::Sphere { radius: 0.5 },
                    Mat4::from_translation(Vec3::new(0.6, 2.95, 1.1))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.55)),
                    white,
                ),
            ]),
            white,
        ),
        // Irises
        (
            SdfNode::Union(vec![
                p(
                    SdfPrimitive::Sphere { radius: 0.32 },
                    Mat4::from_translation(Vec3::new(-0.6, 3.0, 1.3))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.45)),
                    blue,
                ),
                p(
                    SdfPrimitive::Sphere { radius: 0.32 },
                    Mat4::from_translation(Vec3::new(0.6, 3.0, 1.3))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, 0.45)),
                    blue,
                ),
            ]),
            blue,
        ),
        // Hat (smooth union for rounded hat)
        (
            SdfNode::SmoothUnion {
                children: vec![
                    p(
                        SdfPrimitive::Cylinder { h: 0.12, r: 0.7 },
                        Mat4::from_translation(Vec3::new(0.0, 4.0, 0.0)),
                        hat_color,
                    ),
                    p(
                        SdfPrimitive::Cylinder { h: 0.8, r: 0.5 },
                        Mat4::from_translation(Vec3::new(0.0, 4.45, 0.0)),
                        hat_color,
                    ),
                    p(
                        SdfPrimitive::Sphere { radius: 0.52 },
                        Mat4::from_translation(Vec3::new(0.0, 4.85, 0.0)),
                        hat_color,
                    ),
                ],
                k: 0.2,
            },
            hat_color,
        ),
    ]
}

/// Combined SDF for single-mesh paths (voxelization).
fn yoro_sdf_tree() -> kami_sdf::SdfNode {
    let parts = yoro_parts();
    kami_sdf::SdfNode::Union(parts.into_iter().map(|(node, _)| node).collect())
}

/// Build YORO as synthetic NeRF density grid.
fn yoro_nerf_grid(res: u32) -> kami_nerf::DensityGrid {
    let bounds = 5.0f32;
    let sdf = yoro_sdf_tree();
    let mut data = Vec::with_capacity((res * res * res) as usize);
    let mut colors = Vec::with_capacity((res * res * res) as usize);
    let step = bounds * 2.0 / res as f32;
    for z in 0..res {
        for y in 0..res {
            for x in 0..res {
                let p = Vec3::new(
                    -bounds + (x as f32 + 0.5) * step,
                    -bounds + (y as f32 + 0.5) * step,
                    -bounds + (z as f32 + 0.5) * step,
                );
                let s = sdf.sample(p);
                let density = if s.distance <= 0.0 { 1.0 } else { 0.0 };
                data.push(density);
                colors.push([s.color[0], s.color[1], s.color[2]]);
            }
        }
    }
    kami_nerf::DensityGrid::new(
        data,
        [res, res, res],
        Vec3::splat(-bounds),
        Vec3::splat(bounds),
    )
    .with_colors(colors)
}

/// Render a volume (any backend) as a single mesh in orbit mode.
/// Create a ground plane + grid batch for visual context.
fn create_ground_batch(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    material_layout: &wgpu::BindGroupLayout,
) -> DrawBatch {
    let (pos, norm, uv, idx) = mesh::plane(20.0, 20.0, 0);
    let verts = mesh::interleave(&pos, &norm, &uv);
    let mat = MaterialUniform {
        albedo: [0.08, 0.08, 0.10, 1.0],
        metallic: 0.0,
        roughness: 0.95,
        has_albedo_tex: 0,
        has_normal_tex: 0,
        ..Default::default()
    };
    // Ground at y=-0.5
    let ground_transform = glam::Mat4::from_translation(Vec3::new(0.0, -0.5, 0.0));
    let fallback = FallbackTextures {
        white: texture::default_white_texture(device, queue),
        normal: texture::default_normal_texture(device, queue),
        mr: texture::default_mr_texture(device, queue),
    };
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&idx),
        usage: wgpu::BufferUsages::INDEX,
    });
    let inst = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&ground_transform.to_cols_array()),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let mb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&mat),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mbg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: material_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: mb.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&fallback.white.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&fallback.white.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&fallback.normal.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&fallback.normal.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&fallback.mr.view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&fallback.mr.sampler),
            },
        ],
    });
    DrawBatch {
        vertex_buffer: vb,
        index_buffer: ib,
        instance_buffer: inst,
        _material_buffer: mb,
        material_bind_group: mbg,
        index_count: idx.len() as u32,
        instance_count: 1,
    }
}

async fn render_volume_orbit(
    canvas_id: &str,
    volume: &kami_voxel::VoxelVolume,
    scale: f32,
    color: [f32; 4],
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let mesh = kami_mesher::marching_cubes(volume, scale);
    if mesh.vertex_count == 0 {
        return Err("empty mesh".into());
    }

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;
    let mut camera = Camera::new(width as f32 / height as f32);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cam"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(Vec3::new(-0.3, -1.5, -0.8), Vec3::ONE, 2.5);
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("lit"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );

    let fallback = FallbackTextures {
        white: texture::default_white_texture(&device, &queue),
        normal: texture::default_normal_texture(&device, &queue),
        mr: texture::default_mr_texture(&device, &queue),
    };
    let mat = MaterialUniform {
        albedo: color,
        metallic: 0.0,
        roughness: 0.5,
        has_albedo_tex: 0,
        has_normal_tex: 0,
        ..Default::default()
    };
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let inst = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&identity),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let mb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&mat),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mbg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &material_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: mb.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&fallback.white.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&fallback.white.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&fallback.normal.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&fallback.normal.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&fallback.mr.view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&fallback.mr.sampler),
            },
        ],
    });
    let ground = create_ground_batch(&device, &queue, &material_layout);
    let batches = vec![
        DrawBatch {
            vertex_buffer: vb,
            index_buffer: ib,
            instance_buffer: inst,
            _material_buffer: mb,
            material_bind_group: mbg,
            index_count: mesh.index_count,
            instance_count: 1,
        },
        ground,
    ];

    let time = Arc::new(std::sync::Mutex::new(0.0f32));
    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let tc = time.clone();
    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut t = tc.lock().unwrap();
        *t += 1.0 / 60.0;
        camera.orbit(*t * 0.15, 0.4, 12.0);
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));
        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));
    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

/// SDF: Direct Rust SDF code — smooth union fuses parts organically.
/// This is what a programmer writes in Rust: mathematical distance functions.
#[wasm_bindgen]
pub async fn run_embed_sdf(
    canvas_id: &str,
    resolution: u32,
    volume_type: &str,
) -> Result<(), JsValue> {
    let bounds = 5.0f32;
    let res = resolution.max(8).min(64);

    // SDF-specific: smooth union with HIGH k → parts melt into each other (organic/clay look)
    let sdf_parts = yoro_parts_sdf_style();

    if volume_type == "dense" {
        let meshes: Vec<_> = sdf_parts
            .iter()
            .map(|(node, color)| {
                let mesh = kami_mesher::sdf_to_mesh(
                    |x, y, z| {
                        let s = node.sample(Vec3::new(x, y, z));
                        (s.distance, s.color)
                    },
                    res,
                    bounds,
                );
                (mesh, *color)
            })
            .collect();
        log::info!("SDF/multi — res={} parts={}", res, meshes.len());
        return render_multi_mesh_orbit(canvas_id, meshes).await;
    }
    // For voxel paths: combine all into one SDF with smooth union
    let combined = kami_sdf::SdfNode::SmoothUnion {
        children: sdf_parts.into_iter().map(|(n, _)| n).collect(),
        k: 0.4, // high k = very smooth fusion (SDF characteristic)
    };
    let dense = kami_sdf::sample_sdf(&combined, res, bounds);
    let volume = convert_volume(dense, res, volume_type);
    let scale = bounds * 2.0 / res as f32;
    render_volume_smooth_orbit(canvas_id, &volume, bounds, res, [0.34, 0.80, 0.01, 1.0]).await
}

/// SDF JSON-LD: Parse JSON-LD string into SDF tree → mesh → render.
/// Most LLM-efficient format (η=0.95).
#[wasm_bindgen]
pub async fn run_embed_sdf_jsonld(
    canvas_id: &str,
    jsonld: &str,
    resolution: u32,
    volume_type: &str,
) -> Result<(), JsValue> {
    let bounds = 5.0f32;
    let res = resolution.max(8).min(64);

    let sdf_tree = kami_sdf::parse_sdf_jsonld(jsonld).map_err(|e| JsValue::from_str(&e))?;

    if volume_type == "dense" {
        // Mesh entire SDF tree at once (captures all parts regardless of size),
        // then split triangles into color-grouped sub-meshes for per-part rendering.
        let (mesh, vertex_colors) = kami_mesher::sdf_to_colored_mesh(
            |x, y, z| {
                let s = sdf_tree.sample(Vec3::new(x, y, z));
                (s.distance, s.color)
            },
            res,
            bounds,
        );
        let meshes = kami_mesher::split_mesh_by_color(&mesh, &vertex_colors);
        log::info!(
            "SDF-JSONLD/colored — res={} groups={} total_verts={}",
            res,
            meshes.len(),
            mesh.vertex_count
        );
        return render_multi_mesh_orbit(canvas_id, meshes).await;
    }
    let dense = kami_sdf::sample_sdf(&sdf_tree, res, bounds);
    let volume = convert_volume(dense, res, volume_type);
    let scale = bounds * 2.0 / res as f32;
    render_volume_smooth_orbit(canvas_id, &volume, bounds, res, [0.34, 0.80, 0.01, 1.0]).await
}

/// NeRF: Density grid with noise + blur — simulates a learned 3D reconstruction.
#[wasm_bindgen]
pub async fn run_embed_nerf(
    canvas_id: &str,
    resolution: u32,
    volume_type: &str,
) -> Result<(), JsValue> {
    let res = resolution.max(8).min(64);
    let bounds = 5.0f32;

    // NeRF-specific: sample SDF, add noise + gaussian blur to simulate learned density
    let base_sdf = yoro_sdf_tree();
    let mut data = Vec::with_capacity((res * res * res) as usize);
    let mut colors = Vec::with_capacity((res * res * res) as usize);
    let step = bounds * 2.0 / res as f32;

    for z in 0..res {
        for y in 0..res {
            for x in 0..res {
                let px = -bounds + (x as f32 + 0.5) * step;
                let py = -bounds + (y as f32 + 0.5) * step;
                let pz = -bounds + (z as f32 + 0.5) * step;
                let s = base_sdf.sample(Vec3::new(px, py, pz));

                // Convert SDF to density (sigmoid-like) + add pseudo-random noise
                let sigmoid_density = 1.0 / (1.0 + (s.distance * 3.0).exp());
                // Pseudo-random noise based on position (deterministic hash)
                let noise_seed = (x.wrapping_mul(73856093)
                    ^ y.wrapping_mul(19349663)
                    ^ z.wrapping_mul(83492791)) as f32;
                let noise = ((noise_seed % 1000.0) / 1000.0 - 0.5) * 0.15; // ±0.075 noise
                let density = (sigmoid_density + noise).clamp(0.0, 1.0);

                data.push(density);
                // NeRF colors: slightly desaturated + warm shift (learned reconstruction artifact)
                let warmth = 0.05;
                colors.push([
                    (s.color[0] * 0.85 + warmth).min(1.0),
                    (s.color[1] * 0.85).min(1.0),
                    (s.color[2] * 0.85).min(1.0),
                ]);
            }
        }
    }

    let grid = kami_nerf::DensityGrid::new(
        data,
        [res, res, res],
        Vec3::splat(-bounds),
        Vec3::splat(bounds),
    )
    .with_colors(colors);

    if volume_type == "dense" {
        // NeRF smooth mesh with per-vertex colors from density field
        let (mesh, vertex_colors) = kami_mesher::sdf_to_colored_mesh(
            |x, y, z| {
                let density = grid.sample(Vec3::new(x, y, z));
                let d = 0.5 - density; // threshold at 0.5
                let [r, g, b] = grid.sample_color(Vec3::new(x, y, z));
                (d, [r, g, b, 1.0])
            },
            res,
            bounds,
        );
        let meshes = kami_mesher::split_mesh_by_color(&mesh, &vertex_colors);
        log::info!("NeRF/colored — res={} groups={}", res, meshes.len());
        return render_multi_mesh_orbit(canvas_id, meshes).await;
    }

    let dense = grid.to_volume(res, 0.5);
    let volume = convert_volume(dense, res, volume_type);
    let scale = bounds * 2.0 / res as f32;
    render_volume_smooth_orbit(canvas_id, &volume, bounds, res, [0.34, 0.80, 0.01, 1.0]).await
}

/// Render multiple colored meshes in orbit mode (multi-material).
async fn render_multi_mesh_orbit(
    canvas_id: &str,
    meshes: Vec<(kami_render::mesh::LoadedMesh, [f32; 4])>,
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;
    let mut camera = Camera::new(width as f32 / height as f32);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(Vec3::new(-0.3, -1.5, -0.8), Vec3::ONE, 2.5);
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );
    let fallback = FallbackTextures {
        white: texture::default_white_texture(&device, &queue),
        normal: texture::default_normal_texture(&device, &queue),
        mr: texture::default_mr_texture(&device, &queue),
    };

    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut batches = Vec::new();
    for (mesh, color) in &meshes {
        if mesh.vertex_count == 0 {
            continue;
        }
        let mat = MaterialUniform {
            albedo: *color,
            metallic: 0.0,
            roughness: 0.5,
            has_albedo_tex: 0,
            has_normal_tex: 0,
            ..Default::default()
        };
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let inst = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&identity),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&mat),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mbg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: mb.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&fallback.white.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&fallback.white.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&fallback.normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&fallback.normal.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&fallback.mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&fallback.mr.sampler),
                },
            ],
        });
        batches.push(DrawBatch {
            vertex_buffer: vb,
            index_buffer: ib,
            instance_buffer: inst,
            _material_buffer: mb,
            material_bind_group: mbg,
            index_count: mesh.index_count,
            instance_count: 1,
        });
    }

    batches.push(create_ground_batch(&device, &queue, &material_layout));
    if batches.is_empty() {
        return Err("no meshes".into());
    }

    let time = Arc::new(std::sync::Mutex::new(0.0f32));
    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let tc = time.clone();
    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut t = tc.lock().unwrap();
        *t += 1.0 / 60.0;
        camera.orbit(*t * 0.15, 0.4, 12.0);
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));
        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));
    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

/// Mesh YORO parts individually with correct colors.
fn yoro_multi_mesh(res: u32, bounds: f32) -> Vec<(kami_render::mesh::LoadedMesh, [f32; 4])> {
    yoro_parts()
        .into_iter()
        .map(|(sdf_node, color)| {
            let mesh = kami_mesher::sdf_to_mesh(
                |x, y, z| {
                    let s = sdf_node.sample(Vec3::new(x, y, z));
                    (s.distance, s.color)
                },
                res,
                bounds,
            );
            (mesh, color)
        })
        .collect()
}

/// Render a LoadedMesh directly in orbit mode.
async fn render_mesh_orbit(
    canvas_id: &str,
    mesh: kami_render::mesh::LoadedMesh,
    color: [f32; 4],
) -> Result<(), JsValue> {
    if mesh.vertex_count == 0 {
        return Err("empty mesh".into());
    }
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;
    let mut camera = Camera::new(width as f32 / height as f32);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light = LightUniform::directional(Vec3::new(-0.3, -1.5, -0.8), Vec3::ONE, 2.5);
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );
    let fallback = FallbackTextures {
        white: texture::default_white_texture(&device, &queue),
        normal: texture::default_normal_texture(&device, &queue),
        mr: texture::default_mr_texture(&device, &queue),
    };
    let mat = MaterialUniform {
        albedo: color,
        metallic: 0.0,
        roughness: 0.5,
        has_albedo_tex: 0,
        has_normal_tex: 0,
        ..Default::default()
    };
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let inst = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&identity),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let mb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&mat),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mbg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &material_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: mb.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&fallback.white.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&fallback.white.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&fallback.normal.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&fallback.normal.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&fallback.mr.view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&fallback.mr.sampler),
            },
        ],
    });
    let ground = create_ground_batch(&device, &queue, &material_layout);
    let batches = vec![
        DrawBatch {
            vertex_buffer: vb,
            index_buffer: ib,
            instance_buffer: inst,
            _material_buffer: mb,
            material_bind_group: mbg,
            index_count: mesh.index_count,
            instance_count: 1,
        },
        ground,
    ];

    let time = Arc::new(std::sync::Mutex::new(0.0f32));
    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let tc = time.clone();
    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut t = tc.lock().unwrap();
        *t += 1.0 / 60.0;
        camera.orbit(*t * 0.15, 0.4, 12.0);
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));
        render_frame_batches(
            &surface,
            &device,
            &queue,
            &pbr_pipeline,
            &camera_light_bg,
            &shadow_bg,
            &depth_view,
            &batches,
        );
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));
    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    Ok(())
}

/// Extract top-level children from SDF tree for per-part coloring.
fn extract_sdf_children(node: &kami_sdf::SdfNode) -> Vec<(kami_sdf::SdfNode, [f32; 4])> {
    match node {
        kami_sdf::SdfNode::Union(children) | kami_sdf::SdfNode::SmoothUnion { children, .. } => {
            children
                .iter()
                .map(|child| {
                    let color = get_prim_color(child);
                    (child.clone(), color)
                })
                .collect()
        }
        _ => {
            let color = get_prim_color(node);
            vec![(node.clone(), color)]
        }
    }
}

fn get_prim_color(node: &kami_sdf::SdfNode) -> [f32; 4] {
    match node {
        kami_sdf::SdfNode::Primitive { color, .. } => *color,
        kami_sdf::SdfNode::Union(c) | kami_sdf::SdfNode::SmoothUnion { children: c, .. } => c
            .first()
            .map(|ch| get_prim_color(ch))
            .unwrap_or([0.5, 0.5, 0.5, 1.0]),
        kami_sdf::SdfNode::Difference { base, .. } => get_prim_color(base),
        kami_sdf::SdfNode::Intersection { a, .. } => get_prim_color(a),
        _ => [0.5, 0.5, 0.5, 1.0],
    }
}

fn convert_volume(
    dense: kami_voxel::VoxelVolume,
    res: u32,
    volume_type: &str,
) -> kami_voxel::VoxelVolume {
    match volume_type {
        "sparse" => dense.to_sparse(),
        "octree" => {
            let size = res.next_power_of_two();
            let mut oct = kami_voxel::VoxelVolume::new_octree(size);
            for z in 0..res {
                for y in 0..res {
                    for x in 0..res {
                        let v = dense.get(x, y, z);
                        if v.is_solid() {
                            oct.set(x, y, z, v);
                        }
                    }
                }
            }
            oct
        }
        _ => dense,
    }
}

/// Render a volume using smooth MC by converting voxels back to a distance-like field.
/// This makes Sparse/Octree look smooth like Dense, while demonstrating volume storage.
async fn render_volume_smooth_orbit(
    canvas_id: &str,
    volume: &kami_voxel::VoxelVolume,
    bounds: f32,
    res: u32,
    color: [f32; 4],
) -> Result<(), JsValue> {
    let step = bounds * 2.0 / res as f32;
    let origin = -bounds;
    // Convert volume to a distance-like function: solid=-1, air=+1
    let mesh = kami_mesher::sdf_to_mesh(
        |x, y, z| {
            let gx = ((x - origin) / step).floor() as u32;
            let gy = ((y - origin) / step).floor() as u32;
            let gz = ((z - origin) / step).floor() as u32;
            let v = volume.get(gx.min(res - 1), gy.min(res - 1), gz.min(res - 1));
            let d = if v.is_solid() { -0.5 } else { 0.5 };
            (d, if v.is_solid() { v.color } else { color })
        },
        res,
        bounds,
    );
    if mesh.vertex_count == 0 {
        // Fallback to block mesh
        let block_mesh = kami_mesher::marching_cubes(volume, step);
        return render_mesh_orbit(canvas_id, block_mesh, color).await;
    }
    render_mesh_orbit(canvas_id, mesh, color).await
}

fn create_render_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    camera_buffer: &wgpu::Buffer,
    light_buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
) -> (
    wgpu::BindGroupLayout,
    wgpu::BindGroupLayout,
    wgpu::BindGroupLayout,
    wgpu::BindGroup,
    wgpu::BindGroup,
    wgpu::TextureView,
    wgpu::RenderPipeline,
) {
    let camera_light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    let material_layout = pipeline::textured_material_layout(device);

    let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
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
    let shadow_view = shadow_texture.create_view(&Default::default());
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shadow-sampler"),
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

    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
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
    let depth_view = depth_texture.create_view(&Default::default());

    let pbr_pipeline = pipeline::create_pbr_pipeline(
        device,
        format,
        &camera_light_layout,
        &material_layout,
        &shadow_layout,
    );

    (
        camera_light_layout,
        material_layout,
        shadow_layout,
        camera_light_bg,
        shadow_bg,
        depth_view,
        pbr_pipeline,
    )
}

/// DDA (Digital Differential Analyzer) voxel raycast through VoxelWorld.
/// Returns `Some((hit_x, hit_y, hit_z, normal_x, normal_y, normal_z))` for the first solid
/// block along the ray, or `None` if no block within `max_dist`.
fn dda_raycast(
    world: &VoxelWorld,
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
) -> Option<(i32, i32, i32, i32, i32, i32)> {
    if dir.length_squared() < 1e-10 {
        return None;
    }
    let dir = dir.normalize();

    // Current voxel coordinates (floor).
    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    // Step direction (+1 or -1) for each axis.
    let step_x: i32 = if dir.x > 0.0 { 1 } else { -1 };
    let step_y: i32 = if dir.y > 0.0 { 1 } else { -1 };
    let step_z: i32 = if dir.z > 0.0 { 1 } else { -1 };

    // t_max: distance along ray to next voxel boundary on each axis.
    let t_max_x = if dir.x.abs() > 1e-10 {
        let boundary = if dir.x > 0.0 {
            (x + 1) as f32
        } else {
            x as f32
        };
        (boundary - origin.x) / dir.x
    } else {
        f32::INFINITY
    };
    let t_max_y = if dir.y.abs() > 1e-10 {
        let boundary = if dir.y > 0.0 {
            (y + 1) as f32
        } else {
            y as f32
        };
        (boundary - origin.y) / dir.y
    } else {
        f32::INFINITY
    };
    let t_max_z = if dir.z.abs() > 1e-10 {
        let boundary = if dir.z > 0.0 {
            (z + 1) as f32
        } else {
            z as f32
        };
        (boundary - origin.z) / dir.z
    } else {
        f32::INFINITY
    };

    let mut t_max = [t_max_x, t_max_y, t_max_z];
    // t_delta: distance along ray to traverse one full voxel on each axis.
    let t_delta_x = if dir.x.abs() > 1e-10 {
        (1.0 / dir.x).abs()
    } else {
        f32::INFINITY
    };
    let t_delta_y = if dir.y.abs() > 1e-10 {
        (1.0 / dir.y).abs()
    } else {
        f32::INFINITY
    };
    let t_delta_z = if dir.z.abs() > 1e-10 {
        (1.0 / dir.z).abs()
    } else {
        f32::INFINITY
    };
    let t_delta = [t_delta_x, t_delta_y, t_delta_z];

    let mut normal = (0i32, 0i32, 0i32);
    let max_steps = (max_dist * 3.0) as usize + 1;

    for _ in 0..max_steps {
        // Check current voxel.
        let block = world.get_block(x, y, z);
        if block.is_solid() {
            return Some((x, y, z, normal.0, normal.1, normal.2));
        }

        // Advance along the axis with smallest t_max.
        if t_max[0] < t_max[1] && t_max[0] < t_max[2] {
            if t_max[0] > max_dist {
                return None;
            }
            x += step_x;
            t_max[0] += t_delta[0];
            normal = (-step_x, 0, 0);
        } else if t_max[1] < t_max[2] {
            if t_max[1] > max_dist {
                return None;
            }
            y += step_y;
            t_max[1] += t_delta[1];
            normal = (0, -step_y, 0);
        } else {
            if t_max[2] > max_dist {
                return None;
            }
            z += step_z;
            t_max[2] += t_delta[2];
            normal = (0, 0, -step_z);
        }
    }
    None
}

/// Re-mesh a single chunk after block edit and upload new vertex/index data to GPU.
/// Keeps all 12 floats/vertex (pos3+norm3+uv2+color4) for per-vertex color pipeline.
fn remesh_chunk(
    world: &VoxelWorld,
    chunk_key: [i32; 3],
    queue: &wgpu::Queue,
    voxel_batches: &Rc<RefCell<Vec<DrawBatch>>>,
    voxel_batch_map: &Rc<RefCell<std::collections::HashMap<[i32; 3], usize>>>,
) {
    let bm = voxel_batch_map.borrow();
    if let Some(&batch_idx) = bm.get(&chunk_key) {
        if let Some(chunk) = world.chunks.get(&chunk_key) {
            let cs = CHUNK_SIZE as i32;
            let nb = build_chunk_neighbors(world, chunk_key);
            let mut vm = voxel_mesh::greedy_mesh_with_neighbors(chunk, &world.palette, &nb);
            vm.offset_positions([
                (chunk_key[0] * cs) as f32,
                (chunk_key[1] * cs) as f32,
                (chunk_key[2] * cs) as f32,
            ]);
            let mut vb = voxel_batches.borrow_mut();
            if let Some(batch) = vb.get_mut(batch_idx) {
                queue.write_buffer(&batch.vertex_buffer, 0, bytemuck::cast_slice(&vm.vertices));
                queue.write_buffer(&batch.index_buffer, 0, bytemuck::cast_slice(&vm.indices));
                batch.index_count = vm.index_count;
            }
        }
    }
}

/// Build neighbor boundary data for a chunk from VoxelWorld.
fn build_chunk_neighbors(world: &VoxelWorld, key: [i32; 3]) -> voxel_mesh::ChunkNeighbors {
    use kami_game::voxel::CHUNK_SIZE;
    let s = CHUNK_SIZE;
    let mut nb = voxel_mesh::ChunkNeighbors::default();

    let extract_slice =
        |nk: [i32; 3], axis: usize, d: usize| -> Option<[BlockType; CHUNK_SIZE * CHUNK_SIZE]> {
            world.chunks.get(&nk).map(|c| {
                let mut slice = [BlockType::Air; CHUNK_SIZE * CHUNK_SIZE];
                let (u_axis, v_axis) = match axis {
                    0 => (1, 2),
                    1 => (0, 2),
                    _ => (0, 1),
                };
                for v in 0..s {
                    for u in 0..s {
                        let mut pos = [0usize; 3];
                        pos[axis] = d;
                        pos[u_axis] = u;
                        pos[v_axis] = v;
                        slice[v * s + u] = c.get(pos[0], pos[1], pos[2]);
                    }
                }
                slice
            })
        };

    // Horizontal neighbors: if no neighbor chunk exists at world edge, treat as solid
    // to suppress side faces at world boundary (cleaner edge).
    let solid_boundary = Some([BlockType::Stone; CHUNK_SIZE * CHUNK_SIZE]);
    nb.pos_x = extract_slice([key[0] + 1, key[1], key[2]], 0, 0).or_else(|| solid_boundary.clone());
    nb.neg_x =
        extract_slice([key[0] - 1, key[1], key[2]], 0, s - 1).or_else(|| solid_boundary.clone());
    nb.pos_y = extract_slice([key[0], key[1] + 1, key[2]], 1, 0); // above = Air if missing (correct)
    // Below (neg_y): if no neighbor exists, treat as all-solid (suppress bedrock bottom face).
    nb.neg_y = extract_slice([key[0], key[1] - 1, key[2]], 1, s - 1)
        .or_else(|| Some([BlockType::Stone; CHUNK_SIZE * CHUNK_SIZE]));
    nb.pos_z = extract_slice([key[0], key[1], key[2] + 1], 2, 0).or_else(|| solid_boundary.clone());
    nb.neg_z = extract_slice([key[0], key[1], key[2] - 1], 2, s - 1).or_else(|| solid_boundary);
    nb
}

/// Re-mesh a single chunk at a specific LOD level and upload to GPU.
/// Keeps all 12 floats/vertex (pos3+norm3+uv2+color4) for per-vertex color pipeline.
fn remesh_chunk_lod(
    world: &VoxelWorld,
    chunk_key: [i32; 3],
    lod_level: u32,
    queue: &wgpu::Queue,
    voxel_batches: &Rc<RefCell<Vec<DrawBatch>>>,
    voxel_batch_map: &Rc<RefCell<std::collections::HashMap<[i32; 3], usize>>>,
) {
    let bm = voxel_batch_map.borrow();
    if let Some(&batch_idx) = bm.get(&chunk_key) {
        if let Some(chunk) = world.chunks.get(&chunk_key) {
            let cs = CHUNK_SIZE as i32;
            let nb = build_chunk_neighbors(world, chunk_key);
            let mut vm = voxel_mesh::greedy_mesh_with_neighbors(chunk, &world.palette, &nb);
            vm.offset_positions([
                (chunk_key[0] * cs) as f32,
                (chunk_key[1] * cs) as f32,
                (chunk_key[2] * cs) as f32,
            ]);
            let mut vb = voxel_batches.borrow_mut();
            if let Some(batch) = vb.get_mut(batch_idx) {
                queue.write_buffer(&batch.vertex_buffer, 0, bytemuck::cast_slice(&vm.vertices));
                queue.write_buffer(&batch.index_buffer, 0, bytemuck::cast_slice(&vm.indices));
                batch.index_count = vm.index_count;
            }
        }
    }
}

/// Render frame from a slice of DrawBatch references (mixed static + voxel batches).
fn render_frame_batches_ref(
    surface: &wgpu::Surface,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    camera_light_bg: &wgpu::BindGroup,
    shadow_bg: &wgpu::BindGroup,
    depth_view: &wgpu::TextureView,
    batches: &[&DrawBatch],
) {
    render_frame_batches_sky(
        surface,
        device,
        queue,
        pipeline,
        camera_light_bg,
        shadow_bg,
        depth_view,
        batches,
        [0.94, 0.92, 0.84],
    );
}

/// Render frame with custom sky/clear color.
fn render_frame_batches_sky(
    surface: &wgpu::Surface,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    camera_light_bg: &wgpu::BindGroup,
    shadow_bg: &wgpu::BindGroup,
    depth_view: &wgpu::TextureView,
    batches: &[&DrawBatch],
    sky_color: [f32; 3],
) {
    if let Ok(frame) = surface.get_current_texture() {
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pbr"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: sky_color[0] as f64,
                            g: sky_color[1] as f64,
                            b: sky_color[2] as f64,
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
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, camera_light_bg, &[]);
            pass.set_bind_group(2, shadow_bg, &[]);

            for batch in batches {
                pass.set_bind_group(1, &batch.material_bind_group, &[]);
                pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..batch.index_count, 0, 0..batch.instance_count);
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

/// Render frame with two pipelines in a single render pass: standard PBR for static
/// batches (8 floats/vertex), then per-vertex color PBR for voxel batches (12 floats/vertex).
/// Pipeline switching within a render pass is the standard wgpu approach.
fn render_frame_dual_pipeline(
    surface: &wgpu::Surface,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    static_pipeline: &wgpu::RenderPipeline,
    color_pipeline: &wgpu::RenderPipeline,
    camera_light_bg: &wgpu::BindGroup,
    shadow_bg: &wgpu::BindGroup,
    depth_view: &wgpu::TextureView,
    static_batches: &[&DrawBatch],
    voxel_batches: &[&DrawBatch],
    sky_color: [f32; 3],
) {
    if let Ok(frame) = surface.get_current_texture() {
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pbr-dual"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: sky_color[0] as f64,
                            g: sky_color[1] as f64,
                            b: sky_color[2] as f64,
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
                ..Default::default()
            });

            // Static batches: standard PBR pipeline (8 floats/vertex, material uniform albedo).
            pass.set_pipeline(static_pipeline);
            pass.set_bind_group(0, camera_light_bg, &[]);
            pass.set_bind_group(2, shadow_bg, &[]);
            for batch in static_batches {
                pass.set_bind_group(1, &batch.material_bind_group, &[]);
                pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..batch.index_count, 0, 0..batch.instance_count);
            }

            // Voxel batches: per-vertex color PBR pipeline (12 floats/vertex, color from vertex).
            if !voxel_batches.is_empty() {
                pass.set_pipeline(color_pipeline);
                pass.set_bind_group(0, camera_light_bg, &[]);
                pass.set_bind_group(2, shadow_bg, &[]);
                for batch in voxel_batches {
                    pass.set_bind_group(1, &batch.material_bind_group, &[]);
                    pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                    pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..batch.index_count, 0, 0..batch.instance_count);
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

fn render_frame_batches(
    surface: &wgpu::Surface,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    camera_light_bg: &wgpu::BindGroup,
    shadow_bg: &wgpu::BindGroup,
    depth_view: &wgpu::TextureView,
    batches: &[DrawBatch],
) {
    if let Ok(frame) = surface.get_current_texture() {
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pbr"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.94,
                            g: 0.92,
                            b: 0.84,
                            a: 1.0,
                        }), // Nintendo cream #f0ead6
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
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, camera_light_bg, &[]);
            pass.set_bind_group(2, shadow_bg, &[]);

            for batch in batches {
                pass.set_bind_group(1, &batch.material_bind_group, &[]);
                pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..batch.index_count, 0, 0..batch.instance_count);
            }
        }
        // Pass 2 (future): Gaussian Splatting — alpha-blend after PBR.
        // When gaussian-splat feature is enabled and scene contains MeshRef::GaussianSplat:
        //   1. splat_pipeline.dispatch_distances(encoder, sort_bg, count) — compute pass
        //   2. CPU-side sort of sort_entries (or GPU bitonic sort)
        //   3. Begin render pass (load=Load, depth_write=false, blend=premultiplied alpha)
        //   4. splat_pipeline.render_pipeline, set camera + splat bind groups
        //   5. draw(0..4, 0..splat_count) — triangle strip billboard per splat

        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

/// Character Maker: CharacterDef JSON → parametric mesh → direct GPU upload → PBR render.
///
/// Unlike run_embed (which uses IslandScene), this directly uploads kami-character mesh parts
/// as wgpu vertex/index buffers with per-material MaterialUniform bind groups.
/// Renders with the existing PBR shader (SSS skin, clearcoat eyes, anisotropic hair).
#[wasm_bindgen]
pub async fn run_with_character(canvas_id: &str, character_json: &str) -> Result<(), JsValue> {
    log::info!("run_with_character: generating mesh from CharacterDef");
    let def: CharacterDef = serde_json::from_str(character_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid CharacterDef JSON: {e}")))?;

    let char_mesh = kami_character::generate_character(&def);
    let total_verts: usize = char_mesh.parts.iter().map(|p| p.vertices.len()).sum();
    let total_tris: usize = char_mesh.parts.iter().map(|p| p.indices.len() / 3).sum();
    log::info!(
        "Character: {} parts, {} verts, {} tris",
        char_mesh.parts.len(),
        total_verts,
        total_tris
    );

    // Get canvas and init GPU
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;

    // Create render resources (pipeline, bind group layouts, depth buffer, shadow map)
    let camera = Camera::new(width as f32 / height as f32);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("char_camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light =
        LightUniform::directional(Vec3::new(-0.5, -1.2, -0.8), Vec3::new(1.0, 0.95, 0.9), 1.5);
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("char_light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (_, material_layout, _, camera_light_bg, shadow_bg, depth_view, pbr_pipeline) =
        create_render_resources(
            &device,
            format,
            &camera_buffer,
            &light_buffer,
            width,
            height,
        );

    // Create fallback textures for material bind groups
    let fallback_albedo = texture::default_white_texture(&device, &queue);
    let fallback_normal = texture::default_normal_texture(&device, &queue);
    let fallback_mr = texture::default_mr_texture(&device, &queue);
    let sampler = &fallback_albedo.sampler;

    // Build per-material draw batches: vertex buffer + index buffer + material bind group
    struct DrawBatch {
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
        index_count: u32,
        instance_buffer: wgpu::Buffer,
        material_bg: wgpu::BindGroup,
    }

    let mut batches: Vec<DrawBatch> = Vec::new();

    for part in &char_mesh.parts {
        if part.vertices.is_empty() || part.indices.is_empty() {
            continue;
        }

        // Interleave vertex data: pos3 + norm3 + uv2 = 32 bytes per vertex
        let mut vertex_data: Vec<f32> = Vec::with_capacity(part.vertices.len() * 8);
        for v in &part.vertices {
            vertex_data.extend_from_slice(&[v.position.x, v.position.y, v.position.z]);
            vertex_data.extend_from_slice(&[v.normal.x, v.normal.y, v.normal.z]);
            vertex_data.extend_from_slice(&v.uv);
        }

        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("vb_{}", part.name)),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("ib_{}", part.name)),
            contents: bytemuck::cast_slice(&part.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Instance buffer (identity transform)
        let instance_data = glam::Mat4::IDENTITY.to_cols_array();
        let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("inst_{}", part.name)),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Material uniform from kami-character PbrMaterial
        let pbr = kami_character::material::PbrMaterial::for_part(
            part.material,
            &def.skin,
            &def.eyes,
            &def.mouth,
            &def.hair,
            &def.clothing,
        );
        let mat_uniform = MaterialUniform {
            albedo: pbr.base_color,
            metallic: pbr.metallic,
            roughness: pbr.roughness,
            has_albedo_tex: 0,
            has_normal_tex: 0,
            subsurface_color: [
                pbr.subsurface_color[0],
                pbr.subsurface_color[1],
                pbr.subsurface_color[2],
                pbr.subsurface,
            ],
            subsurface_radius: [0.012, 0.036, 0.12],
            sss_model: if pbr.subsurface > 0.0 { 1 } else { 0 },
            aniso_tangent: [1.0, 0.0, 0.0],
            aniso_strength: pbr.anisotropic,
            hair_scatter: [0.8, 0.6, 0.4, 0.3],
            clearcoat: pbr.clearcoat,
            clearcoat_roughness: pbr.clearcoat_roughness,
            emission: pbr.emission,
            tex_flags: 0,
            parallax_depth: 0.0,
            _pad: 0.0,
        };
        let mat_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("mat_{}", part.name)),
            contents: bytemuck::bytes_of(&mat_uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let mat_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("matbg_{}", part.name)),
            layout: &material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: mat_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&fallback_albedo.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&fallback_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&fallback_mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        batches.push(DrawBatch {
            vertex_buffer: vb,
            index_buffer: ib,
            index_count: part.indices.len() as u32,
            instance_buffer: inst_buf,
            material_bg: mat_bg,
        });
    }

    log::info!("Created {} draw batches for PBR rendering", batches.len());

    // Wrap batches in Arc for closure sharing
    let batches = Arc::new(batches);

    // Render loop (orbit camera)
    let mut camera = Camera::new(width as f32 / height as f32);
    camera.target = Vec3::new(0.0, 0.0, 0.0);

    let time = Arc::new(std::sync::Mutex::new(0.0f32));
    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let time_clone = time.clone();

    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut t = time_clone.lock().unwrap();
        *t += 1.0 / 60.0;

        // Orbit camera
        let breath = (*t * 1.2).sin() * 0.005;
        camera.orbit(*t * 0.15, 0.2 + breath, 0.5);
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));

        // Render
        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                let w = web_sys::window().unwrap();
                let cb = f.lock().unwrap();
                w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
                    .unwrap();
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("char_pbr"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.04,
                            b: 0.07,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_pipeline(&pbr_pipeline);
            pass.set_bind_group(0, &camera_light_bg, &[]);
            pass.set_bind_group(2, &shadow_bg, &[]);

            for batch in batches.iter() {
                pass.set_bind_group(1, &batch.material_bg, &[]);
                pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..batch.index_count, 0, 0..1);
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();

    log::info!("Character PBR render loop started");
    Ok(())
}

/// Load VRM/GLB from URL and render with PBR pipeline.
/// Fetches GLB binary via JS fetch(), parses with gltf_loader, renders all primitives.
#[wasm_bindgen]
pub async fn run_embed_vrm(canvas_id: &str, vrm_url: &str) -> Result<(), JsValue> {
    log::info!("run_embed_vrm: loading VRM from {}", vrm_url);

    // Fetch GLB binary from URL
    let window = web_sys::window().ok_or("no window")?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(vrm_url)).await?;
    let resp: web_sys::Response = resp_value.dyn_into()?;
    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "fetch failed: {}",
            resp.status()
        )));
    }
    let buf = wasm_bindgen_futures::JsFuture::from(resp.array_buffer()?).await?;
    let uint8 = js_sys::Uint8Array::new(&buf);
    let glb_data = uint8.to_vec();
    log::info!("VRM data: {} bytes", glb_data.len());

    // Parse GLB
    let scene = kami_render::gltf_loader::load_glb(&glb_data)
        .map_err(|e| JsValue::from_str(&format!("GLB parse error: {e}")))?;
    log::info!(
        "Parsed: {} meshes, {} materials, {} nodes, {} textures",
        scene.meshes.len(),
        scene.materials.len(),
        scene.nodes.len(),
        scene.textures.len()
    );

    // Init GPU
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let (device, queue, surface, _config, format, width, height) = init_gpu(&canvas).await?;

    // Create render resources
    let camera = Camera::new(width as f32 / height as f32);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vrm_camera"),
        contents: bytemuck::bytes_of(&camera.uniform()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let light =
        LightUniform::directional(Vec3::new(-0.5, -1.2, -0.8), Vec3::new(1.0, 0.95, 0.9), 1.5);
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vrm_light"),
        contents: bytemuck::bytes_of(&light),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (
        camera_light_layout_ref,
        material_layout,
        shadow_layout_ref,
        camera_light_bg,
        shadow_bg,
        depth_view,
        pbr_pipeline,
    ) = create_render_resources(
        &device,
        format,
        &camera_buffer,
        &light_buffer,
        width,
        height,
    );

    // Create MToon pipeline (same bind group layout, different shader)
    let mtoon_pipeline = pipeline::create_mtoon_pipeline(
        &device,
        format,
        &camera_light_layout_ref,
        &material_layout,
        &shadow_layout_ref,
    );

    // GPU skinning: create bone palette layout + skinned pipeline + initial identity
    // palette buffer. VRM is always skinned; fallback to non-skinned only if scene
    // has no skin (non-conformant VRM).
    let use_skinned = !scene.skins.is_empty() && !scene.skin_joints.is_empty();
    let bone_layout = pipeline::create_bone_palette_layout(&device);
    let morph_layout = pipeline::create_morph_layout(&device);
    let skinned_mtoon_pipeline = pipeline::create_skinned_mtoon_pipeline(
        &device,
        format,
        &camera_light_layout_ref,
        &material_layout,
        &shadow_layout_ref,
        &bone_layout,
        &morph_layout,
    );
    let joint_count = scene
        .skins
        .first()
        .map(|s| s.joint_node_indices.len())
        .unwrap_or(1);
    // Initial palette: identity matrices (bind pose — visually equivalent to
    // non-skinned path). L4 will compute real joint_matrices from Skeleton.
    let identity_mat: [f32; 16] = glam::Mat4::IDENTITY.to_cols_array();
    let bone_palette_bytes: Vec<u8> = (0..joint_count)
        .flat_map(|_| identity_mat.iter().flat_map(|f| f.to_le_bytes()))
        .collect();
    let bone_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vrm_bone_palette"),
        contents: &bone_palette_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let bone_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vrm_bone_bg"),
        layout: &bone_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: bone_buffer.as_entire_binding(),
        }],
    });
    log::info!(
        "VRM skinning: use_skinned={}, joint_count={}",
        use_skinned,
        joint_count
    );

    // Upload textures to GPU
    let fallback_albedo = texture::default_white_texture(&device, &queue);
    let fallback_normal = texture::default_normal_texture(&device, &queue);
    let fallback_mr = texture::default_mr_texture(&device, &queue);

    let mut gpu_textures: Vec<texture::GpuTexture> = Vec::new();
    for tex in &scene.textures {
        let gpu = texture::create_texture(
            &device,
            &queue,
            &tex.pixels,
            tex.width,
            tex.height,
            "vrm_tex",
            true,
        );
        gpu_textures.push(gpu);
    }

    // Build draw batches
    struct VrmBatch {
        vertex_buffer: wgpu::Buffer,
        index_buffer: wgpu::Buffer,
        index_count: u32,
        instance_buffer: wgpu::Buffer,
        material_bg: wgpu::BindGroup,
        is_mtoon: bool,
        /// Per-mesh morph bind group (group 4: deltas storage + info uniform).
        /// Present only on the skinned path; `None` batches get the shared dummy.
        morph_bg: Option<wgpu::BindGroup>,
        /// Uniform buffer holding MorphInfo { target_count, vertex_count, pad, pad, weights[16] }.
        /// weights region is updated each frame when VRM_MORPH_STATE is dirty.
        morph_info_buffer: Option<wgpu::Buffer>,
        /// Human-readable label `"{mesh_name}:{material_name}"` for part selection.
        label: String,
        /// Original node transform (bind pose). Per-frame root TRS is composed
        /// on top of this for walk/run/jump locomotion (scene 12 ISEKAI mode).
        base_transform: glam::Mat4,
    }

    let sampler = &fallback_albedo.sampler;
    let mut batches: Vec<VrmBatch> = Vec::new();

    for node in &scene.nodes {
        let mesh = &scene.meshes[node.mesh_index];
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            continue;
        }

        // Build vertex buffer. If skinned path active, de-interleave the 32B
        // layout and pack into 56B skinned layout with per-mesh joints/weights.
        let vb = if use_skinned {
            let v = &mesh.vertices;
            let vcount = v.len() / 8;
            let mut pos = Vec::with_capacity(vcount * 3);
            let mut nrm = Vec::with_capacity(vcount * 3);
            let mut uv = Vec::with_capacity(vcount * 2);
            for i in 0..vcount {
                pos.extend_from_slice(&v[i * 8..i * 8 + 3]);
                nrm.extend_from_slice(&v[i * 8 + 3..i * 8 + 6]);
                uv.extend_from_slice(&v[i * 8 + 6..i * 8 + 8]);
            }
            let empty: Vec<u16> = Vec::new();
            let empty_f: Vec<f32> = Vec::new();
            let joints = scene.skin_joints.get(node.mesh_index).unwrap_or(&empty);
            let weights = scene.skin_weights.get(node.mesh_index).unwrap_or(&empty_f);
            let bytes = kami_render::mesh::interleave_skinned(&pos, &nrm, &uv, joints, weights);
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vrm_vb_skinned"),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            })
        } else {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vrm_vb"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            })
        };
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vrm_ib"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let base_transform = node.transform;
        let inst_data = base_transform.to_cols_array();
        let inst = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vrm_inst"),
            contents: bytemuck::cast_slice(&inst_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let mat = &scene.materials[node.material_index];
        let mat_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vrm_mat"),
            contents: bytemuck::bytes_of(mat),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // Select albedo texture (if material has one)
        let albedo_view =
            if let Some(Some(tex_idx)) = scene.material_texture_map.get(node.material_index) {
                if let Some(gpu_tex) = gpu_textures.get(*tex_idx) {
                    &gpu_tex.view
                } else {
                    &fallback_albedo.view
                }
            } else {
                &fallback_albedo.view
            };

        let mat_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vrm_matbg"),
            layout: &material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: mat_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&fallback_normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&fallback_mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let is_mtoon = mat.sss_model == 99;

        // Build per-mesh morph resources (skinned path only).
        let (morph_bg, morph_info_buffer) = if use_skinned {
            let targets = &scene.morph_targets[node.mesh_index];
            let vert_count = (mesh.vertices.len() / 8) as u32;
            let target_count = targets.len() as u32;
            // Concatenated deltas: target_count × vert_count × 3 f32. Empty → 3 zero f32 dummy.
            let mut flat_deltas: Vec<f32> =
                Vec::with_capacity((target_count * vert_count * 3) as usize);
            for t in targets {
                // Pad target to vert_count × 3 in case extraction produced fewer (unlikely).
                let expected = (vert_count * 3) as usize;
                if t.position_deltas.len() >= expected {
                    flat_deltas.extend_from_slice(&t.position_deltas[..expected]);
                } else {
                    flat_deltas.extend_from_slice(&t.position_deltas);
                    flat_deltas.resize(
                        flat_deltas.len() + (expected - t.position_deltas.len()),
                        0.0,
                    );
                }
            }
            if flat_deltas.is_empty() {
                flat_deltas = vec![0.0; 3];
            }
            let delta_bytes: Vec<u8> = flat_deltas.iter().flat_map(|f| f.to_le_bytes()).collect();
            let delta_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vrm_morph_deltas"),
                contents: &delta_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
            // MorphInfo: 4×u32 header + 16 × vec4<f32> weights = 16 + 256 = 272 bytes, pad to 288.
            let mut info_bytes: Vec<u8> = Vec::with_capacity(16 + 256);
            info_bytes.extend_from_slice(&target_count.to_le_bytes());
            info_bytes.extend_from_slice(&vert_count.to_le_bytes());
            info_bytes.extend_from_slice(&0u32.to_le_bytes());
            info_bytes.extend_from_slice(&0u32.to_le_bytes());
            info_bytes.extend_from_slice(&[0u8; 256]); // 64 weights × 4 bytes
            let info_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vrm_morph_info"),
                contents: &info_bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vrm_morph_bg"),
                layout: &morph_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: delta_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: info_buf.as_entire_binding(),
                    },
                ],
            });
            // delta_buf kept alive via bind group; info_buf we hold so we can write weights.
            (Some(bg), Some(info_buf))
        } else {
            (None, None)
        };

        batches.push(VrmBatch {
            vertex_buffer: vb,
            index_buffer: ib,
            index_count: mesh.index_count,
            instance_buffer: inst,
            material_bg: mat_bg,
            is_mtoon,
            morph_bg,
            morph_info_buffer,
            label: node.label.clone(),
            base_transform,
        });
    }

    log::info!("VRM: {} draw batches ready", batches.len());
    // Populate label list for part composer visibility selection.
    VRM_PART_STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.labels = batches.iter().map(|b| b.label.clone()).collect();
        s.hidden.clear();
    });
    let batches = Arc::new(batches);

    // Render loop
    let mut camera = Camera::new(width as f32 / height as f32);
    camera.target = Vec3::new(0.0, 0.9, 0.0); // Upper body height

    // Interactive orbit state: yaw, pitch, distance, dragging, auto_rotate, last_x, last_y
    // Initial yaw=PI to face front of VRM (model is rotated 180°)
    let orbit_state: Rc<RefCell<(f32, f32, f32, bool, bool, f32, f32)>> = Rc::new(RefCell::new((
        std::f32::consts::PI,
        0.2f32,
        2.5f32,
        false,
        true,
        0.0,
        0.0,
    )));

    // Mouse events for orbit control
    {
        let os = orbit_state.clone();
        let md = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = os.borrow_mut();
            s.3 = true; // dragging
            s.4 = false; // stop auto-rotate
            s.5 = e.client_x() as f32;
            s.6 = e.client_y() as f32;
        });
        canvas
            .add_event_listener_with_callback("mousedown", md.as_ref().unchecked_ref())
            .ok();
        md.forget();
    }
    {
        let os = orbit_state.clone();
        let mm = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let mut s = os.borrow_mut();
            if !s.3 {
                return;
            }
            let dx = e.client_x() as f32 - s.5;
            let dy = e.client_y() as f32 - s.6;
            s.0 += dx * 0.005; // yaw
            s.1 = (s.1 - dy * 0.005).clamp(-1.2, 1.2); // pitch
            s.5 = e.client_x() as f32;
            s.6 = e.client_y() as f32;
        });
        canvas
            .add_event_listener_with_callback("mousemove", mm.as_ref().unchecked_ref())
            .ok();
        mm.forget();
    }
    {
        let os = orbit_state.clone();
        let mu = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |_: web_sys::MouseEvent| {
            os.borrow_mut().3 = false;
        });
        canvas
            .add_event_listener_with_callback("mouseup", mu.as_ref().unchecked_ref())
            .ok();
        mu.forget();
    }
    // Wheel zoom
    {
        let os = orbit_state.clone();
        let wh = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |e: web_sys::WheelEvent| {
            e.prevent_default();
            let mut s = os.borrow_mut();
            s.2 = (s.2 + e.delta_y() as f32 * 0.002).clamp(0.5, 6.0); // distance
        });
        canvas
            .add_event_listener_with_callback_and_add_event_listener_options(
                "wheel",
                wh.as_ref().unchecked_ref(),
                web_sys::AddEventListenerOptions::new().passive(false),
            )
            .ok();
        wh.forget();
    }
    // Touch events for mobile
    {
        let os = orbit_state.clone();
        let ts = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            if let Some(t) = e.touches().get(0) {
                let mut s = os.borrow_mut();
                s.3 = true;
                s.4 = false;
                s.5 = t.client_x() as f32;
                s.6 = t.client_y() as f32;
            }
        });
        canvas
            .add_event_listener_with_callback("touchstart", ts.as_ref().unchecked_ref())
            .ok();
        ts.forget();
    }
    {
        let os = orbit_state.clone();
        let tm = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |e: web_sys::TouchEvent| {
            if let Some(t) = e.touches().get(0) {
                let mut s = os.borrow_mut();
                if !s.3 {
                    return;
                }
                let dx = t.client_x() as f32 - s.5;
                let dy = t.client_y() as f32 - s.6;
                s.0 += dx * 0.005;
                s.1 = (s.1 - dy * 0.005).clamp(-1.2, 1.2);
                s.5 = t.client_x() as f32;
                s.6 = t.client_y() as f32;
            }
        });
        canvas
            .add_event_listener_with_callback("touchmove", tm.as_ref().unchecked_ref())
            .ok();
        tm.forget();
    }
    {
        let os = orbit_state.clone();
        let te = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |_: web_sys::TouchEvent| {
            os.borrow_mut().3 = false;
        });
        canvas
            .add_event_listener_with_callback("touchend", te.as_ref().unchecked_ref())
            .ok();
        te.forget();
    }

    // ── M1 locomotion: WASD → root translation, camera orbits VRM ──────────
    // State: (root_x, root_y, root_z, facing_yaw, key_w, key_a, key_s, key_d)
    // key_* are bool stored as f32 (1.0 / 0.0) for tuple homogeneity.
    let locomotion: Rc<RefCell<(f32, f32, f32, f32, bool, bool, bool, bool)>> = Rc::new(
        RefCell::new((0.0, 0.0, 0.0, 0.0, false, false, false, false)),
    );
    {
        let loc = locomotion.clone();
        let os = orbit_state.clone();
        let kd =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                let key = e.key().to_lowercase();
                let mut l = loc.borrow_mut();
                match key.as_str() {
                    "w" => {
                        l.4 = true;
                        os.borrow_mut().4 = false;
                    }
                    "a" => {
                        l.5 = true;
                        os.borrow_mut().4 = false;
                    }
                    "s" => {
                        l.6 = true;
                        os.borrow_mut().4 = false;
                    }
                    "d" => {
                        l.7 = true;
                        os.borrow_mut().4 = false;
                    }
                    _ => {}
                }
            });
        web_sys::window()
            .unwrap()
            .add_event_listener_with_callback("keydown", kd.as_ref().unchecked_ref())
            .ok();
        kd.forget();
    }
    {
        let loc = locomotion.clone();
        let ku =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                let key = e.key().to_lowercase();
                let mut l = loc.borrow_mut();
                match key.as_str() {
                    "w" => l.4 = false,
                    "a" => l.5 = false,
                    "s" => l.6 = false,
                    "d" => l.7 = false,
                    _ => {}
                }
            });
        web_sys::window()
            .unwrap()
            .add_event_listener_with_callback("keyup", ku.as_ref().unchecked_ref())
            .ok();
        ku.forget();
    }

    let time = Arc::new(std::sync::Mutex::new(0.0f32));
    let f: Arc<std::sync::Mutex<Option<Closure<dyn FnMut()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let g = f.clone();
    let time_clone = time.clone();
    let orbit_render = orbit_state.clone();
    let locomotion_render = locomotion.clone();

    *g.lock().unwrap() = Some(Closure::new(move || {
        let mut t = time_clone.lock().unwrap();
        *t += 1.0 / 60.0;

        // Read orbit state
        let (yaw, pitch, dist, _dragging, auto_rotate, _, _) = *orbit_render.borrow();
        let orbit_yaw = if auto_rotate { *t * 0.3 + yaw } else { yaw };

        // ── M1 locomotion: WASD → root position + facing ──
        let (root_pos, facing_yaw) = {
            let mut l = locomotion_render.borrow_mut();
            let dt = 1.0 / 60.0f32;
            let speed = 2.0f32;
            let mut local = glam::Vec3::ZERO;
            if l.4 {
                local.z -= 1.0;
            }
            if l.6 {
                local.z += 1.0;
            }
            if l.5 {
                local.x -= 1.0;
            }
            if l.7 {
                local.x += 1.0;
            }
            if local.length_squared() > 0.0 {
                local = local.normalize();
                // Camera looks from (+sin(yaw), *, +cos(yaw)) toward target. World forward
                // (away from camera, into screen) on XZ is therefore -(sin, 0, cos).
                let cam_fwd = glam::Vec3::new(-orbit_yaw.sin(), 0.0, -orbit_yaw.cos());
                let cam_right = glam::Vec3::new(orbit_yaw.cos(), 0.0, -orbit_yaw.sin());
                let world_move = (cam_fwd * (-local.z) + cam_right * local.x) * speed * dt;
                l.0 += world_move.x;
                l.2 += world_move.z;
                l.3 = (-world_move.x).atan2(-world_move.z);
            }
            (glam::Vec3::new(l.0, l.1, l.2), l.3)
        };

        camera.target = Vec3::new(root_pos.x, root_pos.y + 0.9, root_pos.z);
        camera.orbit(orbit_yaw, pitch, dist);
        queue.write_buffer(&camera_buffer, 0, bytemuck::bytes_of(&camera.uniform()));

        // Update each VRM batch instance transform with root TRS
        let root_tr =
            glam::Mat4::from_translation(root_pos) * glam::Mat4::from_rotation_y(facing_yaw);
        for batch in batches.iter() {
            let m = (root_tr * batch.base_transform).to_cols_array();
            queue.write_buffer(&batch.instance_buffer, 0, bytemuck::cast_slice(&m));
        }

        // L4/L6/L7: run spring + constraint sims every frame, then recompute
        // palette. Spring bones and constraints mark the palette dirty each
        // frame by nature (physics advances), so this block runs unconditionally
        // on the skinned path when a simulator is present.
        if use_skinned {
            VRM_SKIN_STATE.with(|state| {
                let mut s = state.borrow_mut();
                let has_sim = s.spring_sim.is_some() || s.constraint_solver.is_some();
                if !s.pose_dirty && !has_sim {
                    return;
                }
                let sk = match s.skeleton.as_ref() {
                    Some(sk) => sk.clone(),
                    None => return,
                };
                // Compute world transforms from USER pose overrides (pre-spring).
                let user_world = compute_world_transforms(&sk, &s.pose_overrides);
                let node_to_bone = s.node_to_bone.clone();

                // L6: step spring simulator.
                let mut spring_patches: Vec<(usize, [f32; 4])> = Vec::new();
                if let Some(sim) = s.spring_sim.as_mut() {
                    sim.step(
                        1.0 / 60.0,
                        |node_idx| {
                            node_to_bone
                                .get(&node_idx)
                                .and_then(|b| user_world.get(*b))
                                .copied()
                        },
                        &mut spring_patches,
                    );
                }

                // Merge spring patches into a working copy of overrides.
                let mut effective = s.pose_overrides.clone();
                for (node_idx, q) in &spring_patches {
                    if let Some(&bi) = node_to_bone.get(node_idx) {
                        if bi < effective.len() {
                            effective[bi] = Some(*q);
                        }
                    }
                }

                // L7: apply node constraints using the spring-patched world transforms.
                let post_spring_world = compute_world_transforms(&sk, &effective);
                let mut constraint_patches: Vec<(usize, [f32; 4])> = Vec::new();
                if let Some(solver) = s.constraint_solver.as_ref() {
                    let source_local_rot = |node_idx: usize| -> Option<glam::Quat> {
                        node_to_bone.get(&node_idx).and_then(|&bi| {
                            effective
                                .get(bi)
                                .and_then(|o| *o)
                                .map(glam::Quat::from_array)
                                .or_else(|| {
                                    sk.bones
                                        .get(bi)
                                        .map(|b| glam::Quat::from_array(b.local_rotation))
                                })
                        })
                    };
                    let world_lookup = |node_idx: usize| -> Option<glam::Mat4> {
                        node_to_bone
                            .get(&node_idx)
                            .and_then(|b| post_spring_world.get(*b))
                            .copied()
                    };
                    solver.apply(
                        source_local_rot,
                        world_lookup,
                        world_lookup,
                        &mut constraint_patches,
                    );
                }
                for (node_idx, q) in &constraint_patches {
                    if let Some(&bi) = node_to_bone.get(node_idx) {
                        if bi < effective.len() {
                            effective[bi] = Some(*q);
                        }
                    }
                }

                // Compute final palette from effective overrides and upload.
                let palette = compute_pose_palette(&sk, &effective);
                let bytes: Vec<u8> = palette
                    .iter()
                    .flat_map(|m| {
                        m.iter()
                            .flat_map(|col| col.iter().flat_map(|f| f.to_le_bytes()))
                    })
                    .collect();
                queue.write_buffer(&bone_buffer, 0, &bytes);
                s.pose_dirty = false;
            });
        }

        // Apply morph targets if dirty.
        // Skinned path (L5): write weights uniform to each batch's morph_info_buffer
        // (weights region starts at byte offset 16). Non-skinned legacy path: CPU blend.
        VRM_MORPH_STATE.with(|state| {
            let mut s = state.borrow_mut();
            if s.dirty && use_skinned {
                // Pack up to 64 weights into 16 × vec4<f32>.
                let mut weight_buf = [0f32; 64];
                for (i, w) in s.morph_weights.iter().take(64).enumerate() {
                    weight_buf[i] = *w;
                }
                let bytes: Vec<u8> = weight_buf.iter().flat_map(|f| f.to_le_bytes()).collect();
                for batch in batches.iter() {
                    if let Some(info_buf) = batch.morph_info_buffer.as_ref() {
                        queue.write_buffer(info_buf, 16, &bytes);
                    }
                }
                s.dirty = false;
                return;
            }
            if s.dirty && !use_skinned {
                let active_weights: Vec<(usize, f32)> = s
                    .morph_weights
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| w.abs() > 0.001)
                    .map(|(i, w)| (i, *w))
                    .collect();
                if !active_weights.is_empty() {
                    log::info!(
                        "Applying morphs: {:?}",
                        &active_weights[..active_weights.len().min(5)]
                    );
                }
                let mut applied = 0;
                for (batch_idx, batch) in batches.iter().enumerate() {
                    if batch_idx >= s.base_vertices.len() {
                        break;
                    }
                    let base = &s.base_vertices[batch_idx];
                    let deltas = &s.morph_deltas[batch_idx];
                    if deltas.is_empty() {
                        continue;
                    }

                    let mut morphed = base.clone();
                    let vert_count = base.len() / 8; // 8 floats per vertex (pos3+norm3+uv2)
                    let mut any_applied = false;

                    for (ti, delta) in deltas.iter().enumerate() {
                        if ti >= s.morph_weights.len() {
                            break;
                        }
                        let w = s.morph_weights[ti];
                        if w.abs() < 0.001 {
                            continue;
                        }
                        any_applied = true;
                        let delta_verts = delta.len() / 3;
                        let scale = 1.0;
                        for vi in 0..vert_count.min(delta_verts) {
                            morphed[vi * 8 + 0] += delta[vi * 3 + 0] * w * scale; // pos.x
                            morphed[vi * 8 + 1] += delta[vi * 3 + 1] * w * scale; // pos.y
                            morphed[vi * 8 + 2] += delta[vi * 3 + 2] * w * scale; // pos.z
                        }
                    }
                    // Always re-upload if any weight changed (even to reset to base)
                    queue.write_buffer(&batch.vertex_buffer, 0, bytemuck::cast_slice(&morphed));
                    if any_applied {
                        applied += 1;
                    }
                }
                if applied > 0 {
                    log::info!("Morphed {} batches", applied);
                }
                s.dirty = false;
            }
        });

        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                let w = web_sys::window().unwrap();
                let cb = f.lock().unwrap();
                w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
                    .unwrap();
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vrm_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.04,
                            b: 0.07,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &camera_light_bg, &[]);
            pass.set_bind_group(2, &shadow_bg, &[]);
            let hidden_snapshot: std::collections::HashSet<String> =
                VRM_PART_STATE.with(|s| s.borrow().hidden.clone());

            if use_skinned {
                pass.set_pipeline(&skinned_mtoon_pipeline);
                pass.set_bind_group(3, &bone_bg, &[]);
                for batch in batches.iter() {
                    if hidden_snapshot.contains(&batch.label) {
                        continue;
                    }
                    pass.set_bind_group(1, &batch.material_bg, &[]);
                    if let Some(mbg) = batch.morph_bg.as_ref() {
                        pass.set_bind_group(4, mbg, &[]);
                    }
                    pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                    pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..batch.index_count, 0, 0..1);
                }
            } else {
                let mut current_mtoon = false;
                pass.set_pipeline(&pbr_pipeline);
                for batch in batches.iter() {
                    if batch.is_mtoon != current_mtoon {
                        if batch.is_mtoon {
                            pass.set_pipeline(&mtoon_pipeline);
                        } else {
                            pass.set_pipeline(&pbr_pipeline);
                        }
                        current_mtoon = batch.is_mtoon;
                    }
                    pass.set_bind_group(1, &batch.material_bg, &[]);
                    pass.set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                    pass.set_index_buffer(batch.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..batch.index_count, 0, 0..1);
                }
            }
        }
        queue.submit(std::iter::once(enc.finish()));
        frame.present();
        let w = web_sys::window().unwrap();
        let cb = f.lock().unwrap();
        w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));

    let w = web_sys::window().unwrap();
    let cb = g.lock().unwrap();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();

    // Store morph data for JS API
    let morph_count = scene
        .morph_targets
        .iter()
        .map(|m| m.len())
        .max()
        .unwrap_or(0);
    let morph_names = scene.morph_target_names.clone();
    log::info!(
        "VRM: {} morph targets, names: {:?}",
        morph_count,
        &morph_names[..morph_names.len().min(5)]
    );

    // Store base vertex data + morph targets for CPU blending
    let mut total_morph_deltas = 0usize;
    VRM_MORPH_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.morph_weights = vec![0.0; morph_count];
        s.morph_names = morph_names;
        s.base_vertices.clear();
        s.morph_deltas.clear();
        for node in &scene.nodes {
            let mesh = &scene.meshes[node.mesh_index];
            s.base_vertices.push(mesh.vertices.clone());
            let targets = &scene.morph_targets[node.mesh_index];
            let deltas: Vec<Vec<f32>> = targets.iter().map(|t| t.position_deltas.clone()).collect();
            if !deltas.is_empty() {
                total_morph_deltas += deltas.len();
                log::info!(
                    "Batch {}: {} verts, {} morph targets (first delta len: {})",
                    s.base_vertices.len() - 1,
                    mesh.vertices.len() / 8,
                    deltas.len(),
                    deltas.first().map(|d| d.len()).unwrap_or(0)
                );
            }
            s.morph_deltas.push(deltas);
        }
        s.dirty = false;
    });
    log::info!(
        "VRM morph state: {} total morph delta arrays loaded",
        total_morph_deltas
    );

    // Populate skeleton state (L1/L2: data pipeline only; no GPU skinning yet).
    let skeleton = build_skeleton_from_gltf(&scene);
    let joint_count = scene
        .skins
        .first()
        .map(|s| s.joint_node_indices.len())
        .unwrap_or(0);
    let skinned_mesh_count = scene
        .nodes
        .iter()
        .filter(|n| n.skin_index.is_some())
        .count();

    // L6/L7: parse VRM extensions (spring bones + node constraints).
    let (spring_sim, constraint_solver, vrm_chain_count, vrm_constraint_count) =
        match kami_vrm::parse_vrm(&glb_data) {
            Ok(vrm_doc) => {
                let sim = kami_vrm::spring::SpringSimulator::new(&vrm_doc);
                let solver = kami_vrm::constraint::ConstraintSolver::new(&vrm_doc);
                let cc = sim.chain_count();
                let nc = solver.count();
                (Some(sim), Some(solver), cc, nc)
            }
            Err(e) => {
                log::warn!("VRM ext parse failed (spring/constraint disabled): {:?}", e);
                (None, None, 0, 0)
            }
        };

    // Build node→bone map for spring/constraint world-matrix lookups.
    let mut node_to_bone = std::collections::HashMap::new();
    if let Some(skin) = scene.skins.first() {
        for (bone_idx, &node_idx) in skin.joint_node_indices.iter().enumerate() {
            node_to_bone.insert(node_idx, bone_idx);
        }
    }

    VRM_SKIN_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.pose_overrides = vec![None; joint_count];
        s.pose_dirty = true; // force initial palette upload
        s.skeleton = skeleton;
        s.joint_count = joint_count;
        s.skinned_mesh_count = skinned_mesh_count;
        s.spring_sim = spring_sim;
        s.constraint_solver = constraint_solver;
        s.node_to_bone = node_to_bone;
    });
    log::info!(
        "VRM skin state: {} joints, {} skinned meshes, {} spring chains, {} node constraints",
        joint_count,
        skinned_mesh_count,
        vrm_chain_count,
        vrm_constraint_count
    );

    log::info!("VRM PBR render loop started");
    Ok(())
}

/// Thread-local VRM morph/pose state for JS interop.
struct VrmMorphState {
    morph_weights: Vec<f32>,
    morph_names: Vec<String>,
    /// Base vertex data per batch (interleaved pos3+norm3+uv2).
    base_vertices: Vec<Vec<f32>>,
    /// Morph target position deltas per batch per target (flat f32 x,y,z per vertex).
    morph_deltas: Vec<Vec<Vec<f32>>>,
    dirty: bool,
}

thread_local! {
    static VRM_MORPH_STATE: RefCell<VrmMorphState> = RefCell::new(VrmMorphState {
        morph_weights: Vec::new(),
        morph_names: Vec::new(),
        base_vertices: Vec::new(),
        morph_deltas: Vec::new(),
        dirty: false,
    });
}

/// Set VRM morph target weight by index (0.0-1.0).
/// Call this from JS to animate face expressions.
#[wasm_bindgen]
pub fn set_vrm_morph(index: u32, weight: f32) {
    VRM_MORPH_STATE.with(|state| {
        let mut s = state.borrow_mut();
        let i = index as usize;
        if i < s.morph_weights.len() {
            s.morph_weights[i] = weight;
            s.dirty = true;
        }
    });
}

/// Set VRM morph target weight by name.
#[wasm_bindgen]
pub fn set_vrm_morph_by_name(name: &str, weight: f32) {
    VRM_MORPH_STATE.with(|state| {
        let mut s = state.borrow_mut();
        if let Some(idx) = s.morph_names.iter().position(|n| n.contains(name)) {
            s.morph_weights[idx] = weight;
            s.dirty = true;
        }
    });
}

/// Get VRM morph target names as JSON array.
#[wasm_bindgen]
pub fn get_vrm_morph_names() -> String {
    VRM_MORPH_STATE.with(|state| {
        let s = state.borrow();
        serde_json::to_string(&s.morph_names).unwrap_or_else(|_| "[]".into())
    })
}

/// Set VRM camera orbit (yaw radians, pitch radians, distance).
#[wasm_bindgen]
pub fn set_vrm_camera(yaw: f32, pitch: f32, distance: f32) {
    VRM_ORBIT_STATE.with(|state| {
        let mut s = state.borrow_mut();
        *s = (yaw, pitch, distance);
    });
}

thread_local! {
    static VRM_ORBIT_STATE: RefCell<(f32, f32, f32)> = RefCell::new((0.0, 0.25, 2.0));
}

/// Reset all VRM morph weights to 0.
#[wasm_bindgen]
pub fn reset_vrm_morphs() {
    VRM_MORPH_STATE.with(|state| {
        let mut s = state.borrow_mut();
        for w in s.morph_weights.iter_mut() {
            *w = 0.0;
        }
        s.dirty = true;
    });
}

/// VRM skinning state — populated during `run_embed_vrm` for future GPU skinning.
///
/// L1/L2 of the three.js-free render path: data pipeline only. GPU skinning
/// (bone palette storage buffer + skinning WGSL + new pipeline variant) is L3+.
struct VrmSkinState {
    /// Reconstructed skeleton from glTF skin[0] (VRM convention: single skin).
    skeleton: Option<kami_skeleton::Skeleton>,
    /// Number of joints in the skin.
    joint_count: usize,
    /// Number of meshes that reference this skin.
    skinned_mesh_count: usize,
    /// Per-bone pose rotation override (quaternion xyzw). `None` = use bind pose.
    pose_overrides: Vec<Option<[f32; 4]>>,
    /// Set when pose_overrides changes; render loop recomputes palette.
    pose_dirty: bool,
    /// VRM spring bone simulator (L6). `None` if VRM extensions not parsed.
    spring_sim: Option<kami_vrm::spring::SpringSimulator>,
    /// VRM node constraint solver (L7).
    constraint_solver: Option<kami_vrm::constraint::ConstraintSolver>,
    /// glTF node index → bone index (within `skeleton`).
    node_to_bone: std::collections::HashMap<usize, usize>,
}

thread_local! {
    static VRM_SKIN_STATE: RefCell<VrmSkinState> = RefCell::new(VrmSkinState {
        skeleton: None,
        joint_count: 0,
        skinned_mesh_count: 0,
        pose_overrides: Vec::new(),
        pose_dirty: false,
        spring_sim: None,
        constraint_solver: None,
        node_to_bone: std::collections::HashMap::new(),
    });
}

/// Compute per-bone world transforms (no inverse_bind applied).
fn compute_world_transforms(
    skeleton: &kami_skeleton::Skeleton,
    overrides: &[Option<[f32; 4]>],
) -> Vec<glam::Mat4> {
    let n = skeleton.bones.len();
    let mut local = Vec::with_capacity(n);
    for (i, b) in skeleton.bones.iter().enumerate() {
        let rot = overrides
            .get(i)
            .and_then(|o| *o)
            .map(glam::Quat::from_array)
            .unwrap_or_else(|| glam::Quat::from_array(b.local_rotation));
        let pos = glam::Vec3::from(b.local_position);
        let scl = glam::Vec3::from(b.local_scale);
        local.push(glam::Mat4::from_scale_rotation_translation(scl, rot, pos));
    }
    let mut world = vec![glam::Mat4::IDENTITY; n];
    for i in 0..n {
        world[i] = match skeleton.bones[i].parent {
            Some(p) => world[p] * local[i],
            None => local[i],
        };
    }
    world
}

/// Compute joint matrix palette (world * inverse_bind per bone) with optional
/// per-bone rotation overrides. Bones without overrides use the bind-pose TRS
/// from the skeleton. Returns column-major 4x4 matrices ready for GPU upload.
fn compute_pose_palette(
    skeleton: &kami_skeleton::Skeleton,
    overrides: &[Option<[f32; 4]>],
) -> Vec<[[f32; 4]; 4]> {
    let n = skeleton.bones.len();
    let mut local = Vec::with_capacity(n);
    for (i, b) in skeleton.bones.iter().enumerate() {
        let rot = overrides
            .get(i)
            .and_then(|o| *o)
            .map(glam::Quat::from_array)
            .unwrap_or_else(|| glam::Quat::from_array(b.local_rotation));
        let pos = glam::Vec3::from(b.local_position);
        let scl = glam::Vec3::from(b.local_scale);
        local.push(glam::Mat4::from_scale_rotation_translation(scl, rot, pos));
    }
    let mut world = vec![glam::Mat4::IDENTITY; n];
    for i in 0..n {
        world[i] = match skeleton.bones[i].parent {
            Some(p) => world[p] * local[i],
            None => local[i],
        };
    }
    skeleton
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let inv = glam::Mat4::from_cols_array_2d(&b.inverse_bind);
            (world[i] * inv).to_cols_array_2d()
        })
        .collect()
}

/// Build a `kami_skeleton::Skeleton` from a loaded glTF scene's skin[0].
///
/// Remaps glTF node indices → bone indices (dense array), preserving parent
/// relationships within the skin's joint subset. Bones listed in
/// `skin.joint_node_indices` become the skeleton bones; their parents are
/// remapped if the parent is also a joint, else the bone becomes a root.
fn build_skeleton_from_gltf(
    scene: &kami_render::gltf_loader::GltfScene,
) -> Option<kami_skeleton::Skeleton> {
    let skin = scene.skins.first()?;
    if skin.joint_node_indices.is_empty() {
        return None;
    }
    // Map: gltf node index → bone index within this skeleton.
    let mut node_to_bone = std::collections::HashMap::new();
    for (bone_idx, &node_idx) in skin.joint_node_indices.iter().enumerate() {
        node_to_bone.insert(node_idx, bone_idx);
    }
    let mut bones = Vec::with_capacity(skin.joint_node_indices.len());
    for (bone_idx, &node_idx) in skin.joint_node_indices.iter().enumerate() {
        let info = scene.node_hierarchy.get(node_idx)?;
        let parent = info.parent.and_then(|p| node_to_bone.get(&p).copied());
        let inverse_bind = skin
            .inverse_bind_matrices
            .get(bone_idx)
            .copied()
            .unwrap_or_else(|| glam::Mat4::IDENTITY.to_cols_array_2d());
        bones.push(kami_skeleton::Bone {
            name: info.name.clone(),
            parent,
            local_position: info.translation,
            local_rotation: info.rotation,
            local_scale: info.scale,
            inverse_bind,
        });
    }
    Some(kami_skeleton::Skeleton { bones })
}

/// Get VRM skeleton info as JSON. Returns bone names, parent links, and joint count.
///
/// Debug export for L1/L2 of three.js-free render path. Call after `run_embed_vrm`
/// has completed loading.
#[wasm_bindgen]
pub fn get_vrm_skeleton_info() -> String {
    VRM_SKIN_STATE.with(|state| {
        let s = state.borrow();
        let mut out = String::from("{");
        out.push_str(&format!("\"joint_count\":{},", s.joint_count));
        out.push_str(&format!("\"skinned_mesh_count\":{},", s.skinned_mesh_count));
        out.push_str("\"bones\":[");
        if let Some(sk) = &s.skeleton {
            for (i, b) in sk.bones.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let parent = b
                    .parent
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "null".into());
                out.push_str(&format!(
                    "{{\"name\":{},\"parent\":{}}}",
                    serde_json::to_string(&b.name).unwrap_or_else(|_| "\"\"".into()),
                    parent
                ));
            }
        }
        out.push_str("]}");
        out
    })
}

/// Get VRM bone names as a JSON array (ordered by bone index).
#[wasm_bindgen]
pub fn get_vrm_bone_names() -> String {
    VRM_SKIN_STATE.with(|state| {
        let s = state.borrow();
        let names: Vec<&str> = s
            .skeleton
            .as_ref()
            .map(|sk| sk.bones.iter().map(|b| b.name.as_str()).collect())
            .unwrap_or_default();
        serde_json::to_string(&names).unwrap_or_else(|_| "[]".into())
    })
}

/// Set a VRM bone's rotation override (quaternion xyzw). Returns true if the
/// bone was found. The pose palette is recomputed on the next rendered frame.
#[wasm_bindgen]
pub fn set_vrm_bone_rotation(bone_name: &str, x: f32, y: f32, z: f32, w: f32) -> bool {
    VRM_SKIN_STATE.with(|state| {
        let mut s = state.borrow_mut();
        let idx = match s.skeleton.as_ref() {
            Some(sk) => sk.bones.iter().position(|b| b.name == bone_name),
            None => None,
        };
        if let Some(i) = idx {
            if i < s.pose_overrides.len() {
                s.pose_overrides[i] = Some([x, y, z, w]);
                s.pose_dirty = true;
                return true;
            }
        }
        false
    })
}

/// VRM part visibility state — populated at load with per-batch `"mesh:material"`
/// labels; the render loop skips any batch whose label is in `hidden`.
struct VrmPartState {
    labels: Vec<String>,
    hidden: std::collections::HashSet<String>,
}

thread_local! {
    static VRM_PART_STATE: RefCell<VrmPartState> = RefCell::new(VrmPartState {
        labels: Vec::new(),
        hidden: std::collections::HashSet::new(),
    });
}

/// List VRM draw-batch labels (`"{meshName}:{materialName}"`) as a JSON array.
#[wasm_bindgen]
pub fn get_vrm_mesh_labels() -> String {
    VRM_PART_STATE
        .with(|s| serde_json::to_string(&s.borrow().labels).unwrap_or_else(|_| "[]".into()))
}

/// Toggle visibility for all batches whose label contains `substring`.
/// Returns the number of affected batches.
#[wasm_bindgen]
pub fn set_vrm_mesh_visibility(substring: &str, visible: bool) -> u32 {
    let mut count = 0u32;
    VRM_PART_STATE.with(|s| {
        let mut s = s.borrow_mut();
        let labels = s.labels.clone();
        for label in &labels {
            if label.contains(substring) {
                if visible {
                    s.hidden.remove(label);
                } else {
                    s.hidden.insert(label.clone());
                }
                count += 1;
            }
        }
    });
    count
}

/// Compose a VRM by combining a base model with a preset's parts of the
/// given category (`"Hair"` / `"Outfit"` / `"Accessory"` / `"Face"` / `"Body"`).
///
/// Strategy: keep all parts from `base_bytes` EXCEPT the named category, add
/// parts of the named category from `preset_bytes`. Returns composed GLB bytes.
///
/// Used by `createPartComposer` to implement hot preset swap on the wgpu path.
#[wasm_bindgen]
pub fn compose_vrm_with_preset(
    base_bytes: &[u8],
    preset_bytes: &[u8],
    category: &str,
) -> Result<Vec<u8>, JsValue> {
    use kami_vrm::part::PartCategory;
    let cat = match category {
        "Body" => PartCategory::Body,
        "Hair" => PartCategory::Hair,
        "Face" => PartCategory::Face,
        "Outfit" => PartCategory::Outfit,
        "Accessory" => PartCategory::Accessory,
        _ => return Err(JsValue::from_str(&format!("unknown category: {category}"))),
    };
    let base_doc = kami_vrm::parse_vrm(base_bytes)
        .map_err(|e| JsValue::from_str(&format!("base parse: {e:?}")))?;
    let preset_doc = kami_vrm::parse_vrm(preset_bytes)
        .map_err(|e| JsValue::from_str(&format!("preset parse: {e:?}")))?;
    let base_parts = kami_vrm::decompose(&base_doc)
        .map_err(|e| JsValue::from_str(&format!("base decompose: {e:?}")))?;
    let preset_parts = kami_vrm::decompose(&preset_doc)
        .map_err(|e| JsValue::from_str(&format!("preset decompose: {e:?}")))?;

    let mut sources: Vec<kami_vrm::PartSource> = Vec::new();
    // Keep base parts whose category != target.
    for p in &base_parts {
        if p.category != cat {
            sources.push(kami_vrm::PartSource {
                part: p,
                doc: &base_doc,
            });
        }
    }
    // Add preset parts whose category == target.
    for p in &preset_parts {
        if p.category == cat {
            sources.push(kami_vrm::PartSource {
                part: p,
                doc: &preset_doc,
            });
        }
    }
    if sources.is_empty() {
        return Err(JsValue::from_str("no parts to compose"));
    }

    let composed =
        kami_vrm::compose::compose(&sources, &kami_vrm::ComposeConfig { skeleton_base: 0 })
            .map_err(|e| JsValue::from_str(&format!("compose: {e:?}")))?;
    let glb = kami_vrm::export_glb(&composed)
        .map_err(|e| JsValue::from_str(&format!("export: {e:?}")))?;
    Ok(glb)
}

/// Clear all VRM bone pose overrides, returning the model to bind pose.
#[wasm_bindgen]
pub fn reset_vrm_pose() {
    VRM_SKIN_STATE.with(|state| {
        let mut s = state.borrow_mut();
        for o in s.pose_overrides.iter_mut() {
            *o = None;
        }
        s.pose_dirty = true;
    });
}

/// Clamp a bone rotation (degrees) through humanoid joint constraints.
///
/// Looks up `bone_name` in `default_humanoid_constraints()`, then clamps
/// `degrees` to the `[min, max]` range for the given `axis` ("x", "y", or "z").
/// Returns the clamped value, or the input unchanged if bone/axis is not found.
#[wasm_bindgen]
pub fn clamp_bone(bone_name: &str, axis: &str, degrees: f32) -> f32 {
    let d = std::f32::consts::PI / 180.0;
    let constraints = kami_skeleton::default_humanoid_constraints();
    let Some((_, c)) = constraints.iter().find(|(name, _)| *name == bone_name) else {
        return degrees;
    };
    let idx = match axis {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        _ => return degrees,
    };
    let min_deg = c.min[idx] / d;
    let max_deg = c.max[idx] / d;
    degrees.clamp(min_deg, max_deg)
}

/// Evaluate a procedural motion animation at the given time.
///
/// Computes bone rotations (in degrees) for one of 11 built-in motions
/// (idle, breathe, nod, shake, wave_hi, dance, bounce, sway, look_around,
/// excited, sad_sway). All rotations are clamped through
/// `default_humanoid_constraints()`. Returns a JSON object mapping bone names
/// to `{"x": deg, "y": deg, "z": deg}`.
#[wasm_bindgen]
pub fn evaluate_motion(motion_key: &str, time: f32) -> String {
    let t = time;
    let mut bones: Vec<(&str, f32, f32, f32)> = Vec::new();

    match motion_key {
        "idle" => {
            bones.push(("head", 5.0 + (t * 0.8).sin() * 2.0, 0.0, 0.0));
            bones.push(("spine", (t * 1.2).sin() * 1.0, 0.0, 0.0));
        }
        "breathe" => {
            bones.push(("chest", (t * 1.5).sin() * 3.0, 0.0, 0.0));
            bones.push(("spine", (t * 1.5).sin() * 1.5, 0.0, 0.0));
        }
        "nod" => {
            bones.push(("head", (t * 3.0).sin() * 15.0, 0.0, 0.0));
        }
        "shake" => {
            bones.push(("head", 0.0, (t * 4.0).sin() * 20.0, 0.0));
        }
        "wave_hi" => {
            bones.push(("rightUpperArm", 0.0, 0.0, -70.0 + (t * 4.0).sin() * 10.0));
            bones.push(("rightLowerArm", 0.0, -100.0 + (t * 6.0).sin() * 30.0, 0.0));
            bones.push(("head", 0.0, 0.0, (t * 2.0).sin() * 5.0));
        }
        "dance" => {
            bones.push(("hips", (t * 6.0).sin() * 3.0, (t * 3.0).sin() * 10.0, 0.0));
            bones.push(("leftUpperArm", 0.0, 0.0, 50.0 + (t * 3.0).sin() * 20.0));
            bones.push((
                "rightUpperArm",
                0.0,
                0.0,
                -50.0 + (t * 3.0 + 1.0).sin() * 20.0,
            ));
            bones.push(("head", 0.0, 0.0, (t * 3.0).sin() * 8.0));
            bones.push(("spine", 0.0, (t * 3.0).sin() * 5.0, 0.0));
        }
        "bounce" => {
            bones.push(("hips", (t * 4.0).sin().abs() * 5.0, 0.0, 0.0));
            bones.push(("leftUpperArm", 0.0, 0.0, 60.0 + (t * 4.0).sin() * 10.0));
            bones.push(("rightUpperArm", 0.0, 0.0, -60.0 + (t * 4.0).sin() * 10.0));
        }
        "sway" => {
            bones.push(("spine", 0.0, 0.0, (t * 1.5).sin() * 8.0));
            bones.push(("head", 0.0, 0.0, (t * 1.5 + 0.5).sin() * 5.0));
            bones.push(("hips", 0.0, 0.0, (t * 1.5).sin() * 3.0));
        }
        "look_around" => {
            bones.push(("head", (t * 1.2).sin() * 10.0, (t * 0.8).sin() * 35.0, 0.0));
        }
        "excited" => {
            bones.push(("hips", (t * 6.0).sin().abs() * 4.0, 0.0, 0.0));
            bones.push(("leftUpperArm", 0.0, 0.0, 40.0 + (t * 5.0).sin() * 25.0));
            bones.push((
                "rightUpperArm",
                0.0,
                0.0,
                -40.0 + (t * 5.0 + 1.0).sin() * 25.0,
            ));
            bones.push(("leftLowerArm", 0.0, 70.0 + (t * 5.0).sin() * 30.0, 0.0));
            bones.push((
                "rightLowerArm",
                0.0,
                -70.0 + (t * 5.0 + 1.0).sin() * 30.0,
                0.0,
            ));
            bones.push(("head", -5.0 + (t * 3.0).sin() * 5.0, 0.0, 0.0));
        }
        "sad_sway" => {
            bones.push((
                "head",
                15.0 + (t * 0.6).sin() * 3.0,
                0.0,
                (t * 0.8).sin() * 5.0,
            ));
            bones.push(("spine", 8.0 + (t * 0.6).sin() * 2.0, 0.0, 0.0));
        }
        _ => {}
    }

    // Clamp all rotations and build JSON
    let mut result = String::from("{");
    for (i, (name, x, y, z)) in bones.iter().enumerate() {
        let cx = clamp_bone(name, "x", *x);
        let cy = clamp_bone(name, "y", *y);
        let cz = clamp_bone(name, "z", *z);
        if i > 0 {
            result.push(',');
        }
        result.push('"');
        result.push_str(name);
        result.push_str("\":{\"x\":");
        result.push_str(&format!("{:.4}", cx));
        result.push_str(",\"y\":");
        result.push_str(&format!("{:.4}", cy));
        result.push_str(",\"z\":");
        result.push_str(&format!("{:.4}", cz));
        result.push('}');
    }
    result.push('}');
    result
}

// ─── kami-rtc WebRTC SDK wasm_bindgen exports ───

thread_local! {
    static RTC_ROOM: RefCell<Option<kami_rtc::Room>> = RefCell::new(None);
}

/// Create a WebRTC room and return the join signal as JSON.
///
/// # Arguments
/// * `room_id` - Unique room identifier
/// * `local_peer_id` - Local user's peer ID (DID or session ID)
/// * `display_name` - Local user's display name
/// * `config_json` - Room configuration as JSON (optional, empty = defaults)
#[wasm_bindgen]
pub fn rtc_create_room(
    room_id: &str,
    local_peer_id: &str,
    display_name: &str,
    config_json: &str,
) -> String {
    let config: kami_rtc::RoomConfig = if config_json.is_empty() {
        kami_rtc::RoomConfig::default()
    } else {
        serde_json::from_str(config_json).unwrap_or_default()
    };

    let mut room = kami_rtc::Room::new(room_id.into(), local_peer_id.into(), config);
    let join_signal = room.join(display_name);
    let join_json = serde_json::to_string(&join_signal).unwrap_or_default();

    RTC_ROOM.with(|r| {
        *r.borrow_mut() = Some(room);
    });

    join_json
}

/// Process an incoming signaling message. Returns events as JSON array.
#[wasm_bindgen]
pub fn rtc_process_signal(signal_json: &str) -> String {
    let msg: kami_rtc::SignalMessage = match serde_json::from_str(signal_json) {
        Ok(m) => m,
        Err(_) => return "[]".into(),
    };

    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let events = room.process_signal(&msg);
            serde_json::to_string(&events).unwrap_or_else(|_| "[]".into())
        } else {
            "[]".into()
        }
    })
}

/// Create an SDP offer for a specific peer. Returns signal JSON.
#[wasm_bindgen]
pub fn rtc_create_offer(to_peer_id: &str, sdp: &str) -> String {
    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let signal = room.create_offer(to_peer_id.into(), sdp.into());
            serde_json::to_string(&signal).unwrap_or_default()
        } else {
            String::new()
        }
    })
}

/// Create an SDP answer for a specific peer. Returns signal JSON.
#[wasm_bindgen]
pub fn rtc_create_answer(to_peer_id: &str, sdp: &str) -> String {
    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let signal = room.create_answer(to_peer_id.into(), sdp.into());
            serde_json::to_string(&signal).unwrap_or_default()
        } else {
            String::new()
        }
    })
}

/// Create an ICE candidate message. Returns signal JSON.
#[wasm_bindgen]
pub fn rtc_create_ice_candidate(to_peer_id: &str, candidate_json: &str) -> String {
    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let signal = room.create_ice_candidate(to_peer_id.into(), candidate_json.into());
            serde_json::to_string(&signal).unwrap_or_default()
        } else {
            String::new()
        }
    })
}

/// Update local position for spatial audio. Returns signal JSON to broadcast.
#[wasm_bindgen]
pub fn rtc_update_position(x: f32, y: f32, z: f32) -> String {
    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let signal = room.update_position([x, y, z]);
            serde_json::to_string(&signal).unwrap_or_default()
        } else {
            String::new()
        }
    })
}

/// Run spatial audio spatialization. Returns JSON array of
/// `[peer_id, left_vol, right_vol, pan]` tuples.
#[wasm_bindgen]
pub fn rtc_spatialize() -> String {
    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let results = room.spatialize();
            serde_json::to_string(&results).unwrap_or_else(|_| "[]".into())
        } else {
            "[]".into()
        }
    })
}

/// Send data channel message (cursor, annotation, reaction). Returns signal JSON.
#[wasm_bindgen]
pub fn rtc_send_data(data_json: &str) -> String {
    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let signal = room.send_data(data_json.into());
            serde_json::to_string(&signal).unwrap_or_default()
        } else {
            String::new()
        }
    })
}

/// Get room summary as JSON.
#[wasm_bindgen]
pub fn rtc_room_summary() -> String {
    RTC_ROOM.with(|r| {
        let room = r.borrow();
        if let Some(room) = room.as_ref() {
            room.summary_json()
        } else {
            "{}".into()
        }
    })
}

/// Leave the room and clean up. Returns leave signal JSON.
#[wasm_bindgen]
pub fn rtc_leave_room() -> String {
    RTC_ROOM.with(|r| {
        let mut room = r.borrow_mut();
        if let Some(room) = room.as_mut() {
            let signal = room.leave();
            let json = serde_json::to_string(&signal).unwrap_or_default();
            json
        } else {
            String::new()
        }
    })
}

// ════════════════════════════════════════════════════════════════════════
// Terrain demo — Decima-style open world terrain (kami-terrain + kami-atmosphere)
// ════════════════════════════════════════════════════════════════════════

/// Generate terrain chunk mesh data for a given config.
/// Returns JSON: `{ "vertices": [f32...], "indices": [u32...], "vertex_count": N }`
///
/// `config_json`: `{ "width": 129, "depth": 129, "seed": 42, "max_height": 120,
///   "frequency": 0.005, "octaves": 6, "origin_x": 0, "origin_z": 0, "lod": 0 }`
#[wasm_bindgen]
pub fn generate_terrain_chunk(config_json: &str) -> String {
    use kami_terrain::{BiomePreset, Heightmap, HeightmapConfig, Splatmap, generate_chunk_mesh};

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
        lod: Option<u32>,
        /// Biome preset: "plains" | "quarry" | "desert" | "tundra".
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
        lod: None,
        biome: None,
    });

    let w = cfg.width.unwrap_or(129);
    let d = cfg.depth.unwrap_or(129);
    let ox = cfg.origin_x.unwrap_or(0.0);
    let oz = cfg.origin_z.unwrap_or(0.0);
    let lod = cfg.lod.unwrap_or(0);
    let stride = 1u32 << lod;

    let biome = match cfg.biome.as_deref() {
        Some("quarry") => BiomePreset::Quarry,
        Some("desert") => BiomePreset::Desert,
        Some("tundra") => BiomePreset::Tundra,
        _ => BiomePreset::Plains,
    };
    let seed = cfg.seed.unwrap_or(42.0);
    let mut hm_cfg = biome.heightmap(seed);
    // Allow per-call overrides
    if let Some(mh) = cfg.max_height {
        hm_cfg.max_height = mh;
    }
    if let Some(f) = cfg.frequency {
        hm_cfg.frequency = f;
    }
    if let Some(o) = cfg.octaves {
        hm_cfg.octaves = o;
    }

    let st = biome.splat_thresholds();
    let palette = biome.palette();

    let hm = Heightmap::generate(w, d, ox, oz, &hm_cfg);
    let splat = Splatmap::from_heightmap(&hm, st.sand_line, st.snow_line, st.rock_slope);
    let chunk = generate_chunk_mesh(&hm, &splat, ox, oz, stride, 1.0, lod);

    // Flatten TerrainVertex to f32 array (12 floats per vertex)
    let verts: Vec<f32> = chunk
        .vertices
        .iter()
        .flat_map(|v| {
            [
                v.position[0],
                v.position[1],
                v.position[2],
                v.normal[0],
                v.normal[1],
                v.normal[2],
                v.uv[0],
                v.uv[1],
                v.splat[0],
                v.splat[1],
                v.splat[2],
                v.splat[3],
            ]
        })
        .collect();

    serde_json::json!({
        "vertices": verts,
        "indices": chunk.indices,
        "vertex_count": chunk.vertices.len(),
        "index_count": chunk.indices.len(),
        "lod": lod,
        "biome": biome.name(),
        "palette": {
            "base": palette.base,
            "tip": palette.tip,
        },
    })
    .to_string()
}

// ── Vegetation cache for per-frame culling ──
thread_local! {
    static VEG_CACHE: std::cell::RefCell<Vec<kami_vegetation::InstanceData>> = std::cell::RefCell::new(Vec::new());
}

/// Cache the most recently generated vegetation instances for culling.
/// Call once after `generate_vegetation` — subsequent `cull_vegetation` calls
/// read from this cache (avoids re-uploading instances each frame).
#[wasm_bindgen]
pub fn cache_vegetation(config_json: &str) -> u32 {
    use kami_terrain::{BiomePreset, Heightmap, HeightmapConfig, Splatmap};
    use kami_vegetation::{PlacementConfig, place_instances};

    #[derive(serde::Deserialize)]
    struct TerrainCfg {
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
    #[derive(serde::Deserialize)]
    struct PlaceCfg {
        seed: Option<u32>,
        extent: Option<f32>,
        density_scale: Option<f32>,
    }
    #[derive(serde::Deserialize)]
    struct Cfg {
        terrain: Option<TerrainCfg>,
        placement: Option<PlaceCfg>,
    }

    let cfg: Cfg = serde_json::from_str(config_json).unwrap_or(Cfg {
        terrain: None,
        placement: None,
    });
    let t = cfg.terrain.unwrap_or(TerrainCfg {
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
    let p = cfg.placement.unwrap_or(PlaceCfg {
        seed: None,
        extent: None,
        density_scale: None,
    });

    let biome = match t.biome.as_deref() {
        Some("quarry") => BiomePreset::Quarry,
        Some("desert") => BiomePreset::Desert,
        Some("tundra") => BiomePreset::Tundra,
        _ => BiomePreset::Plains,
    };
    let mut hm_cfg = biome.heightmap(t.seed.unwrap_or(42.0));
    if let Some(mh) = t.max_height {
        hm_cfg.max_height = mh;
    }
    if let Some(f) = t.frequency {
        hm_cfg.frequency = f;
    }
    if let Some(o) = t.octaves {
        hm_cfg.octaves = o;
    }
    let w = t.width.unwrap_or(257);
    let d = t.depth.unwrap_or(257);
    let ox = t.origin_x.unwrap_or(-128.0);
    let oz = t.origin_z.unwrap_or(-128.0);

    let hm = Heightmap::generate(w, d, ox, oz, &hm_cfg);
    let st = biome.splat_thresholds();
    let splat = Splatmap::from_heightmap(&hm, st.sand_line, st.snow_line, st.rock_slope);
    let pc = PlacementConfig {
        seed: p.seed.unwrap_or(42),
        extent: p.extent.unwrap_or(220.0),
        density_scale: p.density_scale.unwrap_or(0.5),
        species_filter: Vec::new(),
    };
    let instances = place_instances(&hm, &splat, ox, oz, &pc);
    let count = instances.len() as u32;
    VEG_CACHE.with(|c| *c.borrow_mut() = instances);
    count
}

/// Cull cached vegetation by camera position + budget.
/// Returns flat `[pos.xyz, scale, rotation, species, wind_phase, color_tint]` × N.
/// Call per frame. WASM does distance sort + LOD filter internally.
#[wasm_bindgen]
pub fn cull_vegetation(cam_x: f32, cam_z: f32, budget: u32) -> Vec<f32> {
    VEG_CACHE.with(|c| {
        let inst = c.borrow();
        kami_vegetation::cull_to_buffer(&inst, cam_x, cam_z, budget as usize)
    })
}

// `get_heightmap` (archived 2026-04-14): superseded by cache_heightmap +
// sample_terrain_height. Only the JS-hybrid demos (now in _archive/) consumed it.

/// Compute sky uniform for the atmosphere shader.
/// `time_of_day`: [0, 1] where 0.5 = noon.
/// Returns JSON with sun_dir, sun_color, fog_color, fog_density.
#[wasm_bindgen]
pub fn compute_sky_uniform(time_of_day: f32) -> String {
    let mut cycle = kami_atmosphere::DayNightCycle::default();
    cycle.time = time_of_day;
    let u = cycle.to_uniform();
    serde_json::json!({
        "sun_dir": u.sun_dir,
        "sun_color": u.sun_color,
        "fog_color": u.fog_color,
        "fog_density": u.fog_density,
        "time_of_day": u.time_of_day,
    })
    .to_string()
}

/// Generate water plane mesh + Gerstner wave parameters.
/// `config_json`: `{ "sea_level": 18, "extent": 512, "resolution": 128 }`
/// Returns JSON: `{ "vertices": [f32...], "indices": [u32...], "waves": [...], ... }`
#[wasm_bindgen]
pub fn generate_water_mesh(config_json: &str) -> String {
    use kami_terrain::{WaterConfig, default_waves};

    #[derive(serde::Deserialize)]
    struct Cfg {
        sea_level: Option<f32>,
        extent: Option<f32>,
        resolution: Option<u32>,
    }

    let cfg: Cfg = serde_json::from_str(config_json).unwrap_or(Cfg {
        sea_level: None,
        extent: None,
        resolution: None,
    });

    let water_cfg = WaterConfig {
        sea_level: cfg.sea_level.unwrap_or(18.0),
        extent: cfg.extent.unwrap_or(512.0),
        resolution: cfg.resolution.unwrap_or(128),
        waves: default_waves(),
    };

    let (verts, indices) = kami_terrain::generate_water_mesh(&water_cfg);

    let flat_verts: Vec<f32> = verts
        .iter()
        .flat_map(|v| {
            [
                v.position[0],
                v.position[1],
                v.position[2],
                v.uv[0],
                v.uv[1],
            ]
        })
        .collect();

    let waves: Vec<serde_json::Value> = water_cfg
        .waves
        .iter()
        .map(|w| {
            serde_json::json!({
                "direction": w.direction,
                "amplitude": w.amplitude,
                "wavelength": w.wavelength,
                "speed": w.speed,
                "steepness": w.steepness,
            })
        })
        .collect();

    serde_json::json!({
        "vertices": flat_verts,
        "indices": indices,
        "vertex_count": verts.len(),
        "index_count": indices.len(),
        "sea_level": water_cfg.sea_level,
        "waves": waves,
    })
    .to_string()
}

/// Compute 4 Gerstner waves from wind direction + speed + gust.
/// Returns JSON: `{ "waves": [{dir, amp, wavelength, speed, steepness}, x4] }`
///
/// Use this to update water uniform when wind changes — waves align with wind,
/// amplitude scales with Beaufort wind strength, wavelength follows deep-water
/// dispersion.
#[wasm_bindgen]
pub fn compute_wind_waves(dir_x: f32, dir_z: f32, wind_speed: f32, gust: f32) -> String {
    let waves = kami_terrain::waves_from_wind([dir_x, dir_z], wind_speed, gust);
    let arr: Vec<serde_json::Value> = waves
        .iter()
        .map(|w| {
            serde_json::json!({
                "direction": w.direction,
                "amplitude": w.amplitude,
                "wavelength": w.wavelength,
                "speed": w.speed,
                "steepness": w.steepness,
            })
        })
        .collect();
    serde_json::json!({ "waves": arr }).to_string()
}

/// Generate vegetation instances (grass/fern/palm/conifer/bush) over terrain.
/// Returns JSON: `{ "instances": [f32... 8 per instance], "count": N, "by_species": {...} }`
#[wasm_bindgen]
pub fn generate_vegetation(config_json: &str) -> String {
    use kami_terrain::{Heightmap, HeightmapConfig, Splatmap};
    use kami_vegetation::{PlacementConfig, place_instances};

    #[derive(serde::Deserialize)]
    struct TerrainCfg {
        width: Option<u32>,
        depth: Option<u32>,
        seed: Option<f32>,
        max_height: Option<f32>,
        frequency: Option<f32>,
        octaves: Option<u32>,
        origin_x: Option<f32>,
        origin_z: Option<f32>,
    }
    #[derive(serde::Deserialize)]
    struct PlaceCfg {
        seed: Option<u32>,
        extent: Option<f32>,
        density_scale: Option<f32>,
    }
    #[derive(serde::Deserialize)]
    struct Cfg {
        terrain: Option<TerrainCfg>,
        placement: Option<PlaceCfg>,
    }

    let cfg: Cfg = serde_json::from_str(config_json).unwrap_or(Cfg {
        terrain: None,
        placement: None,
    });
    let t = cfg.terrain.unwrap_or(TerrainCfg {
        width: None,
        depth: None,
        seed: None,
        max_height: None,
        frequency: None,
        octaves: None,
        origin_x: None,
        origin_z: None,
    });
    let p = cfg.placement.unwrap_or(PlaceCfg {
        seed: None,
        extent: None,
        density_scale: None,
    });

    let hm_cfg = HeightmapConfig {
        seed: t.seed.unwrap_or(42.0),
        max_height: t.max_height.unwrap_or(80.0),
        frequency: t.frequency.unwrap_or(0.008),
        octaves: t.octaves.unwrap_or(7),
        ..HeightmapConfig::default()
    };
    let w = t.width.unwrap_or(257);
    let d = t.depth.unwrap_or(257);
    let ox = t.origin_x.unwrap_or(-128.0);
    let oz = t.origin_z.unwrap_or(-128.0);

    let hm = Heightmap::generate(w, d, ox, oz, &hm_cfg);
    let splat = Splatmap::from_heightmap(&hm, 15.0, 100.0, 0.4);

    let pc = PlacementConfig {
        seed: p.seed.unwrap_or(42),
        extent: p.extent.unwrap_or(220.0),
        density_scale: p.density_scale.unwrap_or(0.5),
        species_filter: Vec::new(),
    };
    let instances = place_instances(&hm, &splat, ox, oz, &pc);

    let flat: Vec<f32> = instances
        .iter()
        .flat_map(|i| {
            [
                i.position[0],
                i.position[1],
                i.position[2],
                i.scale,
                i.rotation,
                i.species,
                i.wind_phase,
                i.color_tint,
            ]
        })
        .collect();

    let mut by_species = [0u32; 5];
    for i in &instances {
        let s = i.species as usize;
        if s < 5 {
            by_species[s] += 1;
        }
    }

    serde_json::json!({
        "instances": flat,
        "count": instances.len(),
        "by_species": {
            "grass": by_species[0], "fern": by_species[1], "palm": by_species[2],
            "conifer": by_species[3], "bush": by_species[4],
        },
    })
    .to_string()
}

/// Compute full weather state (sky + wind + clouds) for one frame.
/// `time_of_day`: [0,1], `game_time`: seconds since start, `preset`: "default"|"overcast"|"clear".
#[wasm_bindgen]
pub fn compute_weather(time_of_day: f32, game_time: f32) -> String {
    compute_weather_preset(time_of_day, game_time, "default")
}

/// Compute weather with a named preset.
#[wasm_bindgen]
pub fn compute_weather_preset(time_of_day: f32, game_time: f32, preset: &str) -> String {
    // Executor edge (ADR-0044/0046): named presets load from kami-atmosphere-scene's
    // weather.edn (builtin fallback); an unknown name keeps the old `Weather::default()`.
    let mut weather =
        kami_atmosphere_scene::resolve_weather(preset).unwrap_or_else(kami_atmosphere::Weather::default);
    if time_of_day >= 0.0 {
        weather.day_night.time = time_of_day;
    }
    weather.wind.tick(game_time);
    weather.clouds.tick(&weather.wind, game_time);

    let sky = weather.day_night.to_uniform();
    let wind = weather.wind.to_uniform();
    let cloud = weather.clouds.to_uniform();

    serde_json::json!({
        "sky": {
            "sun_dir": sky.sun_dir,
            "sun_color": sky.sun_color,
            "fog_color": sky.fog_color,
            "fog_density": sky.fog_density,
            "time_of_day": sky.time_of_day,
            "sun_radius": sky.sun_radius,
        },
        "wind": {
            "direction": wind.direction,
            "speed": wind.speed,
            "gust": wind.gust,
            "gust_multiplier": wind.gust_multiplier,
            "turbulence": wind.turbulence,
        },
        "cloud": {
            "coverage": cloud.coverage,
            "altitude": cloud.altitude,
            "scroll_x": cloud.scroll_x,
            "scroll_z": cloud.scroll_z,
            "density": cloud.density,
            "sharpness": cloud.sharpness,
        },
    })
    .to_string()
}
