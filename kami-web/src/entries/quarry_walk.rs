//! Full Rust entry point for quarry-walk demo.
//!
//! HTML only needs a <canvas> + `await init(); run_with_quarry_walk('id')`.
//! All WebGPU setup, event handling, render loop, and HUD updates are Rust.

use std::cell::RefCell;
use std::rc::Rc;

use glam::{Mat4, Vec3};
use kami_game::quarry_scene::{
    CameraMode, CameraState, EYE_HEIGHT, InputState, Player, build_character_mesh, camera_matrices,
    character_model_matrix, tick_player,
};
use kami_render::scene_pipelines::{
    CharacterPipeline, CharacterUniform, SkyPipeline, SkyUniform, TerrainPipeline, TerrainUniform,
    VegetationPipeline, VegetationUniform,
};
use kami_terrain::BiomePreset;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

const WORLD_EXTENT: f32 = 512.0;
const TERRAIN_SEED: f32 = 77.0;
const VEG_BUDGET: u32 = 2500;

/// Run the quarry-walk demo inside a canvas.
#[wasm_bindgen]
pub async fn run_with_quarry_walk(canvas_id: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    set_hud(&document, "s-loading", "Initializing WebGPU...");

    let (device, queue, surface, mut config, format, _w, _h) = crate::init_gpu(&canvas).await?;
    let device = Rc::new(device);
    let queue = Rc::new(queue);

    // ── Terrain + heightmap (via kami_terrain directly) ──
    set_hud(&document, "s-loading", "Generating terrain...");
    let biome = BiomePreset::Quarry;
    let hm_cfg = biome.heightmap(TERRAIN_SEED);
    let hm = kami_terrain::Heightmap::generate(
        513,
        513,
        -WORLD_EXTENT * 0.5,
        -WORLD_EXTENT * 0.5,
        &hm_cfg,
    );
    let st = biome.splat_thresholds();
    let splat =
        kami_terrain::Splatmap::from_heightmap(&hm, st.sand_line, st.snow_line, st.rock_slope);
    let palette = biome.palette();
    let chunk = kami_terrain::generate_chunk_mesh(
        &hm,
        &splat,
        -WORLD_EXTENT * 0.5,
        -WORLD_EXTENT * 0.5,
        1,
        1.0,
        0,
    );
    let terrain_verts_flat: Vec<f32> = chunk
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
    let terrain_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("terrain_vb"),
        contents: bytemuck::cast_slice(&terrain_verts_flat),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let terrain_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("terrain_ib"),
        contents: bytemuck::cast_slice(&chunk.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let terrain_index_count = chunk.indices.len() as u32;

    // Heightmap sampler closure (captured by player physics)
    let hm_rc = Rc::new(hm);

    // ── Vegetation cache (reuse global VEG_CACHE via kami_vegetation) ──
    set_hud(&document, "s-loading", "Placing vegetation...");
    let placement_cfg = kami_vegetation::PlacementConfig {
        seed: 77,
        extent: WORLD_EXTENT * 0.9,
        density_scale: 0.7,
        species_filter: Vec::new(),
    };
    let instances = kami_vegetation::place_instances(
        &hm_rc,
        &splat,
        -WORLD_EXTENT * 0.5,
        -WORLD_EXTENT * 0.5,
        &placement_cfg,
    );
    let instance_count_total = instances.len() as u32;
    let instances_rc = Rc::new(instances);

    // ── Character mesh ──
    let char_mesh = build_character_mesh();
    let char_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("char_vb"),
        contents: bytemuck::cast_slice(&char_mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let char_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("char_ib"),
        contents: bytemuck::cast_slice(&char_mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let char_index_count = char_mesh.indices.len() as u32;

    // ── Pipelines ──
    let terrain_pl = TerrainPipeline::new(&device, format);
    let sky_pl = SkyPipeline::new(&device, format);
    let mut veg_pl = VegetationPipeline::new(&device, format, VEG_BUDGET);
    // Upload per-species meshes (grass 3-blade / fern / palm / conifer / bush)
    let mesh_lib = kami_vegetation::mesh::species_mesh_library();
    let meshes_for_gpu: Vec<(u32, Vec<f32>, Vec<u32>)> = mesh_lib
        .iter()
        .map(|(species, m)| (*species as u32, m.vertices.clone(), m.indices.clone()))
        .collect();
    veg_pl.upload_species_meshes(&device, &meshes_for_gpu);
    let char_pl = CharacterPipeline::new(&device, format);

    // Depth texture
    let depth_tex = Rc::new(RefCell::new(create_depth_texture(
        &device,
        config.width,
        config.height,
    )));

    // ── Player / camera / input (shared state) ──
    let player = Rc::new(RefCell::new({
        let mut p = Player::default();
        // Spawn: find low-point in 100×100 near origin
        let mut best = (0.0f32, 0.0f32, 1e9f32);
        for dx in (-100..=100).step_by(10) {
            for dz in (-100..=100).step_by(10) {
                let h = sample_hm(&hm_rc, dx as f32, dz as f32);
                if h < best.2 {
                    best = (dx as f32, dz as f32, h);
                }
            }
        }
        p.x = best.0;
        p.z = best.1;
        p.y = best.2;
        p
    }));
    let input = Rc::new(RefCell::new(InputState::default()));
    let cam_state = Rc::new(RefCell::new(CameraState::default()));
    // Executor edge (ADR-0044/0046): overcast preset from kami-atmosphere-scene's
    // weather.edn, falling back to the compiled-in Weather::overcast().
    let weather = Rc::new(RefCell::new(
        kami_atmosphere_scene::resolve_weather("overcast")
            .unwrap_or_else(kami_atmosphere::Weather::overcast),
    ));

    // ── Event listeners ──
    attach_input_listeners(&canvas, &input, &cam_state)?;

    // HUD: hide loading, show controls
    set_hud_display(&document, "loading", "none");
    set_hud_display(&document, "hud", "block");
    set_hud_display(&document, "controls", "block");

    // ── Render loop (RAF) ──
    let game_time = Rc::new(RefCell::new(0.0f32));
    let last_time = Rc::new(RefCell::new(window.performance().unwrap().now() as f32));
    let fps_counter = Rc::new(RefCell::new((0u32, 0.0f32)));

    let surface = Rc::new(surface);
    let config_rc = Rc::new(RefCell::new(config.clone()));

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let device_c = device.clone();
    let queue_c = queue.clone();
    let surface_c = surface.clone();
    let config_c = config_rc.clone();
    let depth_c = depth_tex.clone();
    let player_c = player.clone();
    let input_c = input.clone();
    let cam_c = cam_state.clone();
    let weather_c = weather.clone();
    let gt_c = game_time.clone();
    let lt_c = last_time.clone();
    let fps_c = fps_counter.clone();
    let hm_c = hm_rc.clone();
    let instances_c = instances_rc.clone();
    let document_c = document.clone();

    *g.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
        let w = web_sys::window().unwrap();
        let now = w.performance().unwrap().now() as f32;
        let dt = ((now - *lt_c.borrow()) / 1000.0).min(0.05);
        *lt_c.borrow_mut() = now;
        *gt_c.borrow_mut() += dt;
        let gt = *gt_c.borrow();

        // FPS
        {
            let mut fc = fps_c.borrow_mut();
            fc.0 += 1;
            fc.1 += dt;
            if fc.1 >= 0.5 {
                let fps = (fc.0 as f32 / fc.1).round() as u32;
                set_hud(&document_c, "s-fps", &fps.to_string());
                fc.0 = 0;
                fc.1 = 0.0;
            }
        }

        // Physics + input
        let hm_ref = &*hm_c;
        let sampler: &dyn Fn(f32, f32) -> f32 = &|x, z| sample_hm(hm_ref, x, z);
        tick_player(
            &mut player_c.borrow_mut(),
            &mut input_c.borrow_mut(),
            sampler,
            dt,
            WORLD_EXTENT * 0.5 - 3.0,
        );

        // Weather tick (for cloud scroll + gust)
        weather_c.borrow_mut().tick(dt);
        let weather = weather_c.borrow();
        let sky_u = weather.day_night.to_uniform();
        let wind_u = weather.wind.to_uniform();
        let cloud_u = weather.clouds.to_uniform();

        // Camera matrices
        let (eye, target) = camera_matrices(&player_c.borrow(), &cam_c.borrow(), sampler);
        let aspect = config_c.borrow().width as f32 / config_c.borrow().height as f32;
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::PI / 3.0, aspect, 0.3, 2000.0);
        let view_proj = proj * view;

        // Uniforms
        let fog_col = [0.70, 0.72, 0.75];
        let sun_col_neutral = [0.87, 0.87, 0.85];
        let mut base_col = [[0.0f32; 4]; 4];
        let mut tip_col = [[0.0f32; 4]; 4];
        for i in 0..4 {
            base_col[i] = [
                palette.base[i][0],
                palette.base[i][1],
                palette.base[i][2],
                0.0,
            ];
            tip_col[i] = [palette.tip[i][0], palette.tip[i][1], palette.tip[i][2], 0.0];
        }
        let t_u = TerrainUniform {
            view_proj: view_proj.to_cols_array(),
            cam_pos: eye.to_array(),
            _p0: 0.0,
            sun_dir: sky_u.sun_dir,
            _p1: 0.0,
            sun_color: sun_col_neutral,
            fog_density: sky_u.fog_density,
            fog_color: fog_col,
            _p2: 0.0,
            base_col,
            tip_col,
        };
        queue_c.write_buffer(&terrain_pl.uniform, 0, bytemuck::bytes_of(&t_u));

        let inv_vp = view_proj.inverse();
        let s_u = SkyUniform {
            inv_vp: inv_vp.to_cols_array(),
            cam_pos: eye.to_array(),
            _p0: 0.0,
            sun_dir: sky_u.sun_dir,
            _p1: 0.0,
            fog_color: fog_col,
            overcast: cloud_u.coverage,
            scroll_x: cloud_u.scroll_x,
            scroll_z: cloud_u.scroll_z,
            altitude: cloud_u.altitude,
            _p2: 0.0,
        };
        queue_c.write_buffer(&sky_pl.uniform, 0, bytemuck::bytes_of(&s_u));

        let v_u = VegetationUniform {
            view_proj: view_proj.to_cols_array(),
            cam_pos: eye.to_array(),
            time: gt,
            sun_dir: sky_u.sun_dir,
            wind_speed: wind_u.speed,
            fog_color: fog_col,
            fog_density: sky_u.fog_density,
            wind_dir: wind_u.direction,
            gust_mul: wind_u.gust_multiplier,
            biome_dry: 1.0,
        };
        queue_c.write_buffer(&veg_pl.uniform, 0, bytemuck::bytes_of(&v_u));

        let model = character_model_matrix(&player_c.borrow());
        let c_u = CharacterUniform {
            view_proj: view_proj.to_cols_array(),
            model: model.to_cols_array(),
            cam_pos: eye.to_array(),
            _p0: 0.0,
            sun_dir: sky_u.sun_dir,
            _p1: 0.0,
            sun_color: sun_col_neutral,
            fog_density: sky_u.fog_density,
            fog_color: fog_col,
            _p2: 0.0,
        };
        queue_c.write_buffer(&char_pl.uniform, 0, bytemuck::bytes_of(&c_u));

        // Vegetation cull → partition by species (5 contiguous ranges in instance buffer)
        let visible =
            kami_vegetation::cull_to_buffer(&instances_c, eye.x, eye.z, VEG_BUDGET as usize);
        let render_count = (visible.len() / 8) as u32;
        // Re-sort by (species, original distance order preserved within species)
        let mut by_species: [Vec<f32>; 5] = Default::default();
        for chunk in visible.chunks_exact(8) {
            let sp = chunk[5] as u32 as usize;
            if sp < 5 {
                by_species[sp].extend_from_slice(chunk);
            }
        }
        // Pack contiguously + remember per-species ranges
        let mut packed: Vec<f32> = Vec::with_capacity(visible.len());
        let mut species_ranges: [(u32, u32); 5] = [(0, 0); 5];
        for (sp, bucket) in by_species.iter().enumerate() {
            let start = (packed.len() / 8) as u32;
            let count = (bucket.len() / 8) as u32;
            species_ranges[sp] = (start, count);
            packed.extend_from_slice(bucket);
        }
        if !packed.is_empty() {
            queue_c.write_buffer(&veg_pl.instance_vb, 0, bytemuck::cast_slice(&packed));
        }

        // HUD
        {
            let p = player_c.borrow();
            set_hud(&document_c, "s-pos", &format!("{:.1},{:.1}", p.x, p.z));
            set_hud(&document_c, "s-h", &format!("{:.1}", p.y));
            set_hud(&document_c, "s-speed", &format!("{:.1} m/s", p.move_speed));
            let cm = cam_c.borrow();
            set_hud(
                &document_c,
                "s-view",
                if cm.mode == CameraMode::FirstPerson {
                    "FP"
                } else {
                    "3P"
                },
            );
            set_hud(
                &document_c,
                "s-plants",
                &format!("{}/{}", render_count, instance_count_total),
            );
        }

        // ── Render ──
        let frame = match surface_c.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                // Reconfigure on lost/outdated
                let new_cfg = config_c.borrow().clone();
                surface_c.configure(&device_c, &new_cfg);
                request_next(&f);
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth_c
            .borrow()
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut enc = device_c.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("quarry_enc"),
        });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("quarry_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
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
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Sky
            pass.set_pipeline(&sky_pl.pipeline);
            pass.set_bind_group(0, &sky_pl.bind_group, &[]);
            pass.draw(0..3, 0..1);

            // Terrain
            pass.set_pipeline(&terrain_pl.pipeline);
            pass.set_bind_group(0, &terrain_pl.bind_group, &[]);
            pass.set_vertex_buffer(0, terrain_vb.slice(..));
            pass.set_index_buffer(terrain_ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..terrain_index_count, 0, 0..1);

            // Vegetation: per-species indexed draw (5 species, batched via ranges)
            if render_count > 0 && !veg_pl.species_meshes.is_empty() {
                pass.set_pipeline(&veg_pl.pipeline);
                pass.set_bind_group(0, &veg_pl.bind_group, &[]);
                pass.set_vertex_buffer(1, veg_pl.instance_vb.slice(..));
                for (sp_id, (start, count)) in species_ranges.iter().enumerate() {
                    if *count == 0 {
                        continue;
                    }
                    let Some(mesh) = veg_pl.species_meshes.get(sp_id) else {
                        continue;
                    };
                    pass.set_vertex_buffer(0, mesh.vb.slice(..));
                    pass.set_index_buffer(mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, *start..(*start + *count));
                }
            }

            // Character (3P only)
            if cam_c.borrow().mode == CameraMode::ThirdPerson {
                pass.set_pipeline(&char_pl.pipeline);
                pass.set_bind_group(0, &char_pl.bind_group, &[]);
                pass.set_vertex_buffer(0, char_vb.slice(..));
                pass.set_index_buffer(char_ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..char_index_count, 0, 0..1);
            }
        }
        queue_c.submit(Some(enc.finish()));
        frame.present();

        request_next(&f);
    }));

    request_next(&g);
    let _ = config;
    Ok(())
}

