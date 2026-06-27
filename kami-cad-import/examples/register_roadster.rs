//! Print a `curl` command that registers the roadster's SBOM with
//! `sbom.etzhayyim.com` (lexicon `app.etzhayyim.sbom.registerArtifact`).
//!
//! ```bash
//! export etzhayyim_TOKEN=$(gftd agent-token --lxm app.etzhayyim.sbom.registerArtifact)
//! cargo run -p kami-cad-import --example register_roadster | bash
//! ```
//!
//! Or `--no-auth` to print the envelope only (for diffing).

use kami_cad_import::demos::roadster_na;
use kami_cad_import::register::{RegisterOptions, curl_command, register_request};

fn main() {
    let asm = roadster_na();

    let dry = std::env::args().any(|a| a == "--dry-run");
    if dry {
        let req = register_request(&asm, &RegisterOptions::default()).expect("envelope");
        eprintln!("[register] POST {}", req.url);
        eprintln!("[register] body bytes={}", req.body.len());
        eprintln!(
            "[register] vehicleId={} parts={}",
            asm.vehicle_id,
            asm.parts.len()
        );
        return;
    }

    let cmd = curl_command(&asm, &RegisterOptions::default()).expect("curl");
    println!("{cmd}");
}
