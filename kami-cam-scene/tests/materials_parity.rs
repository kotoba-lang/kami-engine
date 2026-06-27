//! Parity tests: the shipped EDN must faithfully reproduce kami-cam's compiled-in
//! stock-material presets. This is the whole point of the data tier (ADR-0046) — EDN
//! becomes the source of truth with *behaviour unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a value loaded from
//! `materials.edn` against `CamMaterial::aluminum_6061()` … (called here, not
//! transcribed). The decimal literals (2.70, 7.87, 4.43, 1.04, 0.66 / 95.0, 163.0,
//! 334.0, 10.0, 6.0) parse to the same f64 as the Rust literals, so parity is exact.

use kami_cam::CamMaterial;
use kami_cam_scene::{
    ALL_MATERIAL_IDS, CamMaterialSpec, MATERIALS_EDN, builtin_material, materials_from_edn,
    shipped_materials, spec_to_cam_material,
};

/// The hardcoded `CamMaterial` for an id — the parity oracle (real Rust, not copied).
fn oracle(id: &str) -> CamMaterial {
    match id {
        "aluminum-6061" => CamMaterial::aluminum_6061(),
        "steel-1045" => CamMaterial::steel_1045(),
        "titanium-ti6al4v" => CamMaterial::titanium_ti6al4v(),
        "abs-plastic" => CamMaterial::abs_plastic(),
        "wood-oak" => CamMaterial::wood_oak(),
        other => panic!("no oracle for {other}"),
    }
}

/// For each shipped material, every field loaded from materials.edn == the value from
/// the Rust `CamMaterial::*()` builder.
#[test]
fn materials_edn_matches_builtin() {
    let loaded = materials_from_edn(MATERIALS_EDN).expect("materials.edn presets parse");
    assert_eq!(loaded.len(), 5, "all five materials present in EDN");

    for id in ALL_MATERIAL_IDS {
        let o = CamMaterialSpec::from_cam_material(&oracle(id));
        assert_eq!(loaded[id], o, "{id}: full CamMaterialSpec parity");

        // The `builtin_material` oracle helper agrees with what we read off the struct.
        let built = builtin_material(id).expect("builtin material");
        assert_eq!(loaded[id], built, "{id}: EDN == builtin_material()");
    }

    // The shipped-materials convenience loader yields the same thing.
    let shipped = shipped_materials().expect("shipped materials");
    for id in ALL_MATERIAL_IDS {
        assert_eq!(shipped[id], loaded[id], "{id}: shipped == loaded");
    }
}

/// `spec_to_cam_material` reconstructs a `CamMaterial` whose fields equal the hardcoded
/// preset's — the real engine struct, behaviourally identical.
#[test]
fn spec_to_cam_material_matches_hardcoded() {
    let loaded = materials_from_edn(MATERIALS_EDN).unwrap();
    for id in ALL_MATERIAL_IDS {
        let m = spec_to_cam_material(&loaded[id]);
        let o = oracle(id);
        assert_eq!(m.name, o.name, "{id}: name");
        assert_eq!(m.density, o.density, "{id}: density (exact f64)");
        assert_eq!(m.hardness, o.hardness, "{id}: hardness (exact f64)");
    }
}
