//! Face-rig data tier — `kami-character`'s MetaHuman FACS face rig as parity-tested EDN.
//!
//! `ControlRig::metahuman_face_rig()` builds its node graph from a flat table of
//! FACS Action-Unit → bone mappings (`add_au_bone(au, bone, axis, max_angle)`, each
//! expanding to a control → multiply → rotation-axis node triple). That **description**
//! is what moves to EDN here (ADR-0046 / ADR-0038); the rig **evaluation** stays native
//! Rust. The EDN-driven [`face_rig_from_edn`] rebuilds the *same*
//! [`kami_character::control_rig::ControlRig`], asserted node-for-node `==` the
//! compiled-in `metahuman_face_rig()` in `tests/face_rig_parity.rs`.
//!
//! ## EDN shape (see `data/face_rig.edn`)
//!
//! ```edn
//! {:character/face-rig
//!  {:au-bones [{:au "AU12_L" :bone 34 :axis [0.0 0.0 1.0] :max-angle 0.3} ...]}}
//! ```

use std::collections::HashMap;

use kami_character::control_rig::{ControlRig, RigNode, RigNodeType};
use kami_scene::{EdnValue, mget, num, root_map, vec3};

/// The canonical FACS face-rig CONFIG shipped with this crate (the AU→bone table).
pub const FACE_RIG_EDN: &str = include_str!("../data/face_rig.edn");

/// Errors raised while loading the face-rig CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum FaceRigError {
    /// The EDN source did not parse to a top-level map.
    #[error("face-rig EDN root is not a map")]
    NotAMap,
    /// `:character/face-rig` → `:au-bones` was missing or not a vector.
    #[error("`:character/face-rig` `:au-bones` missing or not a vector")]
    NoAuBones,
}

/// One FACS Action-Unit → bone mapping row (mirrors an `add_au_bone` call).
#[derive(Debug, Clone, PartialEq)]
pub struct AuBone {
    /// FACS Action-Unit / control input name (e.g. `"AU12_L"`).
    pub au: String,
    /// Target MetaHuman skeleton bone index.
    pub bone: usize,
    /// Rotation axis applied at the target bone.
    pub axis: [f32; 3],
    /// Rotation angle (radians) when the AU is fully active (1.0).
    pub max_angle: f32,
}

impl AuBone {
    /// Read one row from its EDN map (tolerant: missing → defaults, int↔float coercion).
    pub fn from_map(m: &std::collections::BTreeMap<EdnValue, EdnValue>) -> Self {
        let au = mget(m, "au")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let bone = mget(m, "bone")
            .and_then(|v| v.as_integer())
            .unwrap_or(0)
            .max(0) as usize;
        Self {
            au,
            bone,
            axis: vec3(mget(m, "axis")),
            max_angle: num(mget(m, "max-angle")),
        }
    }
}

/// Parse the `:character/face-rig` → `:au-bones` table from EDN `src`.
pub fn au_bones_from_edn(src: &str) -> Result<Vec<AuBone>, FaceRigError> {
    let root = root_map(src).ok_or(FaceRigError::NotAMap)?;
    let rig = mget(&root, "character/face-rig")
        .and_then(|v| v.as_map())
        .ok_or(FaceRigError::NoAuBones)?;
    let rows = mget(rig, "au-bones")
        .and_then(|v| v.as_vector())
        .ok_or(FaceRigError::NoAuBones)?;
    Ok(rows
        .iter()
        .filter_map(|v| v.as_map().map(AuBone::from_map))
        .collect())
}

/// Build the real [`ControlRig`] from an AU→bone table, replaying the exact
/// `metahuman_face_rig()` expansion: each row → a `ctrl_*` ControlInput node, a `mul_*`
/// Multiply(max_angle) node, and a `rot_*` RotationAxis(axis) node targeting the bone,
/// wired control → multiply → rotation. Node indexing matches the builtin so the graphs
/// are identical.
pub fn build_face_rig(rows: &[AuBone]) -> ControlRig {
    let mut nodes: Vec<RigNode> = Vec::new();
    let mut idx = 0usize;
    for row in rows {
        let control_idx = idx;
        nodes.push(RigNode {
            name: format!("ctrl_{}", row.au),
            node_type: RigNodeType::ControlInput {
                control_name: row.au.clone(),
            },
            inputs: vec![],
            target_bone: None,
        });
        idx += 1;

        let mul_idx = idx;
        nodes.push(RigNode {
            name: format!("mul_{}", row.au),
            node_type: RigNodeType::Multiply {
                factor: row.max_angle,
            },
            inputs: vec![(control_idx, 0u32)],
            target_bone: None,
        });
        idx += 1;

        nodes.push(RigNode {
            name: format!("rot_{}", row.au),
            node_type: RigNodeType::RotationAxis { axis: row.axis },
            inputs: vec![(mul_idx, 0u32)],
            target_bone: Some(row.bone),
        });
        idx += 1;
    }

    let eval_order: Vec<usize> = (0..nodes.len()).collect();
    ControlRig {
        nodes,
        eval_order,
        controls: HashMap::new(),
        bone_outputs: HashMap::new(),
    }
}

/// Load the face rig from EDN `src` into the real [`ControlRig`].
pub fn face_rig_from_edn(src: &str) -> Result<ControlRig, FaceRigError> {
    Ok(build_face_rig(&au_bones_from_edn(src)?))
}

/// Convenience: the AU→bone table loaded from the crate-shipped [`FACE_RIG_EDN`].
pub fn shipped_au_bones() -> Result<Vec<AuBone>, FaceRigError> {
    au_bones_from_edn(FACE_RIG_EDN)
}

/// Convenience: the real [`ControlRig`] built from the shipped [`FACE_RIG_EDN`].
pub fn shipped_face_rig() -> Result<ControlRig, FaceRigError> {
    face_rig_from_edn(FACE_RIG_EDN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_nineteen_au_rows() {
        let rows = shipped_au_bones().expect("face_rig.edn parses");
        assert_eq!(rows.len(), 19, "19 AU→bone mappings");
    }

    #[test]
    fn rebuilt_rig_has_three_nodes_per_row() {
        let rig = shipped_face_rig().expect("face rig builds");
        assert_eq!(
            rig.nodes.len(),
            19 * 3,
            "control + multiply + rotation per AU"
        );
        assert_eq!(rig.eval_order.len(), rig.nodes.len());
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(
            au_bones_from_edn("42"),
            Err(FaceRigError::NotAMap)
        ));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            au_bones_from_edn("{:other 1}"),
            Err(FaceRigError::NoAuBones)
        ));
    }

    #[test]
    fn int_bone_and_float_angle_coerce() {
        let rows = au_bones_from_edn(
            "{:character/face-rig {:au-bones [{:au \"X\" :bone 5 :axis [1 0 0] :max-angle 1}]}}",
        )
        .unwrap();
        assert_eq!(rows[0].bone, 5);
        assert_eq!(rows[0].axis, [1.0, 0.0, 0.0], "int vector coerces");
        assert_eq!(rows[0].max_angle, 1.0, "int angle coerces");
    }
}
