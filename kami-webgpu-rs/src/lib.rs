//! kami-webgpu-rs — the native twin of the CLJS `kami.webgpu` executor.
//!
//! It interprets the **same EDN render-IR** (globals + instances) the web renders, but
//! drives wgpu directly instead of the browser WebGPU API (ADR-0001/0040: one EDN, two
//! executors — web = CLJS→WebGPU, native = Rust→wgpu). Rendering is headless (offscreen
//! texture + pixel readback), so it verifies by golden frame in `cargo test` — no window.
//!
//! v1 is the forward lit pass (instanced cuboids, hemisphere ambient + sun + spec + rim,
//! Reinhard tonemap), matching the web instance layout (model + colour + material = 96 B).
//! The shadow pass ports next.

use glam::{Mat4, Vec3};
use kami_scene::{mget, num, root_map, vec3};
use kotoba_edn::EdnValue;

#[derive(Clone, Debug)]
pub struct Instance {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub size: [f32; 2],
    pub yaw: f32,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: f32,
}

#[derive(Clone, Debug)]
pub struct Globals {
    pub horizon: [f32; 3],
    pub sun_dir: [f32; 3],
    pub sun: [f32; 3],
    pub eye: Option<[f32; 3]>,
    pub target: Option<[f32; 3]>,
}

impl Default for Globals {
    fn default() -> Self {
        Globals {
            horizon: [0.7, 0.8, 0.9],
            sun_dir: [-0.4, -0.85, -0.35],
            sun: [1.0, 0.96, 0.85],
            eye: None,
            target: None,
        }
    }
}

fn vec2(v: Option<&EdnValue>) -> [f32; 2] {
    let s = v.and_then(|x| x.as_vector()).unwrap_or(&[]);
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(1.0);
    [g(0), g(1)]
}
fn opt_vec3(v: Option<&EdnValue>) -> Option<[f32; 3]> {
    v.and_then(|x| x.as_vector()).map(|_| vec3(v))
}
/// Local keyword/string name (namespace dropped), if `v` is one.
fn ident(v: Option<&EdnValue>) -> Option<String> {
    v.and_then(|x| {
        x.as_keyword()
            .map(|k| k.0.name.clone())
            .or_else(|| x.as_string().map(|s| s.to_string()))
    })
}
/// Read a number with an explicit default (vs. `num`'s implicit 0.0).
fn num_or(v: Option<&EdnValue>, default: f32) -> f32 {
    v.map(|x| num(Some(x))).unwrap_or(default)
}

// ── EDN render-IR extensions (ADR-0044) ─────────────────────────────────────
// Additive, optional render-IR vocabulary closing three.js/VRM gaps. `parse_ir`
// (the v1 forward-pass path + golden tests) is untouched; `parse_render_ir`
// reads the richer scene. The GPU executor adopts these incrementally.

/// Light kind for `:lights` (closes the "directional-only" gap → multi-light).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    Directional,
    Point,
    Spot,
}

impl LightKind {
    pub fn by_name(name: &str) -> LightKind {
        match name {
            "point" => LightKind::Point,
            "spot" => LightKind::Spot,
            _ => LightKind::Directional,
        }
    }
}

/// One light source. `dir` is used by directional/spot; `pos`/`range` by
/// point/spot; `spot_inner`/`spot_outer` (radians) shape the spot cone.
#[derive(Clone, Debug)]
pub struct Light {
    pub kind: LightKind,
    pub color: [f32; 3],
    pub intensity: f32,
    pub dir: [f32; 3],
    pub pos: [f32; 3],
    pub range: f32,
    pub spot_inner: f32,
    pub spot_outer: f32,
    pub cast_shadow: bool,
}

/// Explicit camera (closes "no fov/near/far" — eye/target alone can't frame).
#[derive(Clone, Debug)]
pub struct Camera {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    /// Vertical FOV in radians.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

/// Environment / image-based lighting (closes "no IBL/env map"). `ibl_url` is a
/// host-loaded equirect/cubemap reference; `ibl_intensity` scales it.
#[derive(Clone, Debug)]
pub struct Environment {
    pub ambient: [f32; 3],
    pub ground: [f32; 3],
    pub ibl_intensity: f32,
    pub ibl_url: Option<String>,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            ambient: [0.7, 0.8, 0.9],
            ground: [0.34, 0.52, 0.30],
            ibl_intensity: 0.0,
            ibl_url: None,
        }
    }
}

/// Shading model for a `:materials` entry (closes "fixed PBR/MToon, not data").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialModel {
    Pbr,
    Mtoon,
    Unlit,
}

impl MaterialModel {
    pub fn by_name(name: &str) -> MaterialModel {
        match name {
            "mtoon" => MaterialModel::Mtoon,
            "unlit" => MaterialModel::Unlit,
            _ => MaterialModel::Pbr,
        }
    }
}

/// Transparency handling (closes the "no alpha-test / glTF MASK" gap).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    /// Cutout: discard fragments below `alpha_cutoff` (hair / foliage / VRM).
    Mask,
    Blend,
}

impl AlphaMode {
    pub fn by_name(name: &str) -> AlphaMode {
        match name {
            "mask" => AlphaMode::Mask,
            "blend" => AlphaMode::Blend,
            _ => AlphaMode::Opaque,
        }
    }
}

/// A named material a mesh/instance references by `id`. Covers PBR metallic-
/// roughness, MToon toon params (shade/outline/rim/matcap), and alpha handling.
#[derive(Clone, Debug)]
pub struct Material {
    pub id: String,
    pub model: MaterialModel,
    pub base: [f32; 3],
    /// MToon shade (second) colour.
    pub shade: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    /// MToon outline width (world units); 0 = no outline.
    pub outline: f32,
    /// MToon rim-light intensity.
    pub rim: f32,
    /// MToon matcap texture reference (host-loaded).
    pub matcap: Option<String>,
}

/// One morph-target weight (VRM expression / glTF morph). `name` is the target
/// (e.g. `happy`, `blink`, `aa`); `weight` in [0,1].
#[derive(Clone, Debug)]
pub struct MorphWeight {
    pub name: String,
    pub weight: f32,
}

/// A skinned / morphable mesh asset (closes "skinned + morph in IR → cuboids
/// only"). The asset (`url`) is host-loaded via kami-vrm / kami-gltf; the IR
/// declares the *binding* — transform, material, skin (joint palette source),
/// per-frame morph weights, and optionally an inline joint palette so a fully
/// data-driven host can draw a VRM avatar with no per-scene code (ADR-0043).
#[derive(Clone, Debug)]
pub struct Mesh {
    pub id: String,
    pub url: String,
    pub pos: [f32; 3],
    /// Rotation quaternion xyzw.
    pub rot: [f32; 4],
    pub scale: f32,
    pub material: Option<String>,
    /// Skin/skeleton id whose joint palette deforms this mesh.
    pub skin: Option<String>,
    /// Optional inline joint palette (column-major mat4 per joint). When empty,
    /// the host supplies the palette from its skeleton evaluation.
    pub joints: Vec<[[f32; 4]; 4]>,
    pub morphs: Vec<MorphWeight>,
    pub cast_shadow: bool,
}

/// The richer render-IR: v1 globals+instances plus the additive vocabulary.
#[derive(Clone, Debug)]
pub struct RenderIr {
    pub globals: Globals,
    pub instances: Vec<Instance>,
    pub lights: Vec<Light>,
    pub camera: Option<Camera>,
    pub env: Environment,
    pub materials: Vec<Material>,
    pub meshes: Vec<Mesh>,
}

