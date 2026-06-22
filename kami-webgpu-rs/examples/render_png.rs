//! Render the demo city with the native executor and save a PNG.
//! `cargo run -p kami-webgpu-rs --example render_png --target aarch64-apple-darwin`
//! A viewable golden frame proving the native Rust/wgpu path renders the same scene
//! (PBR + shadows) the web does — no window.

use kami_webgpu_rs::{demo_city, render};

fn main() {
    let (g, insts) = demo_city();
    let (w, h) = (900u32, 560u32);
    let px = render(&g, &insts, w, h);
    image::save_buffer("native-royale.png", &px, w, h, image::ExtendedColorType::Rgba8).unwrap();
    println!("wrote native-royale.png — {} instances", insts.len());
}
