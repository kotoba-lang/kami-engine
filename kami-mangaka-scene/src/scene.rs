// Scene root — wraps hecs::World + per-character VRM state.

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use hecs::World;
use kami_skeleton::Skeleton;
use kami_vrm::{
    humanoid, parse_vrm,
    spring::SpringSimulator,
    vrm_types::{HumanBoneName, VrmDocument},
};
use serde::{Deserialize, Serialize};

use crate::{
    CharacterId, PropId, Result, SceneError,
    camera::{CameraSpec, LightSpec},
    lexicon,
    pose::{Expression, PoseSpec},
    sim::FxKind,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSpec {
    pub biome: String,
    pub weather: Option<String>,
    pub seed: u64,
    pub ground_size_m: f32,
    pub layout_anchors: Vec<Anchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub name: String,
    pub xform: Transform,
}

/// Per-character runtime state held outside the ECS so SpringSimulator (which
/// borrows mutably during `step`) can coexist with hecs immutable iteration.
pub(crate) struct Character {
    pub rkey: String,
    pub root_xform: Transform,
    pub doc: VrmDocument,
    /// Retained for GPU skinning in P2 (kami-render::scene_pipelines).
    #[allow(dead_code)]
    pub skeleton: Skeleton,
    pub spring: SpringSimulator,
    /// Per-node rotation overrides from the latest `pose()` call.
    pub pose_overrides: HashMap<usize, Quat>,
    /// Spring simulator output buffer (node_idx → quat xyzw).
    pub spring_out: Vec<(usize, [f32; 4])>,
    pub current_expression: Expression,
    pub current_pose_label: Option<String>,
}

pub struct MangakaScene {
    pub(crate) world: World,
    pub(crate) characters: HashMap<CharacterId, Character>,
    pub(crate) char_order: Vec<CharacterId>,
    pub(crate) props: Vec<(PropId, hecs::Entity)>,
    pub(crate) camera: Option<CameraSpec>,
    pub(crate) lights: Vec<LightSpec>,
    pub(crate) env: Option<EnvironmentSpec>,
    next_char: u32,
    next_prop: u32,
}

impl Default for MangakaScene {
    fn default() -> Self {
        Self::new()
    }
}

impl MangakaScene {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            characters: HashMap::new(),
            char_order: Vec::new(),
            props: Vec::new(),
            camera: None,
            lights: Vec::new(),
            env: None,
            next_char: 0,
            next_prop: 0,
        }
    }

    pub fn character_ids(&self) -> &[CharacterId] {
        &self.char_order
    }

    pub fn load_character(&mut self, vrm_bytes: &[u8], rkey: &str) -> Result<CharacterId> {
        let doc = parse_vrm(vrm_bytes).map_err(|e| SceneError::VrmDecode(e.to_string()))?;
        let skeleton =
            humanoid::to_kami_skeleton(&doc).map_err(|e| SceneError::VrmDecode(e.to_string()))?;
        let spring = SpringSimulator::new(&doc);

        let id = CharacterId(self.next_char);
        self.next_char += 1;
        self.characters.insert(
            id,
            Character {
                rkey: rkey.to_owned(),
                root_xform: Transform::default(),
                doc,
                skeleton,
                spring,
                pose_overrides: HashMap::new(),
                spring_out: Vec::new(),
                current_expression: Expression::Neutral,
                current_pose_label: None,
            },
        );
        self.char_order.push(id);
        Ok(id)
    }

    pub fn pose(&mut self, c: CharacterId, pose: &PoseSpec) -> Result<()> {
        let ch = self
            .characters
            .get_mut(&c)
            .ok_or_else(|| SceneError::Render(format!("unknown character {:?}", c)))?;

        ch.root_xform = pose.root_xform;
        ch.current_pose_label = pose.label.clone();
        ch.pose_overrides.clear();

        // 1. Apply semantic preset (if any) — provides the coarse silhouette.
        if let Some(label) = pose.label.as_deref() {
            if let Some(preset) = lexicon::pose_preset(label) {
                apply_bone_rotations(ch, &preset);
            }
        }

        // 2. Explicit bone overrides take precedence over preset rotations.
        apply_bone_rotations(ch, &pose.bones);

        // IK targets are recorded but not solved here — solver lives in a
        // future P2/P3 patch with kami-skeleton blend-tree wiring. Pose
        // descriptors round-trip the targets so the LLM can re-emit them.
        Ok(())
    }

    pub fn expression(&mut self, c: CharacterId, emo: Expression) -> Result<()> {
        let ch = self
            .characters
            .get_mut(&c)
            .ok_or_else(|| SceneError::Render(format!("unknown character {:?}", c)))?;
        ch.current_expression = emo;
        Ok(())
    }

    pub fn set_background(&mut self, env: EnvironmentSpec) {
        self.env = Some(env);
    }

    pub fn add_prop(&mut self, _gltf_bytes: &[u8], xform: Transform) -> Result<PropId> {
        // glTF parsing wires in P2; for now we record the placement so the
        // renderer can lay out placeholders.
        let id = PropId(self.next_prop);
        self.next_prop += 1;
        let ent = self.world.spawn((xform,));
        self.props.push((id, ent));
        Ok(id)
    }

    pub fn set_camera(&mut self, cam: CameraSpec) {
        self.camera = Some(cam);
    }

    pub fn add_light(&mut self, light: LightSpec) {
        self.lights.push(light);
    }

    pub fn tick(&mut self, dt: f32) {
        for id in self.char_order.clone() {
            if let Some(ch) = self.characters.get_mut(&id) {
                step_character(ch, dt);
            }
        }
    }

    pub fn settle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick(1.0 / 60.0);
        }
    }

    pub fn add_wind(&mut self, _dir: Vec3, _speed: f32) {
        // P2: wire to kami-atmosphere wind_field; with feature `sim-dec`,
        // inject as an edge-field forcing term into the DEC complex.
    }

    pub fn add_particle_burst(&mut self, _kind: FxKind, _at: Vec3) {
        // P2: kami-pipelines::ParticleAdapter::emit_burst.
    }

    pub fn to_jsonld(&self) -> serde_json::Value {
        let chars: Vec<_> = self
            .char_order
            .iter()
            .filter_map(|id| self.characters.get(id).map(|ch| (id, ch)))
            .map(|(id, ch)| {
                serde_json::json!({
                    "id": id.0,
                    "rkey": ch.rkey,
                    "pose_label": ch.current_pose_label,
                    "expression": ch.current_expression,
                    "root_xform": ch.root_xform,
                })
            })
            .collect();
        let props: Vec<_> = self
            .props
            .iter()
            .map(|(id, _)| serde_json::json!({ "id": id.0 }))
            .collect();
        serde_json::json!({
            "@context": "https://kami.etzhayyim.com/mangaka-scene/v1",
            "characters": chars,
            "props": props,
            "camera": self.camera,
            "lights": self.lights,
            "environment": self.env,
        })
    }

    pub fn from_jsonld(v: &serde_json::Value) -> Result<Self> {
        let mut s = Self::new();
        if let Some(env) = v.get("environment") {
            if !env.is_null() {
                s.env = serde_json::from_value(env.clone())
                    .map_err(|e| SceneError::Jsonld(e.to_string()))?;
            }
        }
        if let Some(cam) = v.get("camera") {
            if !cam.is_null() {
                s.camera = serde_json::from_value(cam.clone())
                    .map_err(|e| SceneError::Jsonld(e.to_string()))?;
            }
        }
        if let Some(lights) = v.get("lights").and_then(|x| x.as_array()) {
            for l in lights {
                let light: LightSpec = serde_json::from_value(l.clone())
                    .map_err(|e| SceneError::Jsonld(e.to_string()))?;
                s.lights.push(light);
            }
        }
        Ok(s)
    }
}