/// 16 flat floats → a column-major mat4; identity for missing components.
fn mat4_from_flat(v: &[EdnValue]) -> [[f32; 4]; 4] {
    let g = |i: usize| {
        v.get(i)
            .map(|x| num(Some(x)))
            .unwrap_or(if i % 5 == 0 { 1.0 } else { 0.0 })
    };
    [
        [g(0), g(1), g(2), g(3)],
        [g(4), g(5), g(6), g(7)],
        [g(8), g(9), g(10), g(11)],
        [g(12), g(13), g(14), g(15)],
    ]
}

fn parse_mesh(m: &std::collections::BTreeMap<EdnValue, EdnValue>) -> Mesh {
    let rot = {
        let s = mget(m, "rot").and_then(|x| x.as_vector()).unwrap_or(&[]);
        let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(if i == 3 { 1.0 } else { 0.0 });
        [g(0), g(1), g(2), g(3)]
    };
    let joints = mget(m, "joints")
        .and_then(|x| x.as_vector())
        .map(|js| {
            js.iter()
                .filter_map(|j| j.as_vector())
                .map(mat4_from_flat)
                .collect()
        })
        .unwrap_or_default();
    let morphs = mget(m, "morphs")
        .and_then(|x| x.as_map())
        .map(|mm| {
            mm.iter()
                .filter_map(|(k, v)| {
                    ident(Some(k)).map(|name| MorphWeight {
                        name,
                        weight: num(Some(v)),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Mesh {
        id: ident(mget(m, "id")).unwrap_or_default(),
        url: mget(m, "url").and_then(|v| v.as_string()).unwrap_or("").to_string(),
        pos: opt_vec3(mget(m, "pos")).unwrap_or([0.0, 0.0, 0.0]),
        rot,
        scale: num_or(mget(m, "scale"), 1.0),
        material: ident(mget(m, "material")),
        skin: ident(mget(m, "skin")),
        joints,
        morphs,
        cast_shadow: mget(m, "cast-shadow").and_then(|v| v.as_bool()).unwrap_or(true),
    }
}

fn parse_material(m: &std::collections::BTreeMap<EdnValue, EdnValue>) -> Material {
    Material {
        id: ident(mget(m, "id")).unwrap_or_default(),
        model: ident(mget(m, "model"))
            .map(|n| MaterialModel::by_name(&n))
            .unwrap_or(MaterialModel::Pbr),
        base: opt_vec3(mget(m, "base")).unwrap_or([1.0, 1.0, 1.0]),
        shade: opt_vec3(mget(m, "shade")).unwrap_or([0.5, 0.5, 0.5]),
        metallic: num_or(mget(m, "metallic"), 0.0),
        roughness: num_or(mget(m, "roughness"), 0.65),
        emissive: num_or(mget(m, "emissive"), 0.0),
        alpha_mode: ident(mget(m, "alpha-mode"))
            .map(|n| AlphaMode::by_name(&n))
            .unwrap_or(AlphaMode::Opaque),
        alpha_cutoff: num_or(mget(m, "alpha-cutoff"), 0.5),
        outline: num_or(mget(m, "outline"), 0.0),
        rim: num_or(mget(m, "rim"), 0.0),
        matcap: mget(m, "matcap").and_then(|v| v.as_string()).map(|s| s.to_string()),
    }
}

fn parse_light(m: &std::collections::BTreeMap<EdnValue, EdnValue>) -> Light {
    let kind = ident(mget(m, "kind"))
        .map(|n| LightKind::by_name(&n))
        .unwrap_or(LightKind::Directional);
    Light {
        kind,
        color: opt_vec3(mget(m, "color")).unwrap_or([1.0, 1.0, 1.0]),
        intensity: num_or(mget(m, "intensity"), 1.0),
        dir: opt_vec3(mget(m, "dir")).unwrap_or([-0.4, -0.85, -0.35]),
        pos: opt_vec3(mget(m, "pos")).unwrap_or([0.0, 0.0, 0.0]),
        range: num_or(mget(m, "range"), 0.0),
        spot_inner: num_or(mget(m, "inner"), 0.0),
        spot_outer: num_or(mget(m, "outer"), 0.0),
        cast_shadow: mget(m, "cast-shadow").and_then(|v| v.as_bool()).unwrap_or(false),
    }
}

/// Parse the richer EDN render-IR. Backward compatible: a v1 scene (just
/// `:globals` + `:instances`) parses with empty `lights`, no `camera`, default
/// `env`. New keys: `:lights [...]`, `:camera {...}`, `:env {...}`.
pub fn parse_render_ir(edn: &str) -> RenderIr {
    let (globals, instances) = parse_ir(edn);
    let mut lights = Vec::new();
    let mut camera = None;
    let mut env = Environment::default();
    let mut materials = Vec::new();
    let mut meshes = Vec::new();
    env.ambient = globals.horizon;

    if let Some(root) = root_map(edn) {
        if let Some(ls) = mget(&root, "lights").and_then(|x| x.as_vector()) {
            lights = ls.iter().filter_map(|l| l.as_map()).map(parse_light).collect();
        }
        if let Some(ms) = mget(&root, "materials").and_then(|x| x.as_vector()) {
            materials = ms.iter().filter_map(|m| m.as_map()).map(parse_material).collect();
        }
        if let Some(ms) = mget(&root, "meshes").and_then(|x| x.as_vector()) {
            meshes = ms.iter().filter_map(|m| m.as_map()).map(parse_mesh).collect();
        }
        if let Some(cam) = mget(&root, "camera").and_then(|x| x.as_map().cloned()) {
            camera = Some(Camera {
                eye: opt_vec3(mget(&cam, "eye")).or(globals.eye).unwrap_or([5.0, 3.0, 8.0]),
                target: opt_vec3(mget(&cam, "target")).or(globals.target).unwrap_or([0.0, 1.0, 0.0]),
                fov_y: num_or(mget(&cam, "fov"), 0.9),
                near: num_or(mget(&cam, "near"), 0.1),
                far: num_or(mget(&cam, "far"), 1000.0),
            });
        }
        if let Some(e) = mget(&root, "env").and_then(|x| x.as_map().cloned()) {
            if mget(&e, "ambient").is_some() {
                env.ambient = vec3(mget(&e, "ambient"));
            }
            if mget(&e, "ground").is_some() {
                env.ground = vec3(mget(&e, "ground"));
            }
            if let Some(ibl) = mget(&e, "ibl").and_then(|x| x.as_map().cloned()) {
                env.ibl_intensity = num_or(mget(&ibl, "intensity"), 1.0);
                env.ibl_url = mget(&ibl, "url").and_then(|v| v.as_string()).map(|s| s.to_string());
            }
        }
    }
    RenderIr { globals, instances, lights, camera, env, materials, meshes }
}

impl RenderIr {
    /// Look up a material by `id`.
    pub fn material(&self, id: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }
    /// Look up a mesh by `id`.
    pub fn mesh(&self, id: &str) -> Option<&Mesh> {
        self.meshes.iter().find(|m| m.id == id)
    }
}

impl Mesh {
    /// Resolve a morph weight by target name (0.0 when absent).
    pub fn morph(&self, name: &str) -> f32 {
        self.morphs.iter().find(|w| w.name == name).map(|w| w.weight).unwrap_or(0.0)
    }
}

#[cfg(test)]
mod render_ir_ext_tests {
    use super::*;

    #[test]
    fn v1_scene_stays_backward_compatible() {
        // A pre-ADR-0044 scene parses unchanged: no lights, no camera, default env.
        let ir = parse_render_ir(
            "{:globals {:sky {:horizon [0.7 0.8 0.9]}} :instances [{:pos [0 1 0] :color [1 0 0]}]}",
        );
        assert_eq!(ir.instances.len(), 1);
        assert!(ir.lights.is_empty());
        assert!(ir.camera.is_none());
        assert_eq!(ir.env.ambient, [0.7, 0.8, 0.9], "env ambient inherits sky horizon");
        assert_eq!(ir.env.ibl_intensity, 0.0);
    }

    #[test]
    fn parses_multi_light_rig() {
        let ir = parse_render_ir(
            r#"{:instances []
                :lights [{:kind :directional :color [1 0.96 0.85] :intensity 1.2 :dir [-0.4 -0.85 -0.35] :cast-shadow true}
                         {:kind :point :color [1 0.5 0.2] :intensity 3.0 :pos [2 3 0] :range 12.0}
                         {:kind :spot :color [0.6 0.8 1] :pos [0 5 0] :dir [0 -1 0] :range 20.0 :inner 0.3 :outer 0.6}]}"#,
        );
        assert_eq!(ir.lights.len(), 3);
        assert_eq!(ir.lights[0].kind, LightKind::Directional);
        assert!(ir.lights[0].cast_shadow);
        assert_eq!(ir.lights[1].kind, LightKind::Point);
        assert_eq!(ir.lights[1].pos, [2.0, 3.0, 0.0]);
        assert_eq!(ir.lights[1].range, 12.0);
        assert_eq!(ir.lights[2].kind, LightKind::Spot);
        assert!((ir.lights[2].spot_outer - 0.6).abs() < 1e-6);
    }

    #[test]
    fn parses_camera_and_ibl_environment() {
        let ir = parse_render_ir(
            r#"{:instances []
                :camera {:eye [0 2 6] :target [0 1 0] :fov 1.05 :near 0.1 :far 500.0}
                :env {:ambient [0.2 0.2 0.25] :ground [0.1 0.1 0.1]
                      :ibl {:intensity 0.8 :url "studio.hdr"}}}"#,
        );
        let cam = ir.camera.expect("camera");
        assert_eq!(cam.eye, [0.0, 2.0, 6.0]);
        assert!((cam.fov_y - 1.05).abs() < 1e-6);
        assert!((cam.far - 500.0).abs() < 1e-6);
        assert_eq!(ir.env.ambient, [0.2, 0.2, 0.25]);
        assert!((ir.env.ibl_intensity - 0.8).abs() < 1e-6);
        assert_eq!(ir.env.ibl_url.as_deref(), Some("studio.hdr"));
    }

    #[test]
    fn unknown_light_kind_defaults_to_directional() {
        let ir = parse_render_ir("{:instances [] :lights [{:kind :laser-disco :color [1 1 1]}]}");
        assert_eq!(ir.lights[0].kind, LightKind::Directional);
    }

    #[test]
    fn parses_material_table_with_mtoon_and_alpha() {
        let ir = parse_render_ir(
            r#"{:instances []
                :materials [{:id :skin :model :mtoon :base [1 0.8 0.7] :shade [0.6 0.4 0.4]
                             :alpha-mode :mask :alpha-cutoff 0.5 :outline 0.02 :rim 0.3 :matcap "m.png"}
                            {:id :glass :model :pbr :base [0.8 0.9 1] :metallic 0.0 :roughness 0.05
                             :alpha-mode :blend}]}"#,
        );
        assert_eq!(ir.materials.len(), 2);
        let skin = ir.material("skin").expect("skin material");
        assert_eq!(skin.model, MaterialModel::Mtoon);
        assert_eq!(skin.alpha_mode, AlphaMode::Mask);
        assert!((skin.alpha_cutoff - 0.5).abs() < 1e-6);
        assert!((skin.outline - 0.02).abs() < 1e-6);
        assert_eq!(skin.matcap.as_deref(), Some("m.png"));
        let glass = ir.material("glass").expect("glass material");
        assert_eq!(glass.model, MaterialModel::Pbr);
        assert_eq!(glass.alpha_mode, AlphaMode::Blend);
        assert_eq!(glass.alpha_cutoff, 0.5, "default cutoff when unspecified");
    }

    #[test]
    fn material_defaults_and_unknown_lookup() {
        let ir = parse_render_ir("{:instances [] :materials [{:id :plain}]}");
        let p = ir.material("plain").unwrap();
        assert_eq!(p.model, MaterialModel::Pbr, "default model");
        assert_eq!(p.alpha_mode, AlphaMode::Opaque, "default alpha");
        assert_eq!(p.base, [1.0, 1.0, 1.0]);
        assert!(ir.material("nope").is_none());
    }

    #[test]
    fn v1_scene_has_empty_material_table() {
        let ir = parse_render_ir("{:instances [{:pos [0 0 0] :color [1 0 0]}]}");
        assert!(ir.materials.is_empty(), "no :materials → empty table, backward compatible");
        assert!(ir.meshes.is_empty(), "no :meshes → empty, backward compatible");
    }

    #[test]
    fn parses_skinned_morph_vrm_mesh() {
        // A VRM avatar declared purely as data: transform + material + skin +
        // morph weights — the gating piece for the dance scene (ADR-0043).
        let ir = parse_render_ir(
            r#"{:instances []
                :materials [{:id :skin :model :mtoon}]
                :meshes [{:id :avatar :url "mitama.vrm" :pos [0 1 0] :rot [0 0 0 1] :scale 1.1
                          :material :skin :skin :rig
                          :morphs {:happy 0.8 :blink 1.0}
                          :cast-shadow true}]}"#,
        );
        assert_eq!(ir.meshes.len(), 1);
        let a = ir.mesh("avatar").expect("avatar mesh");
        assert_eq!(a.url, "mitama.vrm");
        assert_eq!(a.pos, [0.0, 1.0, 0.0]);
        assert_eq!(a.rot, [0.0, 0.0, 0.0, 1.0]);
        assert!((a.scale - 1.1).abs() < 1e-6);
        assert_eq!(a.material.as_deref(), Some("skin"));
        assert_eq!(a.skin.as_deref(), Some("rig"));
        assert!((a.morph("happy") - 0.8).abs() < 1e-6);
        assert!((a.morph("blink") - 1.0).abs() < 1e-6);
        assert_eq!(a.morph("angry"), 0.0, "absent morph → 0");
        // the mesh resolves its material in the table.
        assert_eq!(ir.material(a.material.as_deref().unwrap()).unwrap().model, MaterialModel::Mtoon);
    }

    #[test]
    fn parses_inline_joint_palette() {
        // a host can ship the evaluated skeleton palette inline (column-major mat4s).
        let ir = parse_render_ir(
            r#"{:instances []
                :meshes [{:id :rigged :url "m.vrm"
                          :joints [[1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1]
                                   [1 0 0 0  0 1 0 0  0 0 1 0  2 3 4 1]]}]}"#,
        );
        let m = ir.mesh("rigged").unwrap();
        assert_eq!(m.joints.len(), 2);
        assert_eq!(m.joints[0], [[1.0,0.0,0.0,0.0],[0.0,1.0,0.0,0.0],[0.0,0.0,1.0,0.0],[0.0,0.0,0.0,1.0]]);
        assert_eq!(m.joints[1][3], [2.0, 3.0, 4.0, 1.0], "translation row");
    }

    #[test]
    fn mesh_rot_and_scale_defaults() {
        let ir = parse_render_ir(r#"{:instances [] :meshes [{:id :m :url "x.glb"}]}"#);
        let m = ir.mesh("m").unwrap();
        assert_eq!(m.rot, [0.0, 0.0, 0.0, 1.0], "identity quaternion default");
        assert_eq!(m.scale, 1.0);
        assert!(m.cast_shadow, "meshes cast shadow by default");
    }
}

/// Parse the EDN render-IR — the same data the CLJS executor consumes.
pub fn parse_ir(edn: &str) -> (Globals, Vec<Instance>) {
    let root = match root_map(edn) {
        Some(m) => m,
        None => return (Globals::default(), vec![]),
    };
    let g = mget(&root, "globals").and_then(|x| x.as_map().cloned());
    let mut globals = Globals::default();
    if let Some(g) = &g {
        if let Some(sky) = mget(g, "sky").and_then(|x| x.as_map().cloned()) {
            globals.horizon = vec3(mget(&sky, "horizon"));
            globals.sun_dir = vec3(mget(&sky, "sun-dir"));
            globals.sun = vec3(mget(&sky, "sun"));
        }
        globals.eye = opt_vec3(mget(g, "eye"));
        globals.target = opt_vec3(mget(g, "target"));
    }
    let insts = mget(&root, "instances")
        .and_then(|x| x.as_vector())
        .unwrap_or(&[])
        .iter()
        .filter_map(|iv| iv.as_map().cloned())
        .map(|m| Instance {
            pos: vec3(mget(&m, "pos")),
            color: vec3(mget(&m, "color")),
            size: vec2(mget(&m, "size")),
            yaw: num(mget(&m, "yaw")),
            metallic: num(mget(&m, "metallic")),
            roughness: mget(&m, "roughness").map(|v| num(Some(v))).unwrap_or(0.65),
            emissive: num(mget(&m, "emissive")),
        })
        .collect();
    (globals, insts)
}

/// Bridge: a kami-clj `scene.edn` → render-IR (globals + scattered prop instances),
/// mirroring the web's deterministic scatter. This is what play3d feeds the data-driven
/// Renderer (ADR-0041 step 2). Live entities (player/bots) are appended by the caller.
pub fn scene_to_ir(scene_src: &str) -> (Globals, Vec<Instance>) {
    let root = match root_map(scene_src) { Some(m) => m, None => return (Globals::default(), vec![]) };
    let mut g = Globals::default();
    let mut ground_color = [0.34, 0.52, 0.30];
    if let Some(sky) = mget(&root, "render/sky").and_then(|x| x.as_map().cloned()) {
        g.horizon = vec3(mget(&sky, "horizon"));
        g.sun_dir = vec3(mget(&sky, "sun-dir"));
        g.sun = vec3(mget(&sky, "sun"));
        if mget(&sky, "ground").is_some() { ground_color = vec3(mget(&sky, "ground")); }
    }
    // camera rig (optional) → eye/target at origin
    if let Some(cam) = mget(&root, "camera").and_then(|x| x.as_map().cloned()) {
        let dist = num(mget(&cam, "distance")); let h = num(mget(&cam, "height"));
        let az = num(mget(&cam, "azimuth")); let lh = num(mget(&cam, "look-height"));
        g.eye = Some([dist * az.cos(), h, dist * az.sin()]);
        g.target = Some([0.0, lh, 0.0]);
    }
    let mut insts = vec![Instance { pos: [0.0, -0.5, 0.0], color: ground_color, size: [400.0, 1.0], yaw: 0.0, metallic: 0.0, roughness: 0.95, emissive: 0.0 }];

    if let Some(props) = mget(&root, "render/props").and_then(|x| x.as_map().cloned()) {
        let count = num(mget(&props, "count")) as i32;
        let spread = { let s = num(mget(&props, "spread")); if s == 0.0 { 140.0 } else { s } };
        let buildings: Vec<_> = mget(&props, "buildings").and_then(|x| x.as_vector())
            .map(|v| v.iter().filter_map(|b| b.as_map().cloned()).collect()).unwrap_or_default();
        let trees = mget(&props, "trees").and_then(|x| x.as_map().cloned());
        let tratio = trees.as_ref().map(|t| num(mget(t, "ratio"))).unwrap_or(0.0);
        let mut seed: u32 = 2654435769;
        let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; (seed & 0x7fffffff) as f32 / 2147483647.0 };
        let mut i = 0;
        while i < count {
            i += 1;
            let x = (rnd() * 2.0 - 1.0) * spread;
            let z = (rnd() * 2.0 - 1.0) * spread;
            if (x * x + z * z).sqrt() < 11.0 { continue; }
            if rnd() < tratio {
                if let Some(t) = &trees {
                    let tw = num(mget(t, "w")); let th = num(mget(t, "h"));
                    let (tm, tr) = (num(mget(t, "metallic")), { let r = num(mget(t, "roughness")); if r == 0.0 { 0.95 } else { r } });
                    insts.push(Instance { pos: [x, 0.0, z], color: [0.45, 0.32, 0.2], size: [tw * 0.3, th * 0.5], yaw: 0.0, metallic: 0.0, roughness: 0.9, emissive: 0.0 });
                    insts.push(Instance { pos: [x, th * 0.5, z], color: vec3(mget(t, "color")), size: [tw, th * 0.6], yaw: 0.0, metallic: tm, roughness: tr, emissive: 0.0 });
                }
            } else if !buildings.is_empty() {
                let b = &buildings[(rnd() * buildings.len() as f32) as usize % buildings.len()];
                let mn = num(mget(b, "min-h")); let mx = num(mget(b, "max-h"));
                let h = mn + rnd() * (mx - mn);
                let rgh = { let r = num(mget(b, "roughness")); if r == 0.0 { 0.7 } else { r } };
                insts.push(Instance { pos: [x, 0.0, z], color: vec3(mget(b, "color")), size: [num(mget(b, "w")), h], yaw: 0.0, metallic: num(mget(b, "metallic")), roughness: rgh, emissive: 0.0 });
            }
        }
    }
    (g, insts)
}

// --- cube (pos+normal), 24 verts / 36 indices — same mesh as the web ---------

fn cube() -> (Vec<f32>, Vec<u16>) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
        ([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]]),
        ([1.0, 0.0, 0.0], [[0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]]),
        ([-1.0, 0.0, 0.0], [[-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, 1.0, 0.0], [[-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, -1.0, 0.0], [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]]),
    ];
    let mut v = Vec::new();
    let mut idx = Vec::new();
    for (n, quad) in faces.iter() {
        let base = (v.len() / 6) as u16;
        for p in quad.iter() {
            v.extend_from_slice(p);
            v.extend_from_slice(n);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (v, idx)
}

fn model_mat(i: &Instance) -> Mat4 {
    let [w, h] = i.size;
    Mat4::from_translation(Vec3::new(i.pos[0], i.pos[1] + h * 0.5, i.pos[2]))
        * Mat4::from_rotation_y(i.yaw)
        * Mat4::from_scale(Vec3::new(w, h, w))
}

// Main + shadow shaders are GENERATED from kami.shaders/lit-shader (the EDN AST via kami.wgsl) and
// committed here by `bb gen-wgsl`; the web (kami.webgpu) renders the same source. `bb wgsl-parity`
// gates that these files stay token-equivalent to kami.shaders, so web↔native can't silently drift.
const SHADER: &str = include_str!("lit_shader.wgsl");

// Depth-only shadow pass — renders instances from the sun's POV into the shadow map.
const SHADOW_WGSL: &str = include_str!("shadow_shader.wgsl");

const MAX_INST: u32 = 16384;

/// A royale-style demo scene (procedural scatter mirroring the web) — shared by the
/// PNG and live-window examples so both render the same world.
pub fn demo_city() -> (Globals, Vec<Instance>) {
    let mut insts: Vec<Instance> = Vec::new();
    insts.push(Instance { pos: [0.0, -0.5, 0.0], color: [0.34, 0.52, 0.30], size: [400.0, 1.0], yaw: 0.0, metallic: 0.0, roughness: 0.95, emissive: 0.0 });
    let mut seed: u32 = 2654435769;
    let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; (seed & 0x7fffffff) as f32 / 2147483647.0 };
    let spread = 90.0;
    for _ in 0..170 {
        let x = (rnd() * 2.0 - 1.0) * spread;
        let z = (rnd() * 2.0 - 1.0) * spread;
        if (x * x + z * z).sqrt() < 8.0 { continue; }
        if rnd() < 0.4 {
            insts.push(Instance { pos: [x, 0.0, z], color: [0.45, 0.32, 0.2], size: [0.33, 1.3], yaw: 0.0, metallic: 0.0, roughness: 0.95, emissive: 0.0 });
            insts.push(Instance { pos: [x, 1.3, z], color: [0.28, 0.55, 0.30], size: [1.1, 1.6], yaw: 0.0, metallic: 0.0, roughness: 0.95, emissive: 0.0 });
        } else {
            let h = 2.0 + rnd() * 5.0;
            let (color, metallic, roughness) = if rnd() < 0.5 { ([0.62, 0.60, 0.66], 0.8, 0.25) } else { ([0.70, 0.66, 0.55], 0.05, 0.85) };
            insts.push(Instance { pos: [x, 0.0, z], color, size: [2.0, h], yaw: 0.0, metallic, roughness, emissive: 0.0 });
        }
    }
    insts.push(Instance { pos: [0.0, 0.0, 0.0], color: [0.30, 0.62, 1.0], size: [0.9, 1.9], yaw: 0.0, metallic: 0.2, roughness: 0.35, emissive: 0.5 });
    let g = Globals { horizon: [0.74, 0.84, 0.95], sun_dir: [-0.4, -0.85, -0.35], sun: [1.0, 0.96, 0.85], eye: Some([45.0, 40.0, 45.0]), target: Some([0.0, 0.0, 0.0]) };
    (g, insts)
}

