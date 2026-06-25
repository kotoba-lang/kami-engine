//! kami-articulated-scene — EDN authoring surface for `kami-articulated`.
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-live` for robot
//! arms: it turns canonical `:arm/*` EDN into the real
//! [`kami_articulated::ArticulatedSystem`], re-using the tolerant `kami-scene`
//! accessors the same way games parse `scene.edn` (missing keys fall back to
//! defaults, namespaced keywords match on `ns/name`, ints coerce to floats).
//!
//! ## Why this is safe (ADR-0038/0040/0042/0046)
//!
//! `kami-articulated` stays "pure Rust + glam + roxmltree, no edn dep". The
//! articulation spec is **load-time DATA** — parsed once into an
//! `ArticulatedSystem`, never touched by the kami-genesis solver — so it is safe
//! to author as EDN. clj/edn が主: the shipped `giemon_arm6.edn` is the source of
//! truth and the `giemon_arm6.urdf` remains the **parity oracle**, asserted
//! `from_edn(EDN) ≈ parse_urdf(URDF)` in [`mod tests`].
//!
//! ## EDN shape (see `fixtures/giemon_arm6/giemon_arm6.edn`)
//!
//! ```edn
//! {:arm/name "giemon_arm6" :arm/dof 6
//!  :arm/base {:link/name "base_link" :link/inertial {:origin [..] :mass .. :inertia {..}}}
//!  :arm/chain
//!  [{:joint/name "j1" :joint/type :revolute :joint/parent "base_link" :joint/child "link1"
//!    :joint/origin [..] :joint/axis [..] :joint/limit {:lower .. :upper .. :effort .. :velocity ..}
//!    :joint/damping .. :child/link {:link/name "link1" :link/inertial {..}}}
//!   ...]}
//! ```

use std::collections::BTreeMap;

use glam::Vec3;
use kami_articulated::{ArticulatedSystem, Inertia, Joint, JointKind, Link, Pose};
use kami_scene::{mget, num, root_map, vec3, EdnValue};

/// The canonical giemon_arm6 articulation shipped as EDN (source of truth).
pub const GIEMON_ARM6_EDN: &str = include_str!("../../fixtures/giemon_arm6/giemon_arm6.edn");

/// The parity-oracle URDF for giemon_arm6 (asserted equal to the EDN in tests).
pub const GIEMON_ARM6_URDF: &str = include_str!("../../fixtures/giemon_arm6/giemon_arm6.urdf");

/// Errors raised while loading an articulation from `:arm/*` EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("arm EDN root is not a map")]
    NotAMap,
    /// `:arm/base` was missing or not a map.
    #[error("`:arm/base` missing or not a map")]
    NoBase,
    /// `:arm/chain` was missing or not a vector.
    #[error("`:arm/chain` missing or not a vector")]
    NoChain,
    /// A chain entry (or its `:child/link`) was not a map.
    #[error("chain entry {0} is not a well-formed map")]
    BadChainEntry(usize),
    /// A link map was missing its `:link/name`.
    #[error("link map missing `:link/name`")]
    NoLinkName,
    /// A chain joint was missing its `:joint/name`.
    #[error("chain joint {0} missing `:joint/name`")]
    NoJointName(usize),
}

fn str_of(v: Option<&EdnValue>) -> Option<String> {
    v.and_then(|x| x.as_string()).map(|s| s.to_string())
}

/// Read a keyword *value* (`:revolute`) as its bare name (`"revolute"`).
fn kw_name(v: Option<&EdnValue>) -> Option<String> {
    v.and_then(|x| x.as_keyword()).map(|kw| kw.0.name.clone())
}

/// `[x y z]` → `Pose` (rpy is always zero in this schema, mirroring the URDF
/// fixtures which carry no rpy).
fn pose_xyz(v: Option<&EdnValue>) -> Pose {
    let [x, y, z] = vec3(v);
    Pose { xyz: Vec3::new(x, y, z), rpy: Vec3::ZERO }
}

/// Build an [`Inertia`] from a map that contains a `:link/inertial` sub-map.
fn inertial_of(link_map: &BTreeMap<EdnValue, EdnValue>) -> Inertia {
    let Some(inr) = mget(link_map, "link/inertial").and_then(|v| v.as_map()) else {
        return Inertia::default();
    };
    let inertia = mget(inr, "inertia").and_then(|v| v.as_map());
    let g = |k: &str| inertia.map(|im| num(mget(im, k))).unwrap_or(0.0);
    Inertia {
        mass: num(mget(inr, "mass")),
        ixx: g("ixx"),
        iyy: g("iyy"),
        izz: g("izz"),
        ixy: g("ixy"),
        ixz: g("ixz"),
        iyz: g("iyz"),
        com: pose_xyz(mget(inr, "origin")),
    }
}

fn link_of(link_map: &BTreeMap<EdnValue, EdnValue>) -> Result<Link, Error> {
    let name = str_of(mget(link_map, "link/name")).ok_or(Error::NoLinkName)?;
    Ok(Link { name, inertia: inertial_of(link_map) })
}

fn joint_kind(s: &str) -> JointKind {
    match s {
        "prismatic" => JointKind::Prismatic,
        "fixed" => JointKind::Fixed,
        "continuous" => JointKind::Continuous,
        _ => JointKind::Revolute,
    }
}

/// Parse canonical `:arm/*` EDN into a [`kami_articulated::ArticulatedSystem`].
///
/// Link order matches the URDF document order the parser produces: the base
/// link first, then each chain entry's `:child/link` in declaration order.
pub fn from_edn(src: &str) -> Result<ArticulatedSystem, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let name = str_of(mget(&root, "arm/name")).unwrap_or_else(|| "robot".to_string());

    let mut links: Vec<Link> = Vec::new();
    let mut joints: Vec<Joint> = Vec::new();

    // Base link.
    let base = mget(&root, "arm/base").and_then(|v| v.as_map()).ok_or(Error::NoBase)?;
    links.push(link_of(base)?);

    // Chain: vector of { :joint/* … :child/link {…} }.
    let chain = mget(&root, "arm/chain").and_then(|v| v.as_vector()).ok_or(Error::NoChain)?;
    for (i, entry) in chain.iter().enumerate() {
        let e = entry.as_map().ok_or(Error::BadChainEntry(i))?;

        let jname = str_of(mget(e, "joint/name")).ok_or(Error::NoJointName(i))?;
        let kind = joint_kind(&kw_name(mget(e, "joint/type")).unwrap_or_else(|| "revolute".to_string()));
        let [ax, ay, az] = vec3(mget(e, "joint/axis"));
        let limit = mget(e, "joint/limit").and_then(|v| v.as_map());
        let lg = |k: &str| limit.map(|lm| num(mget(lm, k))).unwrap_or(0.0);

        joints.push(Joint {
            name: jname,
            kind,
            parent: str_of(mget(e, "joint/parent")).unwrap_or_default(),
            child: str_of(mget(e, "joint/child")).unwrap_or_default(),
            origin: pose_xyz(mget(e, "joint/origin")),
            axis: Vec3::new(ax, ay, az),
            lower: lg("lower"),
            upper: lg("upper"),
            effort: lg("effort"),
            velocity: lg("velocity"),
            damping: num(mget(e, "joint/damping")),
            friction: num(mget(e, "joint/friction")),
        });

        let child = mget(e, "child/link").and_then(|v| v.as_map()).ok_or(Error::BadChainEntry(i))?;
        links.push(link_of(child)?);
    }

    Ok(ArticulatedSystem { name, links, joints })
}

/// Load the shipped giemon_arm6 articulation from its canonical EDN.
pub fn giemon_arm6() -> Result<ArticulatedSystem, Error> {
    from_edn(GIEMON_ARM6_EDN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kami_articulated::parse_urdf;

    /// Mixed absolute/relative tolerance — EDN numbers are `f64`→`f32`, URDF are
    /// `&str`→`f32`; both round to the nearest `f32` so they agree to ~1e-6.
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5 + 1e-4 * b.abs()
    }

    #[test]
    fn from_edn_parses_giemon_arm6() {
        let s = giemon_arm6().expect("giemon_arm6.edn parses");
        assert_eq!(s.name, "giemon_arm6");
        assert_eq!(s.links.len(), 7, "base_link + link1..link6");
        assert_eq!(s.joints.len(), 6, "j1..j6");
        // base link inertia carried over
        let base = &s.links[s.link_index("base_link").unwrap()];
        assert!(close(base.inertia.mass, 2.0));
    }

    /// ADR-0046 parity: the EDN source of truth must reproduce the URDF oracle.
    #[test]
    fn edn_urdf_parity() {
        let e = giemon_arm6().expect("edn");
        let u = parse_urdf(GIEMON_ARM6_URDF).expect("urdf");

        assert_eq!(e.name, u.name, "robot name");
        assert_eq!(e.links.len(), u.links.len(), "link count");
        assert_eq!(e.joints.len(), u.joints.len(), "joint count");

        for ul in &u.links {
            let el = &e.links[e
                .link_index(&ul.name)
                .unwrap_or_else(|| panic!("link `{}` missing in EDN", ul.name))];
            let (a, b) = (el.inertia, ul.inertia);
            assert!(close(a.mass, b.mass), "{} mass {} vs {}", ul.name, a.mass, b.mass);
            for (x, y) in [
                (a.ixx, b.ixx), (a.iyy, b.iyy), (a.izz, b.izz),
                (a.ixy, b.ixy), (a.ixz, b.ixz), (a.iyz, b.iyz),
            ] {
                assert!(close(x, y), "{} inertia {} vs {}", ul.name, x, y);
            }
            assert!(
                close(a.com.xyz.x, b.com.xyz.x)
                    && close(a.com.xyz.y, b.com.xyz.y)
                    && close(a.com.xyz.z, b.com.xyz.z),
                "{} com {:?} vs {:?}", ul.name, a.com.xyz, b.com.xyz
            );
        }

        for uj in &u.joints {
            let ej = &e.joints[e
                .joint_index(&uj.name)
                .unwrap_or_else(|| panic!("joint `{}` missing in EDN", uj.name))];
            assert_eq!(ej.kind, uj.kind, "{} kind", uj.name);
            assert_eq!(ej.parent, uj.parent, "{} parent", uj.name);
            assert_eq!(ej.child, uj.child, "{} child", uj.name);
            for (x, y) in [
                (ej.lower, uj.lower), (ej.upper, uj.upper), (ej.effort, uj.effort),
                (ej.velocity, uj.velocity), (ej.damping, uj.damping),
            ] {
                assert!(close(x, y), "{} limit {} vs {}", uj.name, x, y);
            }
            assert!(
                close(ej.origin.xyz.x, uj.origin.xyz.x)
                    && close(ej.origin.xyz.y, uj.origin.xyz.y)
                    && close(ej.origin.xyz.z, uj.origin.xyz.z),
                "{} origin {:?} vs {:?}", uj.name, ej.origin.xyz, uj.origin.xyz
            );
            assert!(
                close(ej.axis.x, uj.axis.x)
                    && close(ej.axis.y, uj.axis.y)
                    && close(ej.axis.z, uj.axis.z),
                "{} axis {:?} vs {:?}", uj.name, ej.axis, uj.axis
            );
        }
    }
}
