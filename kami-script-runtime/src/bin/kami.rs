//! kami — the cross-platform packaging matrix on the CLI (ADR-0037 §4).
//!
//! Makes `kami-script-runtime::platform` actionable: ask which WASM host, texture
//! format, renderer backend, input default, and rustc triple a target needs — and
//! the exact cargo command to build the host for it. The `bb kami host/package`
//! tooling shells out to this instead of re-encoding the matrix.
//!
//!   cargo run -p kami-script-runtime --bin kami -- targets
//!   cargo run -p kami-script-runtime --bin kami -- plan ios

use kami_script_runtime::Target;

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
                    s.logic.label(),
                    s.tex.label(),
                    s.render.label(),
                    s.input.label(),
                    t.triple().unwrap_or("(NDA console SDK)"),
                );
            }
        }
        Some("plan") => match args.get(2).and_then(|s| Target::from_tag(s)) {
            Some(t) => print_plan(t),
            None => {
                eprintln!("kami: unknown target. one of: {}", tag_list());
                std::process::exit(2);
            }
        },
        // Machine-readable EDN — `bb kami` orchestration reads this so the matrix
        // has exactly one source of truth (this binary), not a re-encoding in bb.
        Some("spec") => match args.get(2).and_then(|s| Target::from_tag(s)) {
            Some(t) => print_spec(t),
            None => {
                eprintln!("kami: unknown target. one of: {}", tag_list());
                std::process::exit(2);
            }
        },
        _ => {
            eprintln!("usage:\n  kami targets          list the packaging matrix\n  kami plan <target>    build plan for one target ({0})\n  kami spec <target>    same, as machine-readable EDN", tag_list());
            std::process::exit(2);
        }
    }
}

fn print_spec(t: Target) {
    println!("{}", t.spec_edn()); // tested contract — see platform::tests
}

fn print_plan(t: Target) {
    let s = t.spec();
    println!("== kami plan: {} ==", t.tag());
    println!("  JIT allowed   : {}", s.jit_allowed);
    println!("  logic host    : {}{}", s.logic.label(), s.host_feature().map(|f| format!("  (feature: {f})")).unwrap_or_default());
    println!("  texture       : {}", s.tex.label());
    println!("  render backend: {}{}", s.render.label(), if s.console_seam { "  (NDA for_console_surface seam — out of repo)" } else { "" });
    println!("  default input : {}", s.input.label());
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
            println!("              logic = {} wasm; renderer = console seam.", s.logic.label());
        }
    }
}

fn tag_list() -> String {
    Target::all().iter().map(|t| t.tag()).collect::<Vec<_>>().join(" ")
}