fn align256(n: u32) -> u32 {
    (n + 255) & !255
}

/// Render the EDN render-IR headless and return RGBA8 pixels (w*h*4), top row first.
/// This is the native execution of the same data the web renders.
pub fn render_to_pixels(ir_edn: &str, w: u32, h: u32) -> Vec<u8> {
    let (g, insts) = parse_ir(ir_edn);
    pollster::block_on(render_async(&g, &insts, w, h))
}

/// Render from already-parsed globals + instances (for callers that build the scene in
/// Rust rather than from EDN text). Returns RGBA8 pixels (w*h*4).
pub fn render(g: &Globals, insts: &[Instance], w: u32, h: u32) -> Vec<u8> {
    pollster::block_on(render_async(g, insts, w, h))
}

/// A reusable executor: owns the GPU device + all render resources, draws the EDN scene
/// into any color view (an offscreen texture for golden frames, or a window surface for a
/// live native player). This is what kami-clj-play3d adopts for a data-driven renderer.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    depth_view: wgpu::TextureView,
    shadow_view: wgpu::TextureView,
    shadow_pipe: wgpu::RenderPipeline,
    shadow_bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    gbuf: wgpu::Buffer,
    inst: wgpu::Buffer,
    idx_count: u32,
    w: u32,
    h: u32,
}

