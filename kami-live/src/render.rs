//! Render-IR bridge — project a dance [`ShowSnapshot`] into the **EDN render-IR**
//! that the native executor (`kami-webgpu-rs`) and the web CLJS renderer already
//! consume (ADR-0040/0041). The dance show is authored as `:dance/*` EDN, ticked
//! by `LiveShow`, and now *rendered* from data too: one function turns the
//! per-frame snapshot into `{:globals … :instances [...]}` — the same shape
//! `kami_webgpu_rs::parse_ir` reads — so the dancer, crowd, and framing draw on
//! every platform with no per-renderer code.
//!
//! The performer is emitted both as a placeholder instance (for v1 renderers)
//! and, when a VRM is bound, as a skinned `:meshes` avatar with `:material` +
//! `:skin` refs (ADR-0044 phase 3) — so a Phase-3 host draws the real rig. The
//! rig also includes `:lights` from the beat-synced lighting and a `:camera`.
//! Crowd fans render as lit instances.

use kotoba_edn::EdnValue;

use crate::lighting::LightingFixture;
use crate::scene::AvatarBinding;
use crate::show::ShowSnapshot;

#[inline]
fn f(x: f32) -> EdnValue {
    EdnValue::float(x as f64)
}

#[inline]
fn kw(name: &str) -> EdnValue {
    EdnValue::kw_bare(name)
}

fn vec3_edn(v: [f32; 3]) -> EdnValue {
    EdnValue::vector([f(v[0]), f(v[1]), f(v[2])])
}

fn vec2_edn(w: f32, h: f32) -> EdnValue {
    EdnValue::vector([f(w), f(h)])
}

/// One render-IR instance map: `{:pos :color :size :yaw :metallic :roughness :emissive}`.
fn instance(
    pos: [f32; 3],
    color: [f32; 3],
    size: (f32, f32),
    yaw: f32,
    emissive: f32,
) -> EdnValue {
    EdnValue::map([
        (kw("pos"), vec3_edn(pos)),
        (kw("color"), vec3_edn(color)),
        (kw("size"), vec2_edn(size.0, size.1)),
        (kw("yaw"), f(yaw)),
        (kw("metallic"), f(0.0)),
        (kw("roughness"), f(0.7)),
        (kw("emissive"), f(emissive)),
    ])
}