// -----------------------------------------------------------------------------
// Internal helpers
// -----------------------------------------------------------------------------

fn apply_bone_rotations(ch: &mut Character, rotations: &[crate::pose::BoneRotation]) {
    for br in rotations {
        let Some(bone_name) = HumanBoneName::from_str(&br.bone) else {
            continue;
        };
        let Some(node_idx) = humanoid::find_bone_node(&ch.doc, bone_name) else {
            continue;
        };
        ch.pose_overrides.insert(node_idx, br.rotation);
    }
}

fn step_character(ch: &mut Character, dt: f32) {
    // Compute world matrix for every node in the VRM scene given current pose
    // overrides + the character's root xform. SpringSimulator::step then reads
    // these as it advances each spring chain.
    let world = compute_node_worlds(&ch.doc, &ch.pose_overrides, ch.root_xform);

    ch.spring_out.clear();
    ch.spring.step(
        dt,
        |node_idx| world.get(node_idx).copied(),
        &mut ch.spring_out,
    );

    // Promote spring outputs into pose_overrides so the next tick's world
    // computation includes them. One frame of latency on intra-chain cascades
    // is acceptable at 60 Hz (per kami-vrm::spring doc comment).
    for (node_idx, q) in &ch.spring_out {
        let q = Quat::from_array(*q);
        ch.pose_overrides.insert(*node_idx, q);
    }
}