impl Renderer {
    pub fn device(&self) -> &wgpu::Device { &self.device }
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }

    /// Resize the (screen) depth target to match a new surface size.
    pub fn resize(&mut self, w: u32, h: u32) {
        self.w = w; self.h = h;
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        });
        self.depth_view = depth.create_view(&Default::default());
    }

    /// Build the executor for a target of `color_format` at `w`×`h`.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, color_format: wgpu::TextureFormat, w: u32, h: u32) -> Self {
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());

        let (verts, idx) = cube();
        let vbuf = make_buf(&device, &queue, bytemuck::cast_slice(&verts), wgpu::BufferUsages::VERTEX);
        let ibuf = make_buf(&device, &queue, bytemuck::cast_slice(&idx), wgpu::BufferUsages::INDEX);
        let inst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: (MAX_INST * 96) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let gbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: 240, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADOW_WGSL.into()) });
        let va = |fmt, off, loc| wgpu::VertexAttribute { format: fmt, offset: off, shader_location: loc };
        let cube_attrs = [va(wgpu::VertexFormat::Float32x3, 0, 0), va(wgpu::VertexFormat::Float32x3, 12, 1)];
        let inst_attrs = [
            va(wgpu::VertexFormat::Float32x4, 0, 2), va(wgpu::VertexFormat::Float32x4, 16, 3),
            va(wgpu::VertexFormat::Float32x4, 32, 4), va(wgpu::VertexFormat::Float32x4, 48, 5),
            va(wgpu::VertexFormat::Float32x4, 64, 6), va(wgpu::VertexFormat::Float32x4, 80, 7),
        ];
        let vlayout = [
            wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &cube_attrs },
            wgpu::VertexBufferLayout { array_stride: 96, step_mode: wgpu::VertexStepMode::Instance, attributes: &inst_attrs },
        ];
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: 2048, height: 2048, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None, layout: None,
            vertex: wgpu::VertexState { module: &shadow_module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
            fragment: None,
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None, layout: None,
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vlayout },
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: color_format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth24Plus, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual, stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &shadow_pipe.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: gbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&shadow_samp) },
            ],
        });
        Renderer { device, queue, depth_view, shadow_view, shadow_pipe, shadow_bind, pipeline, bind, vbuf, ibuf, gbuf, inst, idx_count: idx.len() as u32, w, h }
    }

    /// Upload the frame's uniforms + instances and record the shadow + main passes into
    /// `color_view`, then submit. The same two :passes the web runs.
    pub fn draw(&self, color_view: &wgpu::TextureView, g: &Globals, insts: &[Instance]) {
        let (w, h) = (self.w, self.h);
        let centroid = insts.iter().fold([0.0f32, 0.0], |a, i| [a[0] + i.pos[0], a[1] + i.pos[2]]);
        let n = insts.len().max(1) as f32;
        let (cx, cz) = (centroid[0] / n, centroid[1] / n);
        let eye = g.eye.unwrap_or([cx + 60.0, 80.0, cz + 60.0]);
        let target = g.target.unwrap_or([cx, 0.0, cz]);
        let vp = Mat4::perspective_rh(60f32.to_radians(), w as f32 / h.max(1) as f32, 0.5, 4000.0)
            * Mat4::look_at_rh(Vec3::from(eye), Vec3::from(target), Vec3::Y);
        let sd = Vec3::from(g.sun_dir).normalize_or_zero();
        let ltgt = Vec3::new(cx, 0.0, cz);
        let leye = ltgt - sd * 200.0;
        let light_vp = Mat4::orthographic_rh(-130.0, 130.0, -130.0, 130.0, 1.0, 420.0) * Mat4::look_at_rh(leye, ltgt, Vec3::Y);

        let mut gf = [0f32; 60];
        gf[0..16].copy_from_slice(&vp.to_cols_array());
        gf[16..20].copy_from_slice(&[g.sun_dir[0], g.sun_dir[1], g.sun_dir[2], eye[0]]);
        gf[20..24].copy_from_slice(&[g.sun[0], g.sun[1], g.sun[2], eye[1]]);
        gf[24..28].copy_from_slice(&[g.horizon[0], g.horizon[1], g.horizon[2], eye[2]]);
        gf[28..44].copy_from_slice(&light_vp.to_cols_array());
        // tunable lighting — recovered from the previously-hardcoded fragment, now fed as data so the
        // generated shader (g.light_a..d) renders identically. Matches the web kami.webgpu defaults.
        gf[44..48].copy_from_slice(&[0.20, 0.22, 0.26, 0.65]);          // light_a: ambient rgb, sky-mix weight
        gf[48..52].copy_from_slice(&[0.25, 0.9, 0.25, 3.0]);           // light_b: specStr lo/hi, rim scale/pow
        gf[52..56].copy_from_slice(&[4.0, 256.0, 0.9, 0.7]);          // light_c: shininess lo/hi, sun scale, metal factor
        gf[56..60].copy_from_slice(&[2.2, 0.0025, 0.0006, 1.0 / 2048.0]); // light_d: gamma, shadow bias factor/min, texel
        self.queue.write_buffer(&self.gbuf, 0, bytemuck::cast_slice(&gf));

        let n_inst = insts.len().min(MAX_INST as usize);
        let mut idata: Vec<f32> = Vec::with_capacity(n_inst * 24);
        for i in &insts[..n_inst] {
            idata.extend_from_slice(&model_mat(i).to_cols_array());
            idata.extend_from_slice(&[i.color[0], i.color[1], i.color[2], 1.0]);
            idata.extend_from_slice(&[i.metallic, i.roughness, i.emissive, 0.0]);
        }
        if !idata.is_empty() { self.queue.write_buffer(&self.inst, 0, bytemuck::cast_slice(&idata)); }

        let mut enc = self.device.create_command_encoder(&Default::default());
        let geom = |rp: &mut wgpu::RenderPass, pipe: &wgpu::RenderPipeline, bnd: &wgpu::BindGroup| {
            if n_inst > 0 {
                rp.set_pipeline(pipe);
                rp.set_bind_group(0, bnd, &[]);
                rp.set_vertex_buffer(0, self.vbuf.slice(..));
                rp.set_vertex_buffer(1, self.inst.slice(..));
                rp.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint16);
                rp.draw_indexed(0..self.idx_count, 0, 0..n_inst as u32);
            }
        };
        // PASS 1 — shadow map
        {
            let mut sp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None, color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            geom(&mut sp, &self.shadow_pipe, &self.shadow_bind);
        }
        // PASS 2 — main
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: g.horizon[0] as f64, g: g.horizon[1] as f64, b: g.horizon[2] as f64, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            geom(&mut rp, &self.pipeline, &self.bind);
        }
        self.queue.submit([enc.finish()]);
    }
}