/// Project the snapshot + avatar binding into the render-IR [`EdnValue`].
///
/// - **performer** → one instance at `pose.root_translation`, sized by the
///   avatar `scale` (placeholder until the VRM rig is bound host-side).
/// - **crowd** → one lit instance per fan; raised lightsticks glow (emissive).
/// - **globals** → sky tinted by the current VJ palette, camera framed behind
///   and above the performer.
pub fn show_to_render_ir(snap: &ShowSnapshot, avatar: &AvatarBinding) -> EdnValue {
    let pose = &snap.performer_pose;
    let p = pose.root_translation;
    let s = avatar.scale.max(0.01);

    let mut instances: Vec<EdnValue> = Vec::with_capacity(snap.crowd.len() + 1);

    // performer (placeholder cuboid at the danced pose; warm key tone).
    let perf_y = p.y + pose.vertical_bob;
    instances.push(instance(
        [p.x, perf_y, p.z],
        [1.0, 0.82, 0.72],
        (0.9 * s, 1.8 * s),
        pose.root_yaw,
        0.12 + 0.3 * pose.arms_up,
    ));

    // crowd — lit instances; raised sticks glow with their stick colour.
    for fan in &snap.crowd {
        let pos = [fan.position.x, fan.position.y, fan.position.z];
        let (color, emissive) = if fan.stick_raised {
            (fan.stick_color, 0.6)
        } else {
            ([0.28, 0.30, 0.38], 0.0)
        };
        instances.push(instance(pos, color, (0.45, fan.body_height.max(0.2)), 0.0, emissive));
    }

    // globals: sky tinted by the VJ palette, camera behind+above the performer.
    let tint = snap.vj.palette.primary;
    let sky = EdnValue::map([
        (kw("horizon"), vec3_edn([
            0.15 + 0.4 * tint[0],
            0.18 + 0.4 * tint[1],
            0.22 + 0.4 * tint[2],
        ])),
        (kw("sun-dir"), vec3_edn([-0.4, -0.85, -0.35])),
        (kw("sun"), vec3_edn([1.0, 0.96, 0.85])),
    ]);
    let globals = EdnValue::map([
        (kw("sky"), sky),
        (kw("eye"), vec3_edn([p.x, p.y + 3.0, p.z + 8.0])),
        (kw("target"), vec3_edn([p.x, p.y + 1.0, p.z])),
    ]);

    // ── ADR-0044 vocabulary: lights / camera / materials / meshes ───────────
    // Lights from the beat-synced rig (front/back/spot/strobe/laser fixtures).
    let lights: Vec<EdnValue> = snap
        .lighting
        .iter()
        .map(|lf| {
            EdnValue::map([
                (kw("kind"), kw(fixture_light_kind(lf.fixture))),
                (kw("color"), vec3_edn(lf.color)),
                (kw("intensity"), f(lf.intensity)),
                (kw("dir"), vec3_edn([lf.aim.x, lf.aim.y, lf.aim.z])),
                (kw("cast-shadow"), EdnValue::bool(matches!(lf.fixture, LightingFixture::Spot))),
            ])
        })
        .collect();

    // Explicit camera (fov/near/far) framing the performer.
    let camera = EdnValue::map([
        (kw("eye"), vec3_edn([p.x, p.y + 3.0, p.z + 8.0])),
        (kw("target"), vec3_edn([p.x, p.y + 1.0, p.z])),
        (kw("fov"), f(0.9)),
        (kw("near"), f(0.1)),
        (kw("far"), f(500.0)),
    ]);

    // The performer's MToon material + (when a VRM is bound) the skinned avatar
    // mesh — so a Phase-3 host draws the real rig instead of the placeholder box.
    let materials = vec![EdnValue::map([
        (kw("id"), kw("performer")),
        (kw("model"), kw("mtoon")),
        (kw("base"), vec3_edn([1.0, 0.82, 0.72])),
        (kw("shade"), vec3_edn([0.7, 0.55, 0.5])),
        (kw("alpha-mode"), kw("mask")),
        (kw("alpha-cutoff"), f(0.5)),
    ])];
    let mut meshes: Vec<EdnValue> = Vec::new();
    if !avatar.vrm.is_empty() {
        // VRM expressions driven by the same show (mirrors the Live2D param
        // driver): cheer → happy, beat front → mouth (aa), periodic blink.
        let happy = (snap.cheer_loudness / 40.0).clamp(0.0, 1.0);
        let aa = ((1.0 - (snap.phase.beat_frac * std::f32::consts::TAU).cos()) * 0.5).clamp(0.0, 1.0);
        let blink = blink_expr(snap.phase.time);
        let expressions = EdnValue::map([
            (kw("happy"), f(happy)),
            (kw("aa"), f(aa)),
            (kw("blink"), f(blink)),
        ]);
        meshes.push(EdnValue::map([
            (kw("id"), kw("performer")),
            (kw("url"), EdnValue::string(avatar.vrm.clone())),
            (kw("pos"), vec3_edn([p.x, perf_y, p.z])),
            (kw("rot"), EdnValue::vector([f(0.0), f((pose.root_yaw * 0.5).sin()), f(0.0), f((pose.root_yaw * 0.5).cos())])),
            (kw("scale"), f(s)),
            (kw("material"), kw("performer")),
            (kw("skin"), kw("rig")),
            (kw("expressions"), expressions),
            (kw("cast-shadow"), EdnValue::bool(true)),
        ]));
    }

    // Base animation layer: the avatar's clip played at show time (the host
    // loads the clip from :dance/clips via kami_skeleton_scene::clip_from_edn and
    // blends it via evaluate_blend — ADR-0044 phase 4).
    let mut animations: Vec<EdnValue> = Vec::new();
    if let Some(clip) = &avatar.clip {
        animations.push(EdnValue::map([
            (kw("target"), kw("performer")),
            (kw("clip"), EdnValue::string(clip.clone())),
            (kw("time"), f(snap.phase.time)),
            (kw("interp"), kw("linear")),
            (kw("weight"), f(1.0)),
        ]));
    }

    EdnValue::map([
        (kw("globals"), globals),
        (kw("camera"), camera),
        (kw("lights"), EdnValue::vector(lights)),
        (kw("materials"), EdnValue::vector(materials)),
        (kw("meshes"), EdnValue::vector(meshes)),
        (kw("animations"), EdnValue::vector(animations)),
        (kw("instances"), EdnValue::vector(instances)),
    ])
}

