//! Round-trip a synthetic glTF document through the ingest pipeline:
//!
//!   inline glTF JSON → ingest::gltf → VehicleAssembly → JBeam + CDX 1.5
//!
//! ```bash
//! cargo run -p kami-cad-import --example gltf_minimal
//! ```
//!
//! No external file is loaded — the JSON is built in-memory so the
//! example stays hermetic. Real glTF files (decimated CAD exports) plug
//! in through the same `from_gltf_json` entry point.

use kami_cad_import::ingest::gltf::{IngestOptions, from_gltf_json};
use kami_cad_import::{jbeam_emit, sbom};

fn main() {
    let json = build_gltf_json();
    let asm = from_gltf_json(&json, &IngestOptions::default()).expect("ingest");
    let jbeam = jbeam_emit::emit(&asm).expect("jbeam");
    let cdx = sbom::emit(&asm, &sbom::CycloneDxOptions::default()).expect("cdx");

    let jbeam_v: serde_json::Value = serde_json::from_str(&jbeam).unwrap();
    let cdx_v: serde_json::Value = serde_json::from_str(&cdx).unwrap();

    eprintln!(
        "[gltf] {} parts / {:.0} kg / {} hardpoints / {} jbeam-nodes / {} jbeam-beams / {} cdx-components",
        asm.parts.len(),
        asm.total_mass_kg(),
        asm.hardpoints.len(),
        jbeam_v["nodes"].as_array().unwrap().len(),
        jbeam_v["beams"].as_array().unwrap().len(),
        cdx_v["components"].as_array().unwrap().len(),
    );
    println!("==== JBEAM bytes={} ====", jbeam.len());
    println!("==== CYCLONEDX bytes={} ====", cdx.len());
}

fn build_gltf_json() -> String {
    // Tiny 4-part vehicle: chassis + hood + 2 wheels. Mesh AABBs are
    // baked into POSITION-accessor min/max, the way real glTF exporters
    // (Blender, Cinema4D, FreeCAD) write them.
    r#"{
      "asset": {
        "version": "2.0",
        "extras": {
          "gftd_vehicle": {
            "id": "gltf-mini-v1",
            "display_name": "GLTF Mini Demo Vehicle",
            "revision": "0.1.0",
            "source": {
              "uri": "gltf://gftd/mini/v0.1.0.glb",
              "sha256": "2222222222222222222222222222222222222222222222222222222222222222",
              "license": "MIT"
            }
          }
        }
      },
      "scene": 0,
      "scenes": [
        {
          "nodes": [0],
          "extras": {
            "gftd_hardpoints": [
              { "id": "hp_hood", "from": "chassis", "to": "hood",
                "position": [0, 0.7, 0.95], "kind": "hinge" },
              { "id": "hp_wheel_l", "from": "chassis", "to": "wheel_l",
                "position": [-0.7, 0.3, 0], "kind": "bolt" },
              { "id": "hp_wheel_r", "from": "chassis", "to": "wheel_r",
                "position": [0.7, 0.3, 0], "kind": "bolt" }
            ]
          }
        }
      ],
      "nodes": [
        {
          "name": "chassis",
          "mesh": 0,
          "translation": [0, 0.3, 0],
          "children": [1, 2, 3],
          "extras": {
            "gftd_part": {
              "id": "chassis",
              "kind": "chassis",
              "material": "steel-hss",
              "mass_kg": 180,
              "supplier": { "name": "gftd", "cpe": "", "mpn": "" }
            }
          }
        },
        {
          "name": "hood",
          "mesh": 1,
          "translation": [0, 0.45, 0.85],
          "extras": {
            "gftd_part": {
              "id": "hood",
              "kind": "body",
              "material": "aluminium-sheet",
              "mass_kg": 9
            }
          }
        },
        {
          "name": "wheel_l",
          "mesh": 2,
          "translation": [-0.7, 0, 0.9],
          "extras": {
            "gftd_part": {
              "id": "wheel_l",
              "kind": "wheel",
              "material": "rubber",
              "mass_kg": 15,
              "supplier": { "name": "Bridgestone", "cpe": "", "mpn": "ER300-185-60-R14" }
            }
          }
        },
        {
          "name": "wheel_r",
          "mesh": 2,
          "translation": [0.7, 0, 0.9],
          "extras": {
            "gftd_part": {
              "id": "wheel_r",
              "kind": "wheel",
              "material": "rubber",
              "mass_kg": 15,
              "supplier": { "name": "Bridgestone", "cpe": "", "mpn": "ER300-185-60-R14" }
            }
          }
        }
      ],
      "meshes": [
        { "name": "chassis_mesh", "primitives": [{ "attributes": { "POSITION": 0 } }] },
        { "name": "hood_mesh",    "primitives": [{ "attributes": { "POSITION": 1 } }] },
        { "name": "wheel_mesh",   "primitives": [{ "attributes": { "POSITION": 2 } }] }
      ],
      "accessors": [
        { "min": [-0.85, 0.0, -1.10], "max": [0.85, 0.25, 1.10] },
        { "min": [-0.65, 0.0, -0.45], "max": [0.65, 0.05, 0.45] },
        { "min": [-0.09, -0.30, -0.30], "max": [0.09, 0.30, 0.30] }
      ]
    }"#
    .to_string()
}
