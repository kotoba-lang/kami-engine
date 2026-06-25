//! FirstPerson — resolve VRM first-person mesh visibility per camera view.
//!
//! `kami-vrm` parses the VRM first-person mesh annotations (each mesh node tagged
//! `Auto` / `Both` / `ThirdPersonOnly` / `FirstPersonOnly`) but applies nothing.
//! This is the runtime resolver (the `@pixiv/three-vrm` `VRMFirstPerson`
//! analogue): given the active camera perspective, it answers which mesh nodes
//! are visible — so a host can cull e.g. the head/hair in first-person view to
//! keep them out of the camera. Pure logic; the host drives the cull set.

use crate::vrm_types::{FirstPersonFlag, VrmFirstPerson};

/// Which camera perspective is rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstPersonView {
    FirstPerson,
    ThirdPerson,
}

/// Is a node with `flag` visible in `view`?
///
/// `Auto` defaults to visible in both views — the VRM 1.0 spec's headless-mesh
/// generation + first-person bone offset is a host-side refinement; the simple
/// resolver keeps `Auto` meshes on (a host that builds a headless variant can
/// override per node).
pub fn node_visible(flag: FirstPersonFlag, view: FirstPersonView) -> bool {
    use FirstPersonFlag::*;
    use FirstPersonView::*;
    match (flag, view) {
        (Both, _) | (Auto, _) => true,
        (ThirdPersonOnly, ThirdPerson) => true,
        (ThirdPersonOnly, FirstPerson) => false,
        (FirstPersonOnly, FirstPerson) => true,
        (FirstPersonOnly, ThirdPerson) => false,
    }
}

/// Resolves first-person visibility against a VRM's mesh annotations.
pub struct FirstPersonResolver<'a> {
    fp: Option<&'a VrmFirstPerson>,
}

impl<'a> FirstPersonResolver<'a> {
    pub fn new(fp: Option<&'a VrmFirstPerson>) -> Self {
        Self { fp }
    }

    /// Visibility of a mesh `node` in `view`. Nodes without an annotation (and
    /// avatars without a first-person block) default to visible.
    pub fn visible(&self, node: usize, view: FirstPersonView) -> bool {
        match self.fp {
            None => true,
            Some(fp) => fp
                .mesh_annotations
                .iter()
                .find(|a| a.node == node)
                .map(|a| node_visible(a.annotation_type, view))
                .unwrap_or(true),
        }
    }

    /// The set of node indices to cull (hidden) in `view`.
    pub fn hidden_nodes(&self, view: FirstPersonView) -> Vec<usize> {
        match self.fp {
            None => Vec::new(),
            Some(fp) => fp
                .mesh_annotations
                .iter()
                .filter(|a| !node_visible(a.annotation_type, view))
                .map(|a| a.node)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vrm_types::MeshAnnotation;
    use FirstPersonView::*;

    fn fp() -> VrmFirstPerson {
        VrmFirstPerson {
            mesh_annotations: vec![
                MeshAnnotation { node: 1, annotation_type: FirstPersonFlag::ThirdPersonOnly }, // head
                MeshAnnotation { node: 2, annotation_type: FirstPersonFlag::Both },             // body
                MeshAnnotation { node: 3, annotation_type: FirstPersonFlag::FirstPersonOnly },  // fp arms
                MeshAnnotation { node: 4, annotation_type: FirstPersonFlag::Auto },
            ],
        }
    }

    #[test]
    fn third_person_only_hidden_in_first_person() {
        assert!(!node_visible(FirstPersonFlag::ThirdPersonOnly, FirstPerson));
        assert!(node_visible(FirstPersonFlag::ThirdPersonOnly, ThirdPerson));
    }

    #[test]
    fn first_person_only_hidden_in_third_person() {
        assert!(node_visible(FirstPersonFlag::FirstPersonOnly, FirstPerson));
        assert!(!node_visible(FirstPersonFlag::FirstPersonOnly, ThirdPerson));
    }

    #[test]
    fn both_and_auto_always_visible() {
        for v in [FirstPerson, ThirdPerson] {
            assert!(node_visible(FirstPersonFlag::Both, v));
            assert!(node_visible(FirstPersonFlag::Auto, v));
        }
    }

    #[test]
    fn resolver_cull_sets_per_view() {
        let fp = fp();
        let r = FirstPersonResolver::new(Some(&fp));
        // first-person: head (3rd-only) culled; fp arms shown.
        assert_eq!(r.hidden_nodes(FirstPerson), vec![1]);
        assert!(!r.visible(1, FirstPerson));
        assert!(r.visible(3, FirstPerson));
        // third-person: fp arms culled; head shown.
        assert_eq!(r.hidden_nodes(ThirdPerson), vec![3]);
        assert!(r.visible(1, ThirdPerson));
        assert!(!r.visible(3, ThirdPerson));
    }

    #[test]
    fn unannotated_node_and_no_fp_default_visible() {
        let fp = fp();
        let r = FirstPersonResolver::new(Some(&fp));
        assert!(r.visible(99, FirstPerson), "unannotated node visible");
        let none = FirstPersonResolver::new(None);
        assert!(none.visible(1, FirstPerson));
        assert!(none.hidden_nodes(FirstPerson).is_empty());
    }
}
