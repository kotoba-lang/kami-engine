//! Render through `IrRenderer` with an **image-based (HDR env map) IBL**: a synthetic
//! equirectangular HDR (sky gradient + bright sun disk + ground) is fed to `set_env_map`,
//! and metallic cubes of increasing roughness reflect it — the sun reflection sharpens on
//! smooth metal and blurs (via the mip chain) as roughness rises. Proves the env-map
//! sampling path (equirect lookup + roughness→LOD) on a real device.
//! `cargo run -p kami-webgpu-rs --example render_ir_png --target aarch64-apple-darwin`

use kami_webgpu_rs::render_ir_to_pixels_env;

const SCENE: &str = r#"
{:camera {:eye [0 2.2 11] :target [0 1.4 0] :fov 0.8 :near 0.1 :far 200.0}
 :globals {:horizon [0.45 0.62 0.85]}
 :env {:ambient [0.04 0.05 0.07] :ground [0.10 0.10 0.10] :ibl {:intensity 1.0 :url "studio.hdr"}}
 :lights [{:kind :directional :color [1.0 0.97 0.9] :intensity 0.4 :dir [-0.5 -0.7 -0.4]}]
 ;; a near-mirror metal floor reflects the equirect sky + sun (varying per fragment);
 ;; the row of metal cubes shows roughness sharpening/blurring the reflection.
 :instances [{:pos [0 -0.5 0] :color [0.7 0.72 0.78] :size [80 1] :metallic 0.9 :roughness 0.06}
             {:pos [-4.5 0 0] :color [0.95 0.95 1.0] :size [1.8 2.6] :metallic 1.0 :roughness 0.03}
             {:pos [-1.5 0 0] :color [0.95 0.95 1.0] :size [1.8 2.6] :metallic 1.0 :roughness 0.18}
             {:pos [1.5 0 0]  :color [0.95 0.95 1.0] :size [1.8 2.6] :metallic 1.0 :roughness 0.38}
             {:pos [4.5 0 0]  :color [0.95 0.95 1.0] :size [1.8 2.6] :metallic 1.0 :roughness 0.6}]}
"#;

/// Synthetic equirectangular HDR: zenith→horizon sky gradient, a bright HDR sun disk, and a
/// darker ground hemisphere. Returns RGBA-f32 (`w*h*4`). Matches the shader's equirect uv:
/// yaw = (u-0.5)·2π, pitch = v·π, dir = (sinθ·cosφ, cosθ, sinθ·sinφ).
fn make_env(w: u32, h: u32) -> Vec<f32> {
    use std::f32::consts::PI;
    // sun direction (unit): low and in front of the camera so the mirror floor reflects it
    let (sy, sp) = (-1.571f32, 1.2f32); // sun yaw, pitch (radians from zenith; ~1.57 = horizon)
    let sun = [sp.sin() * sy.cos(), sp.cos(), sp.sin() * sy.sin()];
    let mut px = vec![0f32; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;
            let yaw = (u - 0.5) * 2.0 * PI;
            let pitch = v * PI;
            let (s, c) = (pitch.sin(), pitch.cos());
            let dir = [s * yaw.cos(), c, s * yaw.sin()];
            let up = c; // dir.y in [-1,1]
            let mut col = if up >= 0.0 {
                // sky: horizon (warm pale) → zenith (deep blue)
                let t = up.powf(0.6);
                [
                    0.55 * (1.0 - t) + 0.10 * t,
                    0.70 * (1.0 - t) + 0.22 * t,
                    0.95 * (1.0 - t) + 0.55 * t,
                ]
            } else {
                // ground hemisphere (dim, slightly warm)
                let g = 0.06 + 0.04 * (1.0 + up);
                [g * 1.1, g, g * 0.9]
            };
            // bright HDR sun disk + soft glow
            let dot = dir[0] * sun[0] + dir[1] * sun[1] + dir[2] * sun[2];
            if dot > 0.9995 {
                col = [60.0, 55.0, 45.0]; // core (HDR, well above 1.0 → blooms + sharp reflection)
            } else if dot > 0.95 {
                let g = ((dot - 0.95) / (0.9995 - 0.95)).powf(2.0) * 6.0;
                col = [col[0] + g, col[1] + g * 0.95, col[2] + g * 0.8];
            }
            let i = ((y * w + x) * 4) as usize;
            px[i] = col[0]; px[i + 1] = col[1]; px[i + 2] = col[2]; px[i + 3] = 1.0;
        }
    }
    px
}

fn main() {
    let (w, h) = (960u32, 540u32);
    let env = make_env(1024, 512);
    let px = render_ir_to_pixels_env(SCENE, w, h, 4, (1024, 512, env));

    let nonblack = px.iter().any(|&b| b > 8);
    let varied = px.chunks(4).any(|p| p[0..4] != px[0..4]);
    println!("rendered {w}×{h} with HDR env map — varied={varied} nonblack={nonblack}");
    image::save_buffer("native-ir.png", &px, w, h, image::ExtendedColorType::Rgba8).unwrap();
    println!("wrote native-ir.png");
}