// ── Helpers ──

fn request_next(f: &Rc<RefCell<Option<Closure<dyn FnMut()>>>>) {
    let w = web_sys::window().unwrap();
    let cb = f.borrow();
    w.request_animation_frame(cb.as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
}

fn sample_hm(hm: &kami_terrain::Heightmap, x: f32, z: f32) -> f32 {
    // World (x,z) → grid. hm was generated with origin (-extent/2, -extent/2).
    let origin = -WORLD_EXTENT * 0.5;
    let gx = (x - origin).clamp(0.0, (hm.width - 1) as f32);
    let gz = (z - origin).clamp(0.0, (hm.depth - 1) as f32);
    hm.sample(gx, gz)
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn set_hud(doc: &web_sys::Document, id: &str, val: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        el.set_text_content(Some(val));
    }
}

fn set_hud_display(doc: &web_sys::Document, id: &str, val: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        let _ = el.set_attribute("style", &format!("display:{}", val));
    }
}

fn attach_input_listeners(
    canvas: &web_sys::HtmlCanvasElement,
    input: &Rc<RefCell<InputState>>,
    cam: &Rc<RefCell<CameraState>>,
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    // Keydown
    let input_kd = input.clone();
    let cam_kd = cam.clone();
    let kd = Closure::<dyn FnMut(_)>::new(move |e: web_sys::KeyboardEvent| {
        let k = e.key().to_lowercase();
        let mut i = input_kd.borrow_mut();
        match k.as_str() {
            "w" => i.forward = true,
            "s" => i.back = true,
            "a" => i.left = true,
            "d" => i.right = true,
            "shift" => i.sprint = true,
            " " => i.jump_pressed = true,
            "f" => {
                let mut c = cam_kd.borrow_mut();
                c.mode = if c.mode == CameraMode::FirstPerson {
                    CameraMode::ThirdPerson
                } else {
                    CameraMode::FirstPerson
                };
            }
            _ => {}
        }
    });
    window.add_event_listener_with_callback("keydown", kd.as_ref().unchecked_ref())?;
    kd.forget();

    // Keyup
    let input_ku = input.clone();
    let ku = Closure::<dyn FnMut(_)>::new(move |e: web_sys::KeyboardEvent| {
        let k = e.key().to_lowercase();
        let mut i = input_ku.borrow_mut();
        match k.as_str() {
            "w" => i.forward = false,
            "s" => i.back = false,
            "a" => i.left = false,
            "d" => i.right = false,
            "shift" => i.sprint = false,
            _ => {}
        }
    });
    window.add_event_listener_with_callback("keyup", ku.as_ref().unchecked_ref())?;
    ku.forget();

    // Mouse move (only when pointer locked)
    let input_mm = input.clone();
    let doc = window.document().ok_or("no document")?;
    let doc_mm = doc.clone();
    let canvas_ptr = canvas.clone();
    let mm = Closure::<dyn FnMut(_)>::new(move |e: web_sys::MouseEvent| {
        if doc_mm
            .pointer_lock_element()
            .map(|el| el == *canvas_ptr.as_ref())
            .unwrap_or(false)
        {
            let mut i = input_mm.borrow_mut();
            i.mouse_dx += e.movement_x() as f32;
            i.mouse_dy += e.movement_y() as f32;
        }
    });
    canvas.add_event_listener_with_callback("mousemove", mm.as_ref().unchecked_ref())?;
    mm.forget();

    // Click → request pointer lock
    let canvas_cl = canvas.clone();
    let click = Closure::<dyn FnMut(_)>::new(move |_e: web_sys::MouseEvent| {
        let _ = canvas_cl.request_pointer_lock();
    });
    canvas.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
    click.forget();

    // Wheel → adjust 3P distance
    let cam_wh = cam.clone();
    let wh = Closure::<dyn FnMut(_)>::new(move |e: web_sys::WheelEvent| {
        e.prevent_default();
        let mut c = cam_wh.borrow_mut();
        c.distance = (c.distance + (e.delta_y() as f32) * 0.01).clamp(2.0, 20.0);
    });
    canvas.add_event_listener_with_callback("wheel", wh.as_ref().unchecked_ref())?;
    wh.forget();

    Ok(())
}
