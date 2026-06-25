//! Parity tests: the shipped EDN must faithfully reproduce kami-skeleton's
//! compiled-in `default_humanoid_constraints()` table — every joint, in order,
//! every min/max `[f32;3]` BIT-FOR-BIT equal. This is the whole point of the data
//! tier (ADR-0038 / ADR-0040): EDN becomes the source of truth with *behaviour
//! unchanged*.
//!
//! The oracle is the REAL Rust: each assertion compares a constraint rebuilt from
//! `humanoid.edn` against `kami_skeleton::default_humanoid_constraints()` (called
//! here, not transcribed). Order is load-bearing (the table is declaration-ordered),
//! so the comparison walks the vec index-by-index.
//!
//! `kami_skeleton::JointConstraint` derives no `PartialEq` (it is just `pub min:
//! [f32;3]`, `pub max: [f32;3]`), so we compare the `[f32;3]` arrays directly with
//! exact `f32` `==`. The degree→radian conversion in the loader uses the SAME `f32`
//! factor (`std::f32::consts::PI / 180.0`) and the SAME `<deg> * d` arithmetic as the
//! Rust source, so each angle reproduces bit-for-bit. `kami-skeleton` is untouched.

use kami_skeleton::{default_humanoid_constraints, JointConstraint};
use kami_skeleton_scene::{
    builtin_humanoid_constraints, constraint_from_map, humanoid_constraints_from_edn, limits_eq,
    shipped_humanoid_constraints, Error, ALL_JOINT_NAMES, HUMANOID_EDN,
};

/// Assert two `JointConstraint`s are EXACTLY (bit-for-bit f32) equal. No
/// `PartialEq` on the struct, so compare the `[f32;3]` arrays directly.
fn assert_constraint_eq(name: &str, loaded: &JointConstraint, want: &JointConstraint) {
    for axis in 0..3 {
        assert_eq!(
            loaded.min[axis], want.min[axis],
            "{name}: min[{axis}] exact f32"
        );
        assert_eq!(
            loaded.max[axis], want.max[axis],
            "{name}: max[{axis}] exact f32"
        );
    }
    assert!(limits_eq(&loaded.min, &want.min), "{name}: full min [f32;3]");
    assert!(limits_eq(&loaded.max, &want.max), "{name}: full max [f32;3]");
}

/// The shipped EDN table == the real Rust `default_humanoid_constraints()` —
/// same joint count (13), same names in order, every min/max `[f32;3]` exactly equal.
#[test]
fn humanoid_edn_matches_builtin() {
    let loaded = humanoid_constraints_from_edn(HUMANOID_EDN).expect("humanoid.edn parse");
    let oracle = default_humanoid_constraints();

    // Same joint count.
    assert_eq!(loaded.len(), 13, "13 joints loaded from EDN");
    assert_eq!(oracle.len(), 13, "13 joints in the Rust oracle");
    assert_eq!(loaded.len(), oracle.len(), "EDN vs Rust joint count");

    // Same names, in order; same constraints, exact f32.
    for (i, (loaded_pair, oracle_pair)) in loaded.iter().zip(oracle.iter()).enumerate() {
        let (lname, lc) = loaded_pair;
        let (oname, oc) = oracle_pair;
        assert_eq!(lname, oname, "joint[{i}] name in order");
        assert_eq!(lname, ALL_JOINT_NAMES[i], "joint[{i}] name == ALL_JOINT_NAMES");
        assert_constraint_eq(lname, lc, oc);
    }

    // The `builtin_humanoid_constraints` helper agrees too.
    let built = builtin_humanoid_constraints();
    assert_eq!(built.len(), loaded.len(), "builtin len");
    for (i, ((bn, bc), (ln, lc))) in built.iter().zip(loaded.iter()).enumerate() {
        assert_eq!(bn, ln, "builtin[{i}] name");
        assert_constraint_eq(bn, bc, lc);
    }

    // The shipped convenience loader yields the same thing.
    let shipped = shipped_humanoid_constraints().expect("shipped");
    assert_eq!(shipped.len(), loaded.len());
    for (i, ((sn, sc), (ln, lc))) in shipped.iter().zip(loaded.iter()).enumerate() {
        assert_eq!(sn, ln, "shipped[{i}] name");
        assert_constraint_eq(sn, sc, lc);
    }
}

/// Spot-check the exact f32 reproduction of `<deg> * d` for the boundary cases:
/// the `0.0` literals (`0.0 * d == 0.0`) and a non-trivial value (`145.0 * d`).
#[test]
fn exact_f32_degree_conversion() {
    let d = std::f32::consts::PI / 180.0;
    let loaded = shipped_humanoid_constraints().expect("parse");

    // leftLowerArm: min.y is `0.0`, max.y is `145.0 * d`.
    let (_, lla) = loaded.iter().find(|(n, _)| n == "leftLowerArm").expect("leftLowerArm");
    assert_eq!(lla.min[1], 0.0f32, "0.0 deg → exactly 0.0 rad");
    assert_eq!(lla.max[1], 145.0f32 * d, "145.0 * d bit-for-bit");

    // head: max.x is `60.0 * d`.
    let (_, head) = loaded.iter().find(|(n, _)| n == "head").expect("head");
    assert_eq!(head.max[0], 60.0f32 * d, "60.0 * d bit-for-bit");
}

/// `constraint_from_map` rebuilds a single constraint from a limits map.
#[test]
fn single_constraint_from_map() {
    let root = kami_scene::root_map(
        "{:m {:min-deg [-30.0 -45.0 -30.0] :max-deg [30.0 45.0 30.0]}}",
    )
    .expect("map");
    let m = kami_scene::mget(&root, "m").and_then(|v| v.as_map()).expect("inner map");
    let c = constraint_from_map(m);
    let d = std::f32::consts::PI / 180.0;
    // == the `neck` row of the Rust oracle.
    assert_eq!(c.min[0], -30.0f32 * d);
    assert_eq!(c.max[1], 45.0f32 * d);
}

/// Tolerant-parse errors: non-map root → error, missing table → error.
#[test]
fn tolerant_parse_errors() {
    assert!(matches!(
        humanoid_constraints_from_edn("123"),
        Err(Error::NotAMap)
    ));
    assert!(matches!(
        humanoid_constraints_from_edn("{:x 1}"),
        Err(Error::NoTable)
    ));
    // A non-vector table value is also NoTable.
    assert!(matches!(
        humanoid_constraints_from_edn("{:skeleton/humanoid-constraints 5}"),
        Err(Error::NoTable)
    ));
}
