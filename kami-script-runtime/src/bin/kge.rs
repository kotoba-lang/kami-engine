//! kge — the cross-platform packaging matrix on the CLI (ADR-0037 §4).
//!
//! Makes `kami-script-runtime::platform` actionable: ask which WASM host, texture
//! format, renderer backend, input default, and rustc triple a target needs — and
//! the exact cargo command to build the host for it. The `bb kge host/package`
//! tooling shells out to this instead of re-encoding the matrix.
//!
//!   cargo run -p kami-script-runtime --bin kge -- targets
//!   cargo run -p kami-script-runtime --bin kge -- plan ios

use kami_script_runtime::{LogicHost, Target};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("targets") => {
            println!("{:<8} {:<6} {:<10} {:<8} {:<9} {:<8} triple", "TARGET", "JIT", "HOST", "TEX", "RENDER", "INPUT");
            for t in Target::all() {
                let s = t.spec();
                println!(
                    "{:<8} {:<6} {:<10} {:<8} {:<9} {:<8} {}",
                    t.tag(),
                    if s.jit_allowed { "yes" } else { "NO" },
                    host_label(s.logic),
                    tex_label(s.tex),
                    render_label(s.render),
                    input_label(s.input),
                    t.triple().unwrap_or("(NDA console SDK)"),
                );
            }
        }
        Some("plan") => match args.get(2).and_then(|s| Target::from_tag(s)) {
            Some(t) => print_plan(t),
            None => {
                eprintln!("kge: unknown target. one of: {}", tag_list());
                std::process::exit(2);
            }
        },
        _ => {
            eprintln!("usage:\n  kge targets          list the packaging matrix\n  kge plan <target>    build plan for one target ({})", tag_list());
            std::process::exit(2);
        }
    }
}

fn print_plan(t: Target) {
    let s = t.spec();
    println!("== kge plan: {} ==", t.tag());
    println!("  JIT allowed   : {}", s.jit_allowed);
    println!("  logic host    : {}{}", host_label(s.logic), s.host_feature().map(|f| format!("  (feature: {f})")).unwrap_or_default());
    println!("  texture       : {}", tex_label(s.tex));
    println!("  render backend: {}{}", render_label(s.render), if s.console_seam { "  (NDA for_console_surface seam — out of repo)" } else { "" });
    println!("  default input : {}", input_label(s.input));
    println!();
    match (t.triple(), s.host_feature()) {
        (Some(triple), Some(feature)) => {
            let flags = if feature == "backend-wasmtime" {
                String::new() // default feature
            } else {
                format!(" --no-default-features --features {feature}")
            };
            println!("  build host:\n    cargo build -p kami-clj-play --target {triple}{flags}");
        }
        (Some(triple), None) => {
            println!("  build (browser; guest runs in the page's wasm engine):\n    wasm-pack build --target web --target-dir {triple} kami-clj-host");
        }
        (None, _) => {
            println!("  build host: requires the {} console SDK toolchain (NDA, private repo).", t.tag());
            println!("              logic = {} wasm; renderer = console seam.", host_label(s.logic));
        }
    }
}

fn host_label(h: LogicHost) -> &'static str {
    match h {
        LogicHost::BrowserWasm => "browser",
        LogicHost::Wasmtime => "wasmtime",
        LogicHost::Wasmi => "wasmi",
    }
}
fn tex_label(t: kami_script_runtime::TexFmt) -> &'static str {
    use kami_script_runtime::TexFmt::*;
    match t {
        Ktx2Auto => "ktx2-auto",
        Ktx2Bcn => "ktx2-bcn",
        Ktx2Astc => "ktx2-astc",
    }
}
fn render_label(r: kami_script_runtime::RenderBackend) -> &'static str {
    use kami_script_runtime::RenderBackend::*;
    match r {
        WebGpu => "webgpu",
        Metal => "metal",
        Vulkan => "vulkan",
        Dx12 => "dx12",
        Console => "console",
    }
}
fn input_label(i: kami_script_runtime::InputDefault) -> &'static str {
    use kami_script_runtime::InputDefault::*;
    match i {
        Keyboard => "keyboard",
        Touch => "touch",
        Gamepad => "gamepad",
    }
}
fn tag_list() -> String {
    Target::all().iter().map(|t| t.tag()).collect::<Vec<_>>().join(" ")
}
