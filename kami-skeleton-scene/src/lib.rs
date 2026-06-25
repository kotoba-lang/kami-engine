//! kami-skeleton-scene — EDN authoring surface for `kami-skeleton`'s default
//! humanoid joint-constraint table (the anatomical Euler-angle rotation limits,
//! ADR-0040 "animation: constraints / retarget maps as EDN").
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-input-scene` for the
//! skeletal-animation system: it turns canonical `:skeleton/humanoid-constraints`
//! EDN (an *ordered* table of `[joint-name {:min-deg [..] :max-deg [..]}]` pairs)
//! into the real [`kami_skeleton::JointConstraint`] engine struct, the same way
//! the hardcoded [`kami_skeleton::default_humanoid_constraints`] table builds it.
//! It re-uses the tolerant `kami-scene` accessors the same way games parse
//! `scene.edn` (namespaced keywords match on `ns/name`, malformed entries skip).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot skeletal evaluation (`Skeleton::evaluate`, `evaluate_constrained`,
//! `solve_ik_ccd`, `JointConstraint::clamp`) stays native Rust (`kami-skeleton`).
//! The constraint *table* — which bone has which Euler-angle limit — is
//! **init-time CONFIG** read once when an app builds its constraint index, so it
//! is safe to move to EDN. `kami-skeleton` itself stays untouched; the EDN
//! dependency lives only here. The compiled-in `default_humanoid_constraints()`
//! remains the [`builtin_humanoid_constraints`] fallback and is parity-tested
//! (bit-for-bit f32) against the shipped EDN ([`crate::HUMANOID_EDN`]).
//!
//! ## Degrees in EDN, radians at load (exact-f32 parity)
//!
//! The Rust source authors each limit in degrees and converts with
//! `let d = std::f32::consts::PI / 180.0;` then `<deg> * d`. This crate stores the
//! *same* degree literals in EDN ([`min-deg`]/[`max-deg`]) and converts with the
//! *identical* factor ([`DEG_TO_RAD`]) in the *same* `f32` arithmetic, so each
//! loaded angle is **bit-for-bit equal** to the Rust `<deg> * d` it mirrors.
//!
//! ## EDN shape (see `data/humanoid.edn`)
//!
//! ```edn
//! {:skeleton/humanoid-constraints
//!  [["head" {:min-deg [-60.0 -80.0 -40.0] :max-deg [60.0 80.0 40.0]}]
//!   ["neck" {:min-deg [-30.0 -45.0 -30.0] :max-deg [30.0 45.0 30.0]}]
//!   ... all 13, IN ORDER ...]}
//! ```
//!
//! The table is an **ordered** vector of `[joint-name {…}]` pairs (order matters:
//! it mirrors the `default_humanoid_constraints()` declaration order). The
//! joint-name is a string that round-trips exactly (`"leftUpperArm"`); the limits
//! are `:min-deg` / `:max-deg`, each a `[x y z]` 3-vector of degrees.

use kami_scene::{mget, root_map, vec3, EdnValue};
use kami_skeleton::{default_humanoid_constraints, JointConstraint};

/// EDN-authored animation clips (`:dance/clips` → [`kami_skeleton::AnimationClip`]).
pub mod clip;
pub use clip::clip_from_edn;

/// The canonical default humanoid joint-constraint table shipped with this crate.
/// This is the source of truth; the compiled-in `default_humanoid_constraints()`
/// table is the parity-tested mirror.
pub const HUMANOID_EDN: &str = include_str!("../data/humanoid.edn");

/// Degrees → radians factor. **Bit-identical** to the `d` in
/// `kami_skeleton::default_humanoid_constraints` (`std::f32::consts::PI / 180.0`),
/// so multiplying an EDN degree literal by this reproduces the Rust `<deg> * d`
/// `f32` result exactly.
pub const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

/// Joint names in the order they are declared in
/// `default_humanoid_constraints()` (also the order shipped in `humanoid.edn`).
/// Iteration source for `builtin`/parity; kept here (not in `kami-skeleton`) so
/// the engine crate stays untouched.
pub const ALL_JOINT_NAMES: [&str; 13] = [
    "head",
    "neck",
    "spine",
    "chest",
    "hips",
    "leftUpperArm",
    "rightUpperArm",
    "leftLowerArm",
    "rightLowerArm",
    "leftUpperLeg",
    "rightUpperLeg",
    "leftLowerLeg",
    "rightLowerLeg",
];