/// VRM `blink` expression weight: ~0 (eyes open) most of the time, spiking to 1
/// (closed) for a quick blink every ~3 s. Deterministic on show time.
fn blink_expr(t: f32) -> f32 {
    let m = t - 3.0 * (t / 3.0).floor(); // t mod 3
    if m < 0.12 {
        1.0 - (m / 0.06 - 1.0).abs().min(1.0)
    } else {
        0.0
    }
}

/// Map a lighting fixture to a render-IR light kind: moving heads (spot) project
/// a cone; everything else washes as a directional.
fn fixture_light_kind(fixture: LightingFixture) -> &'static str {
    match fixture {
        LightingFixture::Spot => "spot",
        _ => "directional",
    }
}

/// Serialise [`show_to_render_ir`] to an EDN string — feed straight to
/// `kami_webgpu_rs::parse_ir` (native) or the web reader.
pub fn show_to_render_ir_edn(snap: &ShowSnapshot, avatar: &AvatarBinding) -> String {
    kotoba_edn::to_string(&show_to_render_ir(snap, avatar))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::DanceScene;
    use kami_scene::{mget, root_map, vec3};

    const SCENE: &str = r#"
    {:dance/show   {:bpm 120.0 :stage :club}
     :dance/avatar {:vrm "m.vrm" :home [0.0 1.0 0.0] :scale 1.0}
     :dance/crowd  {:fans 40 :seed 3}
     :dance/vj     [{:pattern :stripes :palette :neon-pink}]
     :dance/setlist [{:title "A" :bpm 120.0 :bars 8 :dance :wota
                      :cues [{:beat 0 :kind :drop :tag "d"}]}]}
    "#;

    fn snap_after(secs: f32) -> (ShowSnapshot, AvatarBinding) {
        let mut sc = DanceScene::from_edn(SCENE).expect("scene");
        sc.show.start();
        let steps = (secs / (1.0 / 60.0)) as i32;
        for _ in 0..steps {
            sc.show.tick(1.0 / 60.0);
        }
        (sc.show.snapshot(), sc.avatar)
    }

    #[test]
    fn render_ir_is_well_formed_edn() {
        let (snap, avatar) = snap_after(1.0);
        let edn = show_to_render_ir_edn(&snap, &avatar);
        // re-parse with the same accessors the renderers use.
        let root = root_map(&edn).expect("render-ir parses as a map");
        assert!(mget(&root, "globals").and_then(|v| v.as_map()).is_some());
        let insts = mget(&root, "instances")
            .and_then(|v| v.as_vector())
            .expect("instances vector");
        // performer + crowd fans.
        assert!(insts.len() >= 1, "at least the performer instance");
        let perf = insts[0].as_map().expect("instance map");
        let pos = vec3(mget(perf, "pos"));
        // performer home y=1.0 (+ bob); x/z near origin.
        assert!(pos[1] > 0.5, "performer lifted to home height, got {pos:?}");
    }

    #[test]
    fn camera_tracks_performer() {
        let (snap, avatar) = snap_after(0.5);
        let root = root_map(&show_to_render_ir_edn(&snap, &avatar)).unwrap();
        let g = mget(&root, "globals").and_then(|v| v.as_map()).unwrap();
        let eye = vec3(mget(g, "eye"));
        let target = vec3(mget(g, "target"));
        assert!(eye[2] > target[2], "eye sits behind the performer on +z");
        assert!(eye[1] > target[1], "eye sits above the target");
    }

    #[test]
    fn emits_vrm_avatar_mesh_and_lights() {
        // The dance render-IR carries the avatar as a skinned :meshes entry
        // (ADR-0044 phase 3) plus the lighting rig as :lights — not just cuboids.
        let (snap, avatar) = snap_after(0.2);
        let root = root_map(&show_to_render_ir_edn(&snap, &avatar)).unwrap();
        // avatar mesh present, bound to the VRM and the performer material.
        let meshes = mget(&root, "meshes").and_then(|v| v.as_vector()).expect("meshes");
        assert_eq!(meshes.len(), 1, "the bound VRM avatar");
        let m = meshes[0].as_map().unwrap();
        assert_eq!(mget(m, "url").and_then(|v| v.as_string()), Some("m.vrm"));
        assert_eq!(mget(m, "skin").and_then(|v| v.as_keyword()).map(|k| k.0.name.as_str()), Some("rig"));
        // VRM expressions driven by the show (happy/aa/blink present).
        let ex = mget(m, "expressions").and_then(|v| v.as_map()).expect("expressions");
        assert!(mget(ex, "aa").is_some(), "lipsync mouth expression");
        assert!(mget(ex, "blink").is_some());
        // lights emitted from the rig; camera present.
        assert!(mget(&root, "lights").and_then(|v| v.as_vector()).map_or(false, |l| !l.is_empty()));
        assert!(mget(&root, "camera").and_then(|v| v.as_map()).is_some());
        // performer material is mtoon with mask cutout.
        let mats = mget(&root, "materials").and_then(|v| v.as_vector()).unwrap();
        let pm = mats[0].as_map().unwrap();
        assert_eq!(mget(pm, "model").and_then(|v| v.as_keyword()).map(|k| k.0.name.as_str()), Some("mtoon"));
    }

    #[test]
    fn emits_animation_layer_for_avatar_clip() {
        let (snap, mut avatar) = snap_after(0.5);
        avatar.clip = Some("idle".into());
        let root = root_map(&show_to_render_ir_edn(&snap, &avatar)).unwrap();
        let anims = mget(&root, "animations").and_then(|v| v.as_vector()).expect("animations");
        assert_eq!(anims.len(), 1);
        let a = anims[0].as_map().unwrap();
        assert_eq!(mget(a, "clip").and_then(|v| v.as_string()), Some("idle"));
        assert_eq!(mget(a, "target").and_then(|v| v.as_keyword()).map(|k| k.0.name.as_str()), Some("performer"));
        assert!(mget(a, "time").is_some());
    }

    #[test]
    fn no_animation_layer_without_clip() {
        let (snap, avatar) = snap_after(0.2); // example SCENE avatar has no :clip
        let root = root_map(&show_to_render_ir_edn(&snap, &avatar)).unwrap();
        let anims = mget(&root, "animations").and_then(|v| v.as_vector()).unwrap();
        assert!(anims.is_empty());
    }

    #[test]
    fn no_mesh_when_avatar_unbound() {
        let (snap, mut avatar) = snap_after(0.2);
        avatar.vrm = String::new();
        let root = root_map(&show_to_render_ir_edn(&snap, &avatar)).unwrap();
        let meshes = mget(&root, "meshes").and_then(|v| v.as_vector()).unwrap();
        assert!(meshes.is_empty(), "no VRM bound → no avatar mesh");
    }

    #[test]
    fn deterministic_render_ir() {
        let a = {
            let (s, av) = snap_after(0.75);
            show_to_render_ir_edn(&s, &av)
        };
        let b = {
            let (s, av) = snap_after(0.75);
            show_to_render_ir_edn(&s, &av)
        };
        assert_eq!(a, b, "same EDN scene + time → identical render-IR");
    }
}
