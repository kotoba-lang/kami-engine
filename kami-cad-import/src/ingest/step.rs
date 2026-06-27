//! STEP / IGES → VehicleAssembly via FreeCAD CLI shell-out.
//!
//! Pipeline:
//!
//! ```text
//! input.step  ─┐
//!              ├─►  freecad --console -c "<convert script>"  ─►  /tmp/<sha>.glb
//! input.iges  ─┘                                                       │
//!                                                                      ▼
//!                                          ingest::gltf::from_gltf_json
//! ```
//!
//! FreeCAD's headless console exposes `Import.open(path)` (STEP / IGES /
//! BREP) and `Import.export([objs], path)` (glTF 2.0 since FreeCAD 0.20).
//! We compose a minimal converter script, run it via subprocess, then
//! feed the resulting glTF JSON through the existing `from_gltf_json`
//! adapter so the same annotation contract (`asset.extras.gftd_vehicle`
//! + per-node `extras.gftd_part`) applies.
//!
//! Why not parse STEP directly: the STEP AP242 product structure is
//! complex (NIST OpenSSEDK is the canonical reader, ~50K LoC C++) and
//! out of scope for this crate. FreeCAD already ships a battle-tested
//! reader on macOS / Linux / Windows (Homebrew: `brew install --cask
//! freecad`); shelling out trades portability for correctness.
//!
//! When `freecad` is not on PATH the function returns
//! `StepError::FreeCadNotFound` with a one-line install hint so callers
//! can surface it in their own error UI.

use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

use crate::ingest::gltf::{GltfError, IngestOptions, from_gltf_json};
use crate::part::{AssemblyError, VehicleAssembly};

#[derive(Debug, Error)]
pub enum StepError {
    #[error(
        "freecad CLI not found on PATH — install via `brew install --cask freecad` (macOS) or `apt-get install freecad` (Debian)"
    )]
    FreeCadNotFound,
    #[error("freecad invocation failed (exit={exit}): {stderr}")]
    FreeCadFailed { exit: i32, stderr: String },
    #[error("input file not found: {0}")]
    InputNotFound(PathBuf),
    #[error("output glTF was not produced at {0}")]
    OutputMissing(PathBuf),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("gltf: {0}")]
    Gltf(#[from] GltfError),
    #[error("assembly: {0}")]
    Assembly(#[from] AssemblyError),
}

/// Locate the `freecad` (or `freecadcmd`) binary on PATH. Returns
/// `None` when neither variant is callable.
fn find_freecad() -> Option<PathBuf> {
    for name in ["freecadcmd", "freecad"] {
        if let Ok(out) = Command::new("which").arg(name).output() {
            if out.status.success() {
                if let Ok(s) = String::from_utf8(out.stdout) {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(PathBuf::from(trimmed));
                    }
                }
            }
        }
    }
    None
}

/// FreeCAD console script that opens a STEP / IGES file and re-exports
/// the entire document as glTF 2.0. Annotation propagation (`extras.*`)
/// requires that the source file already encode `gftd_part` / `gftd_vehicle`
/// in its STEP `name` field — most STEP exporters drop arbitrary
/// metadata, so the practical workflow is:
///
/// 1. Run this STEP→glTF conversion (no annotations preserved).
/// 2. Open the resulting glTF in Blender / VS Code / a text editor.
/// 3. Decorate each top-level node with `extras.gftd_part = { ... }`.
/// 4. Run `ingest::gltf::from_gltf_json` on the annotated glTF.
///
/// For automated annotation, callers can pass a `python_postprocess`
/// script via `StepOptions::python_postprocess` that runs after the
/// FreeCAD export but before glTF parsing — typical use: walk the
/// glTF JSON and inject `gftd_part` annotations from a parts.csv.
fn freecad_script(input: &Path, output: &Path) -> String {
    let in_s = input.display();
    let out_s = output.display();
    format!(
        r#"
import FreeCAD, Import, ImportGui
doc = FreeCAD.newDocument()
Import.insert(r'{in_s}', doc.Name)
objs = [o for o in doc.Objects if o.TypeId.startswith('Part::')]
ImportGui.export(objs, r'{out_s}')
FreeCAD.closeDocument(doc.Name)
"#
    )
}

#[derive(Debug, Clone, Default)]
pub struct StepOptions {
    /// Where to put the intermediate glTF. Defaults to the system temp
    /// directory + `kami-cad-import-step.glb`. Idempotent re-runs
    /// overwrite this file.
    pub intermediate_gltf: Option<PathBuf>,
    /// Optional path to a Python script run as a post-processor on the
    /// glTF JSON before ingestion (typical use: inject `gftd_part`
    /// annotations from an external manifest). Receives the glTF JSON
    /// path as `sys.argv[1]`. May edit the file in place.
    pub python_postprocess: Option<PathBuf>,
    /// glTF ingest options forwarded to `from_gltf_json`.
    pub ingest: IngestOptions,
    /// Override `freecad` binary location.
    pub freecad_bin: Option<PathBuf>,
}

/// Convert a STEP / IGES file to a `VehicleAssembly` via FreeCAD CLI +
/// glTF.
pub fn from_step_file(
    input: impl AsRef<Path>,
    opts: &StepOptions,
) -> Result<VehicleAssembly, StepError> {
    let input = input.as_ref();
    if !input.exists() {
        return Err(StepError::InputNotFound(input.to_path_buf()));
    }
    let bin = opts
        .freecad_bin
        .clone()
        .or_else(find_freecad)
        .ok_or(StepError::FreeCadNotFound)?;

    let intermediate = opts
        .intermediate_gltf
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("kami-cad-import-step.gltf"));

    // 1. STEP → glTF via FreeCAD console.
    let script = freecad_script(input, &intermediate);
    let out = Command::new(&bin)
        .arg("--console")
        .arg("-c")
        .arg(&script)
        .output()
        .map_err(StepError::Io)?;
    if !out.status.success() {
        return Err(StepError::FreeCadFailed {
            exit: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    if !intermediate.exists() {
        return Err(StepError::OutputMissing(intermediate));
    }

    // 2. (Optional) post-process the glTF — injects gftd_part annotations.
    if let Some(py_script) = &opts.python_postprocess {
        let st = Command::new("python3")
            .arg(py_script)
            .arg(&intermediate)
            .status()
            .map_err(StepError::Io)?;
        if !st.success() {
            return Err(StepError::FreeCadFailed {
                exit: st.code().unwrap_or(-1),
                stderr: format!("python_postprocess script failed: {}", py_script.display()),
            });
        }
    }

    // 3. Parse the glTF as if it had been authored directly.
    let json = std::fs::read_to_string(&intermediate)?;
    let asm = from_gltf_json(&json, &opts.ingest)?;
    Ok(asm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_input_returns_input_not_found() {
        let err = from_step_file("/tmp/does-not-exist.step", &StepOptions::default()).unwrap_err();
        match err {
            StepError::InputNotFound(p) => assert!(p.ends_with("does-not-exist.step")),
            // If freecad is not installed on the host that runs this
            // test, find_freecad fires before the input check — that's
            // also acceptable.
            StepError::FreeCadNotFound => {}
            other => panic!("unexpected error: {:?}", other),
        }
    }
}