/// Walk the glTF scene tree, composing world matrices node by node with
/// `pose_overrides` substituted in for `node.rotation` where present, and
/// the character's `root_xform` applied at scene roots.
fn compute_node_worlds(
    doc: &VrmDocument,
    pose_overrides: &HashMap<usize, Quat>,
    root_xform: Transform,
) -> Vec<Mat4> {
    let n = doc.gltf.nodes.len();
    let mut world = vec![Mat4::IDENTITY; n];
    let mut visited = vec![false; n];

    let root_mat = Mat4::from_scale_rotation_translation(
        root_xform.scale,
        root_xform.rotation,
        root_xform.translation,
    );

    // Roots = scene.nodes; if no scene exists fall back to nodes without parents.
    let roots: Vec<usize> = if let Some(scene_idx) = doc.gltf.scene {
        doc.gltf
            .scenes
            .get(scene_idx)
            .map(|s| s.nodes.clone())
            .unwrap_or_default()
    } else if let Some(s) = doc.gltf.scenes.first() {
        s.nodes.clone()
    } else {
        // Build parent-set heuristic.
        let mut has_parent = vec![false; n];
        for node in &doc.gltf.nodes {
            for &c in &node.children {
                if c < n {
                    has_parent[c] = true;
                }
            }
        }
        (0..n).filter(|i| !has_parent[*i]).collect()
    };

    for &r in &roots {
        if r >= n {
            continue;
        }
        walk_node(doc, pose_overrides, r, root_mat, &mut world, &mut visited);
    }
    world
}

fn walk_node(
    doc: &VrmDocument,
    pose_overrides: &HashMap<usize, Quat>,
    idx: usize,
    parent: Mat4,
    world: &mut Vec<Mat4>,
    visited: &mut Vec<bool>,
) {
    if idx >= doc.gltf.nodes.len() || visited[idx] {
        return;
    }
    visited[idx] = true;

    let node = &doc.gltf.nodes[idx];
    let t = Vec3::from(node.translation.unwrap_or([0.0; 3]));
    let rot_base = node
        .rotation
        .map(Quat::from_array)
        .unwrap_or(Quat::IDENTITY);
    let r = pose_overrides.get(&idx).copied().unwrap_or(rot_base);
    let s = Vec3::from(node.scale.unwrap_or([1.0; 3]));

    let local = Mat4::from_scale_rotation_translation(s, r, t);
    let w = parent * local;
    world[idx] = w;

    let children = node.children.clone();
    for c in children {
        walk_node(doc, pose_overrides, c, w, world, visited);
    }
}