/// Errors raised while loading the humanoid constraint table from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("humanoid-constraints EDN root is not a map")]
    NotAMap,
    /// The `:skeleton/humanoid-constraints` table was missing or not a vector.
    #[error("`:skeleton/humanoid-constraints` missing or not a vector")]
    NoTable,
}

/// Read a `[x y z]` degree 3-vector and convert to radians with the SAME `f32`
/// arithmetic the Rust source uses (`<deg> * d`), so the result is bit-identical.
///
/// `kami_scene::vec3` coerces int↔float and pads short vectors with `0.0`; the
/// degree value is read as `f32` and multiplied by [`DEG_TO_RAD`].
fn deg_vec3_to_rad(v: Option<&EdnValue>) -> [f32; 3] {
    let d = vec3(v);
    [d[0] * DEG_TO_RAD, d[1] * DEG_TO_RAD, d[2] * DEG_TO_RAD]
}

/// Build one real [`JointConstraint`] from its EDN map (`{:min-deg [..] :max-deg
/// [..]}`). Degrees are read and converted to radians with the same `f32`
/// multiply the Rust source uses, so the `[f32;3]` limits are bit-for-bit equal.
pub fn constraint_from_map(map: &std::collections::BTreeMap<EdnValue, EdnValue>) -> JointConstraint {
    JointConstraint {
        min: deg_vec3_to_rad(mget(map, "min-deg")),
        max: deg_vec3_to_rad(mget(map, "max-deg")),
    }
}

/// The compiled-in fallback / parity oracle: the real
/// `kami_skeleton::default_humanoid_constraints()`, with names owned (`String`).
/// This is what the shipped EDN is parity-tested against.
pub fn builtin_humanoid_constraints() -> Vec<(String, JointConstraint)> {
    default_humanoid_constraints()
        .into_iter()
        .map(|(name, c)| (name.to_string(), c))
        .collect()
}

/// Parse the whole `:skeleton/humanoid-constraints` table from EDN `src` into an
/// **ordered** `Vec<(joint-name, JointConstraint)>` (order preserved from the
/// vector). A pair malformed in *shape* (not a `[name {..}]` 2-vector, non-string
/// name, non-map limits) is skipped, matching how the rest of the data tier
/// degrades on shape errors.
pub fn humanoid_constraints_from_edn(src: &str) -> Result<Vec<(String, JointConstraint)>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = mget(&root, "skeleton/humanoid-constraints")
        .and_then(|v| v.as_vector())
        .ok_or(Error::NoTable)?;

    let mut out = Vec::with_capacity(table.len());
    for pair in table {
        // Each entry is a [joint-name {limits}] 2-vector.
        let Some(slots) = pair.as_vector() else { continue };
        let (Some(name_v), Some(limits_v)) = (slots.first(), slots.get(1)) else {
            continue;
        };
        let Some(name) = name_v.as_string() else { continue };
        let Some(limits) = limits_v.as_map() else { continue };
        out.push((name.to_string(), constraint_from_map(limits)));
    }
    Ok(out)
}

/// Convenience: load & rebuild the humanoid constraint table from the shipped
/// [`HUMANOID_EDN`].
pub fn shipped_humanoid_constraints() -> Result<Vec<(String, JointConstraint)>, Error> {
    humanoid_constraints_from_edn(HUMANOID_EDN)
}

/// Compare two `[f32;3]` arrays for exact (bit-for-bit) `f32` equality. Used by
/// parity tests because [`JointConstraint`] derives no `PartialEq`.
pub fn limits_eq(a: &[f32; 3], b: &[f32; 3]) -> bool {
    a[0] == b[0] && a[1] == b[1] && a[2] == b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_joints_in_order() {
        let table = shipped_humanoid_constraints().expect("humanoid.edn parse");
        assert_eq!(table.len(), 13, "13 joints shipped");
        for (i, name) in ALL_JOINT_NAMES.iter().enumerate() {
            assert_eq!(table[i].0, *name, "joint[{i}] name in order");
        }
    }

    #[test]
    fn deg_to_rad_matches_source_factor() {
        // Same `f32` factor the Rust source uses → exact reproduction of `<deg> * d`.
        let d = std::f32::consts::PI / 180.0;
        assert_eq!(DEG_TO_RAD, d);
        assert_eq!(60.0f32 * DEG_TO_RAD, 60.0f32 * d);
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(
            humanoid_constraints_from_edn("42"),
            Err(Error::NotAMap)
        ));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            humanoid_constraints_from_edn("{:other 1}"),
            Err(Error::NoTable)
        ));
    }
}