async fn render_async(g: &Globals, insts: &[Instance], w: u32, h: u32) -> Vec<u8> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no GPU adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.expect("no device");
    let fmt = wgpu::TextureFormat::Rgba8Unorm;
    let r = Renderer::new(device, queue, fmt, w, h);
    let color = r.device().create_texture(&wgpu::TextureDescriptor {
        label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: fmt, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
    });
    let color_view = color.create_view(&Default::default());
    r.draw(&color_view, g, insts);

    // copy color → readback buffer (bytes_per_row 256-aligned)
    let bpr = align256(w * 4);
    let rb = r.device().create_buffer(&wgpu::BufferDescriptor {
        label: None, size: (bpr * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
    });
    let mut enc = r.device().create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &rb, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    r.queue().submit([enc.finish()]);

    let slice = rb.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    r.device().poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * bpr) as usize;
        out.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    out
}

fn make_buf(device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let b = device.create_buffer(&wgpu::BufferDescriptor {
        label: None, size: data.len() as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST, // COPY_DST or writes silently no-op
        mapped_at_creation: false,
    });
    queue.write_buffer(&b, 0, data);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ports of kami.webgpu.geometry (sphere/cylinder) for cross-platform parity tests ----
    fn push_v6(v: &mut Vec<f32>, p: [f64; 3], n: [f64; 3]) {
        v.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32, n[0] as f32, n[1] as f32, n[2] as f32]);
    }
    fn geo_sphere(r: f32, rings: usize, sectors: usize) -> (Vec<f32>, Vec<u16>) {
        let pi = std::f64::consts::PI;
        let mut v = Vec::new();
        for i in 0..=rings {
            for j in 0..=sectors {
                let phi = pi * i as f64 / rings as f64;
                let th = 2.0 * pi * j as f64 / sectors as f64;
                let n = [phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin()];
                push_v6(&mut v, [r as f64 * n[0], r as f64 * n[1], r as f64 * n[2]], n);
            }
        }
        let stride = (sectors + 1) as u16;
        let mut idx = Vec::new();
        for i in 0..rings {
            for j in 0..sectors {
                let a = (i * (sectors + 1) + j) as u16;
                idx.extend_from_slice(&[a, a + 1, a + stride + 1, a, a + stride + 1, a + stride]);
            }
        }
        (v, idx)
    }
    fn cyl_ring(r: f32, sectors: usize, y: f64) -> Vec<[f64; 3]> {
        let pi = std::f64::consts::PI;
        (0..=sectors).map(|j| {
            let th = 2.0 * pi * j as f64 / sectors as f64;
            [r as f64 * th.cos(), y, r as f64 * th.sin()]
        }).collect()
    }
    fn geo_cylinder(r: f32, h: f32, sectors: usize) -> (Vec<f32>, Vec<u16>) {
        let hy = h as f64 / 2.0;
        let (top, bot) = (cyl_ring(r, sectors, hy), cyl_ring(r, sectors, -hy));
        let mut v = Vec::new();
        for j in 0..top.len() {
            let [x, _, z] = top[j];
            let m = (x * x + z * z).sqrt().max(1e-6);
            let n = [x / m, 0.0, z / m];
            push_v6(&mut v, top[j], n);
            push_v6(&mut v, bot[j], n);
        }
        let mut idx = Vec::new();
        for j in 0..sectors {
            let a = (2 * j) as u16;
            idx.extend_from_slice(&[a, a + 1, a + 3, a, a + 3, a + 2]);
        }
        let mut cap = |v: &mut Vec<f32>, idx: &mut Vec<u16>, y: f64, ny: [f64; 3], dir: i32, base: u16| {
            push_v6(v, [0.0, y, 0.0], ny);
            for p in cyl_ring(r, sectors, y) { push_v6(v, p, ny); }
            for j in 0..sectors as u16 {
                if dir > 0 { idx.extend_from_slice(&[base, base + 1 + j, base + 2 + j]); }
                else { idx.extend_from_slice(&[base, base + 2 + j, base + 1 + j]); }
            }
        };
        let nv = (2 * top.len()) as u16;
        cap(&mut v, &mut idx, hy, [0.0, 1.0, 0.0], 1, nv);
        cap(&mut v, &mut idx, -hy, [0.0, -1.0, 0.0], -1, nv + (1 + top.len()) as u16);
        (v, idx)
    }
    // The shaders are generated from kami.shaders/lit-shader (bb gen-wgsl). Assert the include_str'd
    // WGSL matches the cljc canonical fixture token-for-token, so native can't drift from the web
    // shader. Skips if kami-webgpu isn't co-located (same policy as the geometry goldens).
    fn shader_canon(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace() && *c != '(' && *c != ')').collect()
    }
    fn assert_shader_parity(fixture: &str, native: &str) {
        let path = format!("{}/../../kami-webgpu/fixtures/{}", env!("CARGO_MANIFEST_DIR"), fixture);
        let Ok(golden) = std::fs::read_to_string(&path) else {
            eprintln!("skip: {fixture} not found (kami-webgpu not co-located)"); return;
        };
        assert_eq!(shader_canon(native), shader_canon(&golden),
                   "native shader must be token-equivalent to the kami.shaders canonical (run bb gen-wgsl)");
    }
    #[test]
    fn lit_shader_matches_cljc_canonical() { assert_shader_parity("lit-shader.wgsl", SHADER); }
    #[test]
    fn shadow_shader_matches_cljc_canonical() { assert_shader_parity("shadow-shader.wgsl", SHADOW_WGSL); }

    fn load_golden(name: &str) -> Option<(Vec<f32>, Vec<u16>)> {
        let path = format!("{}/../../kami-webgpu/fixtures/{}-golden.json", env!("CARGO_MANIFEST_DIR"), name);
        let json = std::fs::read_to_string(&path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid golden json");
        let gv = v["verts"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
        let gi = v["indices"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u16).collect();
        Some((gv, gi))
    }
    fn assert_parity(name: &str, got: (Vec<f32>, Vec<u16>)) {
        let (gv, gi) = match load_golden(name) {
            Some(g) => g,
            None => { eprintln!("skip: {name} golden not found (kami-webgpu not co-located)"); return; }
        };
        assert_eq!(got.1, gi, "{name} indices must match the CLJC geometry golden exactly");
        assert_eq!(got.0.len(), gv.len(), "{name} vertex count");
        // verts go through f64 transcendentals → match to f32 precision (JVM/Rust libm may differ in ulps)
        for (a, b) in got.0.iter().zip(gv.iter()) {
            assert!((a - b).abs() < 1e-4, "{name} vertex parity within f32 precision: {a} vs {b}");
        }
    }

    #[test]
    fn parses_the_same_edn_render_ir() {
        let edn = "{:globals {:sky {:horizon [0.74 0.84 0.95] :sun-dir [-0.4 -0.85 -0.35] :sun [1.0 0.96 0.85]}}
                    :instances [{:pos [0 0 0] :color [0.6 0.6 0.66] :size [2 5] :metallic 0.8 :roughness 0.25}]}";
        let (g, insts) = parse_ir(edn);
        assert_eq!(g.horizon, [0.74, 0.84, 0.95]);
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].size, [2.0, 5.0]);
        assert_eq!(insts[0].metallic, 0.8);
    }

    #[test]
    fn parse_ir_defaults_when_fields_missing() {
        // missing globals → defaults; partial instance → roughness 0.65, metallic/emissive 0
        let (g, insts) = parse_ir("{:instances [{:pos [1 0 2] :color [0.3 0.6 1.0] :size [1 2]}]}");
        assert_eq!(g.sun_dir, [-0.4, -0.85, -0.35], "default sun");
        assert!(g.eye.is_none(), "no camera → overview derived later");
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].roughness, 0.65, "roughness defaults");
        assert_eq!(insts[0].metallic, 0.0);
        assert_eq!(insts[0].emissive, 0.0);
    }

    #[test]
    fn parse_ir_empty_or_malformed() {
        assert_eq!(parse_ir("not-a-map").1.len(), 0);
        assert_eq!(parse_ir("{}").1.len(), 0);
    }

    #[test]
    fn scene_to_ir_scatters_props_and_parses_sky() {
        // the play3d bridge: a kami-clj scene.edn → ground + scattered prop instances
        let scene = "{:render/sky {:horizon [0.74 0.84 0.95] :sun-dir [-0.4 -0.85 -0.35] :sun [1 0.96 0.85]}
                      :render/props {:count 80 :spread 80.0
                        :buildings [{:color [0.62 0.60 0.66] :min-h 2 :max-h 6 :w 2 :metallic 0.8 :roughness 0.25}]
                        :trees {:color [0.28 0.55 0.30] :h 2.6 :w 1.1 :ratio 0.4 :roughness 0.95}}}";
        let (g, insts) = scene_to_ir(scene);
        assert_eq!(g.horizon, [0.74, 0.84, 0.95], "sky parsed");
        assert_eq!(insts[0].size, [400.0, 1.0], "first instance is the ground plane");
        assert!(insts.len() > 20, "ground + scattered props: {}", insts.len());
    }

    #[test]
    fn scene_to_ir_applies_camera_rig() {
        // :camera rig (distance/azimuth/height) → eye/target on the globals
        let scene = "{:render/sky {:horizon [0.7 0.8 0.9] :sun-dir [-0.4 -0.85 -0.35] :sun [1 1 1]}
                      :camera {:distance 70.0 :height 48.0 :azimuth 0.0 :look-height 1.0}
                      :render/props {:count 4 :spread 40.0 :buildings [{:color [0.6 0.6 0.6] :min-h 2 :max-h 4 :w 2}]}}";
        let (g, _) = scene_to_ir(scene);
        let eye = g.eye.expect("camera rig sets eye");
        // azimuth 0 → eye.x = distance*cos(0) = 70, eye.y = height = 48
        assert!((eye[0] - 70.0).abs() < 0.01, "eye.x from distance/azimuth: {}", eye[0]);
        assert_eq!(eye[1], 48.0, "eye.y = height");
        assert_eq!(g.target.unwrap()[1], 1.0, "target.y = look-height");
    }

    #[test]
    fn cube_mesh_shape() {
        let (verts, idx) = cube();
        assert_eq!(verts.len(), 24 * 6, "24 verts × (pos3 + normal3)");
        assert_eq!(idx.len(), 36, "6 faces × 2 tris × 3 indices");
        assert!(idx.iter().all(|&i| i < 24), "all indices reference a real vertex");
        assert_eq!(*idx.iter().max().unwrap(), 23);
    }

    /// Cross-platform geometry parity: the native cube() must be byte-for-byte the canonical box
    /// generated by kami.webgpu.geometry (.cljc) and committed to fixtures/box-golden.json — so
    /// the web and native renderers share ONE geometry source proven by fixture, not two
    /// hand-mirrored copies that can drift. Skips gracefully if kami-webgpu isn't co-located.
    #[test]
    fn cube_matches_cljc_geometry_golden() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kami-webgpu/fixtures/box-golden.json");
        let json = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("skip: golden fixture not found (kami-webgpu not co-located): {path}");
                return;
            }
        };
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid golden json");
        let gv: Vec<f32> = v["verts"].as_array().unwrap().iter()
            .map(|x| x.as_f64().unwrap() as f32).collect();
        let gi: Vec<u16> = v["indices"].as_array().unwrap().iter()
            .map(|x| x.as_u64().unwrap() as u16).collect();
        let (verts, idx) = cube();
        assert_eq!(verts, gv, "native cube vertices must match the CLJC geometry golden");
        assert_eq!(idx, gi, "native cube indices must match the CLJC geometry golden");
    }

    #[test]
    fn sphere_matches_cljc_geometry_golden() {
        assert_parity("sphere", geo_sphere(1.0, 4, 6));
    }

    #[test]
    fn cylinder_matches_cljc_geometry_golden() {
        assert_parity("cylinder", geo_cylinder(1.0, 2.0, 6));
    }

    #[test]
    fn model_mat_translates_lifts_and_scales() {
        let i = Instance {
            pos: [10.0, 0.0, 20.0], color: [1.0, 1.0, 1.0], size: [2.0, 4.0],
            yaw: 0.0, metallic: 0.0, roughness: 0.5, emissive: 0.0,
        };
        let m = model_mat(&i);
        // local origin → world: x,z from pos; y lifted by h/2 so the box sits on the ground
        let p = m.transform_point3(Vec3::ZERO);
        assert!((p.x - 10.0).abs() < 1e-4 && (p.z - 20.0).abs() < 1e-4, "xz from pos: {p:?}");
        assert!((p.y - 2.0).abs() < 1e-4, "y lifted by h/2: {}", p.y);
        // +0.5 local-x corner scales by w=2 → +1 world half-extent
        let c = m.transform_point3(Vec3::new(0.5, 0.0, 0.0));
        assert!((c.x - 11.0).abs() < 1e-4, "scaled half-extent: {}", c.x);
    }

    #[test]
    fn align256_rounds_up_to_256() {
        assert_eq!(align256(1), 256);
        assert_eq!(align256(256), 256);
        assert_eq!(align256(257), 512);
        assert_eq!(align256(3600), 3840); // 900px × 4 bytes (3600) → next 256-multiple
    }

    #[test]
    fn scene_to_ir_is_deterministic() {
        // the xorshift scatter must be reproducible (web + native must agree on the world)
        let scene = "{:render/sky {:horizon [0.7 0.8 0.9] :sun-dir [-0.4 -0.85 -0.35] :sun [1 1 1]}
                      :render/props {:count 50 :spread 60.0
                        :buildings [{:color [0.6 0.6 0.66] :min-h 2 :max-h 6 :w 2}]
                        :trees {:color [0.28 0.55 0.30] :h 2.6 :w 1.1 :ratio 0.4}}}";
        let (_, a) = scene_to_ir(scene);
        let (_, b) = scene_to_ir(scene);
        assert_eq!(a.len(), b.len(), "same instance count");
        assert_eq!(a[1].pos, b[1].pos, "deterministic scatter (fixed seed)");
        assert_eq!(a.last().unwrap().pos, b.last().unwrap().pos);
    }

    #[test]
    fn scene_to_ir_empty_props_is_just_ground() {
        let (g, insts) = scene_to_ir("{:render/sky {:horizon [0.7 0.8 0.9] :sun-dir [0 -1 0] :sun [1 1 1]}}");
        assert_eq!(insts.len(), 1, "no props → only the ground plane");
        assert_eq!(insts[0].size, [400.0, 1.0]);
        assert_eq!(g.horizon, [0.7, 0.8, 0.9]);
    }

    #[test]
    fn scene_to_ir_ground_color_from_sky() {
        let (_, insts) = scene_to_ir("{:render/sky {:horizon [0.7 0.8 0.9] :sun-dir [0 -1 0] :sun [1 1 1] :ground [0.2 0.5 0.3]}}");
        assert_eq!(insts[0].color, [0.2, 0.5, 0.3], "ground plane uses sky :ground");
    }

    #[test]
    fn scatter_rng_matches_the_web() {
        // The native xorshift (seed 2654435769) must produce the same sequence as the web's
        // CLJS scatter (game.cljs) so web + native render the same world from the same EDN.
        // Expected values computed in JS (CLJS-faithful 32-bit xorshift):
        //   node -e 'let s=2654435769>>>0; const r=()=>{s^=s<<13;s^=s>>>17;s^=s<<5;s>>>=0;
        //            return (s&0x7fffffff)/2147483647}; ...'
        let mut seed: u32 = 2654435769;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed & 0x7fffffff) as f32 / 2147483647.0
        };
        let expected = [0.633187f32, 0.751414, 0.9666, 0.01183, 0.798444];
        for e in expected {
            let g = rnd();
            assert!((g - e).abs() < 1e-4, "native rng diverged from web: got {g}, web {e}");
        }
    }

    #[test]
    fn renders_geometry_headless() {
        // a single building filling the view; centre must differ from the sky clear.
        let edn = "{:globals {:sky {:horizon [0.74 0.84 0.95] :sun-dir [-0.4 -0.85 -0.35] :sun [1.0 0.96 0.85]}
                              :eye [6 5 6] :target [0 1 0]}
                    :instances [{:pos [0 0 0] :color [0.85 0.3 0.3] :size [3 4] :roughness 0.6}]}";
        let px = render_to_pixels(edn, 64, 64);
        assert_eq!(px.len(), 64 * 64 * 4);
        let c = ((32 * 64 + 32) * 4) as usize; // centre pixel
        let (r, gc, b) = (px[c], px[c + 1], px[c + 2]);
        let sky = (189u8, 214, 242); // ~horizon in 8-bit
        let is_sky = (r as i32 - sky.0 as i32).abs() < 12
            && (gc as i32 - sky.1 as i32).abs() < 12
            && (b as i32 - sky.2 as i32).abs() < 12;
        assert!(!is_sky, "centre should be the lit building, not sky: got {r},{gc},{b}");
        assert!(r > gc && r > b, "building is reddish: got {r},{gc},{b}");
    }

    #[test]
    fn caster_casts_a_shadow() {
        // a ground plane filling the view; a tall caster should darken the ground (shadow map).
        let cam = ":eye [0 50 22] :target [0 0 0]";
        let sky = ":horizon [0.1 0.1 0.12] :sun-dir [-0.45 -0.8 -0.4] :sun [1 0.96 0.85]";
        let ground = "{:pos [0 -0.5 0] :color [0.7 0.7 0.7] :size [200 1] :roughness 0.95}";
        let caster = "{:pos [0 0 0] :color [0.5 0.5 0.5] :size [5 16] :roughness 0.95}";
        let lit_only = format!("{{:globals {{:sky {{{sky}}} {cam}}} :instances [{ground}]}}");
        let shadowed = format!("{{:globals {{:sky {{{sky}}} {cam}}} :instances [{ground} {caster}]}}");
        // darkest luminance anywhere in the frame
        let darkest = |px: &[u8]| px.chunks(4)
            .map(|c| (c[0] as i32 * 30 + c[1] as i32 * 59 + c[2] as i32 * 11) / 100)
            .min().unwrap_or(0);
        let la = darkest(&render_to_pixels(&lit_only, 96, 96));
        let lb = darkest(&render_to_pixels(&shadowed, 96, 96));
        assert!(lb + 12 < la, "the caster should darken the ground via shadow: lit min={la}, shadowed min={lb}");
    }
}
