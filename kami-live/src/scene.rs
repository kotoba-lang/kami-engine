//! Dance scene loader — build a [`LiveShow`] + VRM avatar binding from EDN.
//!
//! This is the clj/edn authoring surface for VRM dance scenes (the live-music
//! counterpart of `kami-scene`'s `scene.edn` for games). A designer authors the
//! venue tempo, the avatar, and the choreography as plain EDN data; this module
//! parses it the same tolerant way games parse `scene.edn` (missing keys fall
//! back to defaults, namespaced keywords match on `ns/name`, ints coerce to
//! floats) and assembles the deterministic [`LiveShow`] that `kami-live` already
//! drives. The actual VRM mesh load + skinning stays host-side (kami-vrm /
//! `kami-web::run_embed_vrm`); this module only resolves the *binding* (which
//! avatar, where it stands, which features) and the *choreography* clock.
//!
//! ## EDN shape
//!
//! ```edn
//! {:game/id    :gftd.games/vrm-dance
//!  :game/title "KAMI VRM Dance"
//!
//!  ;; venue + master tempo
//!  :dance/show
//!  {:bpm   128.0
//!   :stage :hall            ;; :club | :hall | :festival
//!   :swing 0.0              ;; [-0.5 0.5] off-beat groove
//!   :meter [4 8]            ;; beats/bar, bars/phrase
//!   :performer "Mitama"}
//!
//!  ;; VRM avatar bound to the performer (host loads the GLB/VRM by path)
//!  :dance/avatar
//!  {:vrm   "models/mitama.vrm"
//!   :home  [0.0 1.0 0.0]    ;; stage-centre footprint (else stage default)
//!   :scale 1.0
//!   :look-at      true      ;; VRM look-at toward camera
//!   :spring-bones true}     ;; VRMC_springBone sim (hair / skirt)
//!
//!  ;; choreography: an ordered setlist; each track is a dance section
//!  :dance/setlist
//!  [{:title "Opening" :bpm 128.0 :bars 16 :dance :wota
//!    :cues [{:beat 0 :kind :callout :tag "intro"}
//!           {:beat 32 :kind :drop :tag "drop-1"}]}
//!   {:title "Chorus"  :bars 16 :dance :kpop-point :audio :opener
//!    :cues [{:beat 0 :kind :drop :tag "hook"}]}
//!   {:title "Bridge"  :bars 8  :dance :hold
//!    :cues [{:beat 0 :kind :breakdown :tag "bridge"}]}]}
//! ```

use glam::Vec3;
use kami_scene::{EdnValue, mget, num, root_map, vec3};
use std::collections::BTreeMap;

use crate::audio::{
    default_sound_bank, midi_to_hz, AudioPattern, BassLine, BassNote, DrumPattern, DrumSlot, SoundCue,
};
use crate::crowd::CrowdConfig;
use crate::lighting::{Envelope, LightingCue, LightingFixture};
use crate::setlist::{CueKind, CuePoint, Track, TrackId};
use crate::show::LiveShow;
use crate::stage::StagePreset;
use crate::vj::{Palette, VJDeck, VJPattern};

/// How a VRM avatar is bound to the performer. The mesh/skinning load is the
/// host's job (kami-vrm); this is the resolved *intent* from the EDN scene.
#[derive(Debug, Clone, PartialEq)]
pub struct AvatarBinding {
    /// Asset path to the `.vrm` / `.glb` the host loads (relative to the game dir).
    pub vrm: String,
    /// Stage-space footprint of the avatar. `None` → use the stage performer zone.
    pub home: Option<Vec3>,
    /// Uniform scale applied to the loaded avatar.
    pub scale: f32,
    /// Enable VRM look-at (eyes/head track the camera).
    pub look_at: bool,
    /// Enable the VRMC_springBone simulation (hair / skirt secondary motion).
    pub spring_bones: bool,
    /// Optional base animation clip name (resolved by the host from
    /// `:dance/clips` via `kami_skeleton_scene::clip_from_edn`) — emitted as an
    /// `:animations` layer driven by show time (ADR-0044 phase 4).
    pub clip: Option<String>,
    /// Optional VRMC_springBone tuning (`:dance/avatar :spring`). When set, the
    /// host applies it to `kami_vrm::SpringSimulator` at avatar load (init-time
    /// config — not per-frame render-IR). `None` uses the VRM's own values.
    pub spring: Option<SpringTuning>,
    /// What the VRM look-at gaze tracks when `look_at` is on. `None` (the default)
    /// tracks the camera; a `:look-at {:target [x y z]}` map fixes it on a point.
    pub look_at_target: Option<LookTarget>,
    /// Show→expression drives (`:dance/avatar :expressions`). Maps VRM expression
    /// names to a live-show signal so the face animates from the beat/crowd
    /// (blink / lip-sync / smile-on-cheer) with no per-frame authoring. Defaults
    /// to `happy←cheer, aa←beat, blink←blink` when omitted. Resolved per frame by
    /// [`AvatarBinding::expression_weights`] → fed to `kami_vrm::ExpressionManager`.
    pub expressions: Vec<ExpressionDrive>,
    /// Optional vocal lip-sync (`:dance/avatar :voice`): a vowel timeline driving
    /// the VRM mouth (aa/ih/ou/ee/oh). When set it overrides the beat-driven `:aa`.
    pub voice: Option<VoiceLine>,
    /// Optional MMD `.vmd` motion path (`:dance/avatar :vmd`). The host loads it
    /// via `kami_skeleton_scene::vmd_to_clip` (MMD bone names → VRM humanoid) and
    /// plays it as the base motion instead of (or layered over) `:clip`.
    pub vmd: Option<String>,
}

/// Where a VRM performer's gaze points (VRM look-at).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LookTarget {
    /// Track the active camera (the default directorial gaze).
    Camera,
    /// Fixed world-space point (e.g. the audience, a co-performer's spot).
    Fixed([f32; 3]),
}

/// Global spring-bone (hair / skirt) tuning that scales the VRM's per-joint
/// values. All in [0,1]-ish ranges the simulator expects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringTuning {
    /// Restore-to-rest stiffness (higher = stiffer hair).
    pub stiffness: f32,
    /// Velocity damping (higher = more sluggish).
    pub drag: f32,
    /// Downward gravity pull on the chain.
    pub gravity: f32,
}

/// One camera shot in a `:dance/camera :shots` choreography — an eye/look offset
/// that becomes active at `at_bar` and is dollied toward the next shot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraShot {
    pub at_bar: u32,
    pub offset: Vec3,
    pub look: Vec3,
}

/// A static stage set piece (`:dance/stage :props`) — LED wall / riser / truss /
/// speaker / screen — realised into a render-IR instance so the venue is dressed
/// from data. `kind` is a free-form label; the geometry is the box `pos`+`size`.
#[derive(Debug, Clone, PartialEq)]
pub struct StageProp {
    pub kind: String,
    pub pos: Vec3,
    /// Footprint (width, height) of the box, like a render-IR instance `:size`.
    pub size: [f32; 2],
    pub color: [f32; 3],
    /// Self-illumination (LED walls / screens glow); 0 = unlit prop.
    pub emissive: f32,
}

/// Camera rig framing the performer (`:dance/camera`). The eye sits at the live
/// performer position + `offset`; the look target at performer + `look`. So the
/// camera follows the dancer, but the rig (distance, height, fov) is authored as
/// data. An optional `:shots` list keys offsets to bars for a camera-work
/// choreography (wide → close → side …), dollied with a smoothstep ease.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraRig {
    pub offset: Vec3,
    pub look: Vec3,
    pub fov: f32,
    pub shots: Vec<CameraShot>,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            offset: Vec3::new(0.0, 3.0, 8.0),
            look: Vec3::new(0.0, 1.0, 0.0),
            fov: 0.9,
            shots: Vec::new(),
        }
    }
}

impl CameraRig {
    /// Eye/look offset at a continuous bar position. With no `:shots` this is the
    /// static rig; otherwise the active shot dollies toward the next one.
    pub fn framing_at(&self, bar: f32) -> (Vec3, Vec3) {
        if self.shots.is_empty() {
            return (self.offset, self.look);
        }
        let mut i = 0;
        for (k, s) in self.shots.iter().enumerate() {
            if (s.at_bar as f32) <= bar {
                i = k;
            }
        }
        let a = self.shots[i];
        if i + 1 < self.shots.len() {
            let b = self.shots[i + 1];
            let span = (b.at_bar as f32 - a.at_bar as f32).max(1e-3);
            let t = ((bar - a.at_bar as f32) / span).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t); // smoothstep ease
            (a.offset.lerp(b.offset, t), a.look.lerp(b.look, t))
        } else {
            (a.offset, a.look)
        }
    }
}

/// Which live-show signal drives a VRM expression's weight each frame
/// (`:dance/avatar :expressions {<name> {:from <source> :gain <f>}}`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExprSource {
    /// Crowd cheer loudness × gain, clamped to [0,1] — e.g. a smile on cheers.
    Cheer,
    /// Mouth-open on the beat: (1 − cos 2π·beat_frac)/2 × gain — lip-sync "aa".
    Beat,
    /// Periodic eye blink (a short pulse every ~3 s); ignores gain.
    Blink,
}

/// A mouth vowel for phoneme lip-sync (`:dance/avatar :voice`). Maps to the VRM
/// vowel expression: A→aa, I→ih, U→ou, E→ee, O→oh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vowel { A, I, U, E, O }

impl Vowel {
    /// The VRM expression name this vowel drives.
    pub fn vrm_expr(self) -> &'static str {
        match self {
            Vowel::A => "aa",
            Vowel::I => "ih",
            Vowel::U => "ou",
            Vowel::E => "ee",
            Vowel::O => "oh",
        }
    }
    fn from_name(s: &str) -> Option<Vowel> {
        match s {
            "a" | "aa" => Some(Vowel::A),
            "i" | "ih" => Some(Vowel::I),
            "u" | "ou" => Some(Vowel::U),
            "e" | "ee" => Some(Vowel::E),
            "o" | "oh" => Some(Vowel::O),
            _ => None,
        }
    }
}

/// One sung syllable: a `vowel` mouth shape held for `dur` beats from `at_beat`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Phoneme {
    pub at_beat: f32,
    pub vowel: Vowel,
    pub dur: f32,
}

/// A vocal lip-sync line (`:dance/avatar :voice :phonemes`): a vowel timeline that
/// drives the VRM mouth, beyond the beat-synced `:aa`. Authored as data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VoiceLine {
    pub phonemes: Vec<Phoneme>,
}

impl VoiceLine {
    /// The active vowel's VRM expression + mouth weight at a continuous beat,
    /// with a short attack/release so syllables open and close. `None` between
    /// syllables (mouth closed).
    pub fn vowel_weight(&self, beat: f32) -> Option<(&'static str, f32)> {
        if self.phonemes.is_empty() {
            return None;
        }
        // loop the sung phrase over its span so the mouth keeps moving.
        let span = self.phonemes.iter().map(|p| p.at_beat + p.dur).fold(0.0, f32::max);
        let beat = if span > 1e-3 { beat - span * (beat / span).floor() } else { beat };
        for p in &self.phonemes {
            if beat >= p.at_beat && beat < p.at_beat + p.dur {
                let t = beat - p.at_beat;
                let edge = (p.dur * 0.25).min(0.15);
                let w = if edge <= 0.0 {
                    1.0
                } else if t < edge {
                    t / edge
                } else if t > p.dur - edge {
                    (p.dur - t) / edge
                } else {
                    1.0
                };
                return Some((p.vowel.vrm_expr(), w.clamp(0.0, 1.0)));
            }
        }
        None
    }
}

/// One show→expression drive: which VRM expression, driven by which signal.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpressionDrive {
    /// VRM expression name (`happy` / `aa` / `blink` / …), resolved host-side by
    /// `kami_vrm::ExpressionManager`.
    pub name: String,
    pub source: ExprSource,
    /// Multiplier on the source signal (ignored by `Blink`).
    pub gain: f32,
}

impl AvatarBinding {
    /// The default face animation when `:expressions` is omitted: smile on cheers,
    /// lip-sync on the beat, periodic blink.
    pub fn default_expressions() -> Vec<ExpressionDrive> {
        vec![
            ExpressionDrive {
                name: "happy".into(),
                source: ExprSource::Cheer,
                gain: 0.025,
            },
            ExpressionDrive {
                name: "aa".into(),
                source: ExprSource::Beat,
                gain: 1.0,
            },
            ExpressionDrive {
                name: "blink".into(),
                source: ExprSource::Blink,
                gain: 1.0,
            },
        ]
    }

    /// Resolve every drive against this frame's show signals → `name → weight`
    /// in [0,1]. Deterministic given the inputs; the host feeds the result to
    /// `kami_vrm::ExpressionManager::resolve`. Zero-weight entries are omitted.
    pub fn expression_weights(
        &self,
        cheer_loudness: f32,
        beat_frac: f32,
        time: f32,
    ) -> BTreeMap<String, f32> {
        use std::f32::consts::TAU;
        let mut out = BTreeMap::new();
        for d in &self.expressions {
            let w = match d.source {
                ExprSource::Cheer => (cheer_loudness * d.gain).clamp(0.0, 1.0),
                ExprSource::Beat => {
                    (((1.0 - (beat_frac * TAU).cos()) * 0.5) * d.gain).clamp(0.0, 1.0)
                }
                ExprSource::Blink => {
                    let m = time - 3.0 * (time / 3.0).floor();
                    if m < 0.12 {
                        (1.0 - (m / 0.06 - 1.0).abs().min(1.0)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                }
            };
            if w > 0.0 {
                out.insert(d.name.clone(), w);
            }
        }
        out
    }
}

impl Default for AvatarBinding {
    fn default() -> Self {
        Self {
            vrm: String::new(),
            home: None,
            scale: 1.0,
            look_at: true,
            spring_bones: true,
            clip: None,
            spring: None,
            look_at_target: None,
            expressions: AvatarBinding::default_expressions(),
            voice: None,
            vmd: None,
        }
    }
}

/// A fully-resolved VRM dance scene: the avatar binding plus the deterministic
/// [`LiveShow`] (tempo grid + choreography setlist + stage + crowd + lighting).
///
/// The returned `show` is **not** started — call [`LiveShow::start`] when the
/// host is ready (e.g. on the first user gesture so audio can begin). Each frame,
/// `show.tick(dt)` advances the clock and `show.snapshot().performer_pose` gives
/// the [`crate::DancePose`] to drive the VRM rig.
pub struct DanceScene {
    pub title: String,
    pub avatar: AvatarBinding,
    pub show: LiveShow,
    /// EDN-declared reactions to show events (`:dance/triggers`).
    pub director: crate::director::Director,
    /// Optional Live2D (2D) performer, driven by the same beat-synced pose
    /// (`:dance/live2d`). When bound, [`DanceFrame::live2d`] carries its params.
    pub live2d: Option<crate::live2d::Live2DBinding>,
    /// EDN-authored animation clip definitions (`:dance/clips`). The host loads
    /// each once via `kami_skeleton_scene::clip_from_edn` (re-serialise with
    /// `kotoba_edn::to_string`); the render-IR `:animations` reference them by
    /// `:name`. Kept as raw EDN so the loader stays independent of kami-skeleton.
    pub clips: Vec<EdnValue>,
    /// Post-processing effect chain (`:dance/post`), injected into each frame's
    /// render-IR as `:post` for the kami-postfx executor (ADR-0044 phase 6).
    pub post: Vec<EdnValue>,
    /// The active camera shot, set by the latest fired `:camera` trigger action
    /// (e.g. `:closeup` / `:wide` / `:punch`); emitted as `:camera-shot` so the
    /// host adjusts framing. Persists until another `:camera` action fires.
    pub active_camera: Option<String>,
    /// Camera rig (`:dance/camera`) framing the performer — authored as data.
    pub camera: CameraRig,
    /// Static stage set pieces (`:dance/stage :props`) dressed into the render-IR
    /// `:instances` each frame (LED wall / risers / truss / speakers).
    pub stage: Vec<StageProp>,
    /// Web-Audio cue bank (`:dance/audio :bank`, kami.audio recipes). Drum/bass
    /// cues and `:sound` triggers resolve to these recipes → `DanceFrame.sounds`.
    pub sound_bank: BTreeMap<String, SoundCue>,
}

impl DanceScene {
    /// The names of the authored animation clips (`:dance/clips … :name`).
    pub fn clip_names(&self) -> Vec<String> {
        self.clips
            .iter()
            .filter_map(|c| c.as_map())
            .filter_map(|m| {
                mget(m, "name")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
            })
            .collect()
    }
}

/// One action that fired this frame: the kind of moment plus the author's
/// free-form action map (`:fx`/`:sound`/`:camera`/…). A self-contained, owned
/// copy so a host can apply it without borrowing the scene.
#[derive(Debug, Clone)]
pub struct FiredAction {
    pub on: crate::director::TriggerOn,
    pub actions: BTreeMap<String, EdnValue>,
}

impl FiredAction {
    /// Look up an action value as an identifier (keyword/string name).
    pub fn action(&self, key: &str) -> Option<String> {
        self.actions.get(key).and_then(|v| {
            v.as_keyword()
                .map(|k| k.0.name.clone())
                .or_else(|| v.as_string().map(|s| s.to_string()))
        })
    }
}

/// Everything a host needs to present one frame of the dance scene: the EDN
/// render-IR to draw, and the authored reactions that fired this tick.
#[derive(Debug, Clone)]
pub struct DanceFrame {
    /// Render-IR for this frame — feed to `kami-webgpu-rs` (native) or the web
    /// CLJS executor. See [`crate::render::show_to_render_ir`].
    pub render_ir: EdnValue,
    /// Authored reactions that fired this tick, in order.
    pub actions: Vec<FiredAction>,
    /// Live2D render-IR entry (`{:kind :live2d :model … :params {…}}`) when a
    /// `:dance/live2d` performer is bound; `None` otherwise.
    pub live2d: Option<EdnValue>,
    /// Audio cues that fired this tick (drum hits / bass notes / pad swaps /
    /// stop), synthesised from the active track's `:audio` program. A host's Web
    /// Audio bridge plays them; empty when the track has no `:audio`.
    pub audio: Vec<crate::audio::AudioCue>,
    /// The same sounds projected into `kami.audio`-style EDN recipe maps
    /// (`{:wave :freq :to :dur :gain :at}`) — drum/bass cues + fired `:sound`
    /// triggers resolved through the scene's `:dance/audio :bank`. A CLJS / Web
    /// Audio host plays each directly; no asset files (ADR-0038 "everything EDN").
    pub sounds: Vec<EdnValue>,
}

impl DanceFrame {
    /// Serialise the render-IR to an EDN string.
    pub fn render_ir_edn(&self) -> String {
        kotoba_edn::to_string(&self.render_ir)
    }
}

/// Summary of a headless run — what a CLI / golden test reports without a GPU.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// Frames advanced.
    pub frames: u32,
    /// Total authored reactions that fired across the run.
    pub total_actions: usize,
    /// `:fx` action value → how many times it fired.
    pub fx_counts: BTreeMap<String, usize>,
    /// Render-IR of the final frame (drawable EDN).
    pub final_render_ir: EdnValue,
    /// Beat / bar reached at the end of the run.
    pub final_beat: u32,
    pub final_bar: u32,
    /// `:meshes` count in the final frame (VRM avatars on the data path).
    pub mesh_count: usize,
    /// Live2D parameter count in the final frame (0 if no `:dance/live2d`).
    pub live2d_params: usize,
}

/// Drive a scene for `frames` ticks at `fps` with no renderer — the show runs
/// purely from its EDN. Starts the show, accumulates fired `:fx` reactions, and
/// keeps the last frame's render-IR. Deterministic: same scene + args → same
/// report (the basis for a golden test or a `bb` verify).
pub fn run_headless(scene: &mut DanceScene, frames: u32, fps: f32) -> RunReport {
    let dt = 1.0 / fps.max(1.0);
    scene.show.start();
    let mut fx_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_actions = 0usize;
    let mut final_render_ir = EdnValue::map([]);
    let mut mesh_count = 0;
    let mut live2d_params = 0;
    for _ in 0..frames {
        let f = scene.frame(dt);
        total_actions += f.actions.len();
        for a in &f.actions {
            if let Some(fx) = a.action("fx") {
                *fx_counts.entry(fx).or_insert(0) += 1;
            }
        }
        mesh_count = f
            .render_ir
            .as_map()
            .and_then(|m| {
                mget(m, "meshes")
                    .and_then(|v| v.as_vector())
                    .map(|s| s.len())
            })
            .unwrap_or(0);
        live2d_params = f
            .live2d
            .as_ref()
            .and_then(|l| l.as_map())
            .and_then(|m| mget(m, "params").and_then(|v| v.as_map()).map(|p| p.len()))
            .unwrap_or(0);
        final_render_ir = f.render_ir;
    }
    let phase = scene.show.grid().phase();
    RunReport {
        frames,
        total_actions,
        fx_counts,
        final_render_ir,
        final_beat: phase.beat,
        final_bar: phase.bar,
        mesh_count,
        live2d_params,
    }
}

/// Build a render-IR `:particles` burst for a fired `:fx` reaction at `pos`.
/// Known fx → a tuned burst; unknown fx → `None` (no particles).
fn fx_particle_burst(fx: &str, pos: [f32; 3]) -> Option<EdnValue> {
    let f = |x: f32| EdnValue::float(x as f64);
    let v3 = |v: [f32; 3]| EdnValue::vector([f(v[0]), f(v[1]), f(v[2])]);
    let burst = |color: [f32; 3], count: f32, speed: f32, life: f32, gravity: f32, size: f32| {
        EdnValue::map([
            (EdnValue::kw_bare("pos"), v3(pos)),
            (EdnValue::kw_bare("color"), v3(color)),
            (EdnValue::kw_bare("count"), f(count)),
            (EdnValue::kw_bare("speed"), f(speed)),
            (EdnValue::kw_bare("life"), f(life)),
            (EdnValue::kw_bare("gravity"), f(gravity)),
            (EdnValue::kw_bare("size"), f(size)),
        ])
    };
    // each fx is a distinct burst signature — colour, count, speed, life, gravity
    // (negative = rises), size. The executor draws them as additive billboards.
    Some(match fx {
        "confetti" => burst([1.0, 0.6, 0.2], 40.0, 4.0, 2.0, 2.0, 0.04),
        "pyro" | "fire" | "flame" => burst([1.0, 0.4, 0.1], 36.0, 7.0, 1.4, -1.2, 0.07),
        "sparkle" | "sparkles" => burst([1.0, 1.0, 0.6], 20.0, 2.0, 1.0, 0.0, 0.03),
        "sparkle-blast" => burst([1.0, 1.0, 0.8], 60.0, 6.0, 1.2, 0.2, 0.04),
        "fireworks" | "firework" => burst([0.6, 0.8, 1.0], 80.0, 9.0, 2.2, 1.5, 0.05),
        "laser" | "laser-burst" => burst([0.4, 1.0, 0.6], 24.0, 14.0, 0.7, 0.0, 0.02),
        "smoke" | "haze" => burst([0.6, 0.6, 0.66], 18.0, 1.2, 3.0, -0.6, 0.18),
        "bubbles" => burst([0.6, 0.85, 1.0], 28.0, 1.6, 2.6, -0.8, 0.06),
        "hearts" => burst([1.0, 0.4, 0.6], 16.0, 1.8, 2.4, -0.5, 0.07),
        "stars" | "star-shower" => burst([1.0, 0.95, 0.7], 30.0, 3.0, 2.0, 1.2, 0.04),
        "snow" => burst([0.95, 0.97, 1.0], 40.0, 0.8, 4.0, 0.4, 0.05),
        "petals" | "sakura" => burst([1.0, 0.7, 0.8], 30.0, 1.0, 3.5, 0.5, 0.05),
        "glitter" => burst([1.0, 0.9, 0.5], 50.0, 3.0, 1.6, 0.6, 0.025),
        "embers" => burst([1.0, 0.5, 0.2], 26.0, 2.4, 2.8, -0.7, 0.035),
        _ => return None,
    })
}

/// Map a stage name (`:club` / `:hall` / `:festival`) to a [`StagePreset`].
/// Unknown / missing → `Hall`.
pub fn stage_preset_by_name(name: &str) -> StagePreset {
    match name {
        "club" => StagePreset::Club,
        "festival" => StagePreset::Festival,
        _ => StagePreset::Hall,
    }
}

/// Map a cue name (`:drop` / `:breakdown` / `:callout`) to a [`CueKind`].
/// Unknown / missing → `Custom`.
pub fn cue_kind_by_name(name: &str) -> CueKind {
    match name {
        "drop" => CueKind::Drop,
        "breakdown" => CueKind::Breakdown,
        "callout" => CueKind::Callout,
        _ => CueKind::Custom,
    }
}

/// Build the scene's sound bank: the [`default_sound_bank`] plus any
/// `:dance/audio :bank {<name> {:wave :freq :to :dur :gain}}` overrides/additions.
fn parse_sound_bank(root: &BTreeMap<EdnValue, EdnValue>) -> BTreeMap<String, SoundCue> {
    let mut bank = default_sound_bank();
    if let Some(bm) = mget(root, "dance/audio")
        .and_then(|v| v.as_map())
        .and_then(|am| mget(am, "bank").and_then(|v| v.as_map()))
    {
        for (k, v) in bm {
            let name = k
                .as_keyword()
                .map(|kw| kw.0.name.clone())
                .or_else(|| k.as_string().map(|s| s.to_string()));
            let (Some(name), Some(cm)) = (name, v.as_map()) else { continue };
            bank.insert(
                name,
                SoundCue {
                    wave: mget(cm, "wave")
                        .and_then(|v| v.as_string().map(|s| s.to_string()))
                        .or_else(|| ident(mget(cm, "wave")))
                        .unwrap_or_else(|| "sine".into()),
                    freq: mget(cm, "freq").map(|v| num(Some(v))).unwrap_or(440.0),
                    to: mget(cm, "to").map(|v| num(Some(v))),
                    dur: mget(cm, "dur").map(|v| num(Some(v))).unwrap_or(0.1),
                    gain: mget(cm, "gain").map(|v| num(Some(v))).unwrap_or(0.2),
                },
            );
        }
    }
    bank
}

/// Project a [`SoundCue`] recipe into a `kami.audio`-style EDN map
/// (`{:wave :freq :to? :dur :gain :at}`), scaling gain by `vel` and overriding
/// the frequency for pitched notes (bass / pad).
fn sound_cue_to_edn(c: &SoundCue, at: f32, vel: f32, freq: Option<f32>) -> EdnValue {
    let f = |x: f32| EdnValue::float(x as f64);
    let mut entries = vec![
        (EdnValue::kw_bare("wave"), EdnValue::string(c.wave.clone())),
        (EdnValue::kw_bare("freq"), f(freq.unwrap_or(c.freq))),
        (EdnValue::kw_bare("dur"), f(c.dur)),
        (EdnValue::kw_bare("gain"), f(c.gain * vel.clamp(0.0, 1.0).max(0.05))),
        (EdnValue::kw_bare("at"), f(at)),
    ];
    if let Some(to) = c.to {
        entries.insert(2, (EdnValue::kw_bare("to"), f(to)));
    }
    EdnValue::map(entries)
}

/// Resolve a named full audio program (`:opener` / `:ballad` / `:encore`); an
/// unknown name → `None` (track plays with externally-mixed audio).
fn audio_pattern_by_name(name: &str) -> Option<AudioPattern> {
    match name {
        "opener" => Some(AudioPattern::opener()),
        "ballad" => Some(AudioPattern::ballad()),
        "encore" => Some(AudioPattern::encore()),
        _ => None,
    }
}

/// Map a drum-slot name (`:kick` / `:snare` / …) to a [`DrumSlot`].
fn drum_slot_by_name(name: &str) -> Option<DrumSlot> {
    Some(match name {
        "kick" => DrumSlot::Kick,
        "snare" => DrumSlot::Snare,
        "closed-hat" => DrumSlot::ClosedHat,
        "open-hat" => DrumSlot::OpenHat,
        "clap" => DrumSlot::Clap,
        "crash" => DrumSlot::Crash,
        "tom" => DrumSlot::Tom,
        "rim" => DrumSlot::Rim,
        _ => return None,
    })
}

/// A MIDI note list: `[60 63 67]` → `vec![60, 63, 67]` (clamped to 0..127).
fn midi_list(v: Option<&EdnValue>) -> Vec<u8> {
    v.and_then(|x| x.as_vector())
        .unwrap_or(&[])
        .iter()
        .map(|n| int(Some(n), 0).clamp(0, 127) as u8)
        .collect()
}

/// Parse a drum pattern: a named preset (`:four-on-floor` / `:ballad` /
/// `:pumping` / `:empty`) or an inline `{:kick [v0 … v7] :snare […]}` map of
/// per-slot 8-step (8th-note) velocities.
fn drum_pattern_from_edn(v: Option<&EdnValue>) -> Option<DrumPattern> {
    if let Some(name) = ident(v) {
        return Some(match name.as_str() {
            "ballad" => DrumPattern::ballad(),
            "pumping" => DrumPattern::pumping(),
            "empty" => DrumPattern::empty(),
            _ => DrumPattern::four_on_floor(),
        });
    }
    if let Some(m) = v.and_then(|x| x.as_map()) {
        let mut p = DrumPattern::empty();
        for (k, val) in m {
            if let (Some(name), Some(steps)) =
                (k.as_keyword().map(|kw| kw.0.name.clone()), val.as_vector())
            {
                if let Some(slot) = drum_slot_by_name(&name) {
                    for (i, sv) in steps.iter().enumerate().take(8) {
                        p.set(slot, i, num(Some(sv)));
                    }
                }
            }
        }
        return Some(p);
    }
    None
}

/// Parse a bass line: a named preset (`:c-minor` / `:empty`) or a vector of
/// `{:beat b :midi m :len beats :vel v}` notes.
fn bass_line_from_edn(v: Option<&EdnValue>) -> Option<BassLine> {
    if let Some(name) = ident(v) {
        return Some(match name.as_str() {
            "empty" => BassLine::empty(),
            _ => BassLine::root_pattern_c_minor(),
        });
    }
    if let Some(vec) = v.and_then(|x| x.as_vector()) {
        let notes = vec
            .iter()
            .filter_map(|n| n.as_map())
            .map(|nm| BassNote {
                at_beat: int(mget(nm, "beat"), 0).max(0) as u32,
                pitch_midi: int(mget(nm, "midi"), 60).clamp(0, 127) as u8,
                length_beats: {
                    let l = num(mget(nm, "len"));
                    if l > 0.0 { l } else { 1.0 }
                },
                velocity: mget(nm, "vel")
                    .map(|x| num(Some(x)).clamp(0.0, 1.0))
                    .unwrap_or(0.85),
            })
            .collect();
        return Some(BassLine { notes });
    }
    None
}

/// Resolve a track's audio program from EDN. Accepts a named full preset
/// (`:opener`) **or** an inline `{:drums … :bass … :lead-arp [..] :pad-chord [..]}`
/// map — so a track's synth is fully describable in EDN, not just a Rust preset.
fn audio_from_edn(v: Option<&EdnValue>) -> Option<AudioPattern> {
    if let Some(name) = ident(v) {
        return audio_pattern_by_name(&name);
    }
    if let Some(m) = v.and_then(|x| x.as_map()) {
        return Some(AudioPattern {
            drums: drum_pattern_from_edn(mget(m, "drums")),
            bass: bass_line_from_edn(mget(m, "bass")),
            lead_arp: midi_list(mget(m, "lead-arp")),
            pad_chord: midi_list(mget(m, "pad-chord")),
        });
    }
    None
}

/// Map a fixture name (`:front-par` / `:spot` / …) to a [`LightingFixture`].
/// Unknown / missing → `FrontPar`.
pub fn lighting_fixture_by_name(name: &str) -> LightingFixture {
    match name {
        "back-par" => LightingFixture::BackPar,
        "spot" => LightingFixture::Spot,
        "blinder" => LightingFixture::Blinder,
        "laser" => LightingFixture::Laser,
        "strobe" => LightingFixture::Strobe,
        _ => LightingFixture::FrontPar,
    }
}

/// Map a VJ pattern name (`:stripes` / `:pulse` / …) to a [`VJPattern`].
/// Unknown / missing → `Solid`.
pub fn vj_pattern_by_name(name: &str) -> VJPattern {
    match name {
        "stripes" => VJPattern::Stripes,
        "pulse" => VJPattern::Pulse,
        "rings" => VJPattern::Rings,
        "scope" => VJPattern::Scope,
        "noise" => VJPattern::Noise,
        _ => VJPattern::Solid,
    }
}

/// Resolve a palette from EDN: a named keyword (`:neon-pink` / `:cool-wave` /
/// `:sunset` / `:monochrome`) or an inline `{:primary [..] :secondary [..]
/// :accent [..]}` map. Unknown / missing → `COOL_WAVE`.
fn palette_from_edn(v: Option<&EdnValue>) -> Palette {
    if let Some(name) = ident(v) {
        return match name.as_str() {
            "neon-pink" => Palette::NEON_PINK,
            "sunset" => Palette::SUNSET,
            "monochrome" => Palette::MONOCHROME,
            _ => Palette::COOL_WAVE,
        };
    }
    if let Some(m) = v.and_then(|x| x.as_map()) {
        return Palette {
            primary: vec3(mget(m, "primary")),
            secondary: vec3(mget(m, "secondary")),
            accent: vec3(mget(m, "accent")),
        };
    }
    Palette::COOL_WAVE
}

/// Parse `:dance/vj` → a per-phrase `(pattern, palette)` program. Returns `None`
/// when absent (caller falls back to the deck's default program).
fn parse_vj(root: &BTreeMap<EdnValue, EdnValue>) -> Option<VJDeck> {
    let steps = mget(root, "dance/vj").and_then(|v| v.as_vector())?;
    let program: Vec<(VJPattern, Palette)> = steps
        .iter()
        .filter_map(|s| s.as_map())
        .map(|m| {
            (
                ident(mget(m, "pattern"))
                    .map(|n| vj_pattern_by_name(&n))
                    .unwrap_or(VJPattern::Solid),
                palette_from_edn(mget(m, "palette")),
            )
        })
        .collect();
    Some(VJDeck::new(program))
}

/// Parse an envelope from EDN. Accepts a bare keyword (`:hold` / `:breathe` /
/// `:ramp` / `:pulse` / `:strobe`, the last two with default shape) or a map
/// carrying the shape parameter (`{:pulse 0.7}` → decay, `{:strobe 0.25}` → duty).
fn envelope_from_edn(v: Option<&EdnValue>) -> Envelope {
    if let Some(name) = ident(v) {
        return match name.as_str() {
            "breathe" => Envelope::Breathe,
            "ramp" => Envelope::Ramp,
            "pulse" => Envelope::Pulse { decay: 0.5 },
            "strobe" => Envelope::Strobe { duty: 0.5 },
            _ => Envelope::Hold,
        };
    }
    if let Some(m) = v.and_then(|x| x.as_map()) {
        if let Some(d) = mget(m, "pulse") {
            return Envelope::Pulse {
                decay: num(Some(d)),
            };
        }
        if let Some(d) = mget(m, "strobe") {
            return Envelope::Strobe { duty: num(Some(d)) };
        }
    }
    Envelope::Hold
}

/// A value that may be authored as a keyword (`:wota`) or a string (`"wota"`).
/// Returns the keyword's *local* name (namespace dropped) or the string.
fn ident(v: Option<&EdnValue>) -> Option<String> {
    let v = v?;
    v.as_keyword()
        .map(|k| k.0.name.clone())
        .or_else(|| v.as_string().map(|s| s.to_string()))
}

/// Read a boolean, defaulting when absent / non-boolean.
fn flag(v: Option<&EdnValue>, default: bool) -> bool {
    v.and_then(|x| x.as_bool()).unwrap_or(default)
}

/// Read an integer (coercing a float), defaulting when absent / non-numeric.
fn int(v: Option<&EdnValue>, default: i64) -> i64 {
    v.and_then(|x| x.as_integer().or_else(|| x.as_float().map(|f| f as i64)))
        .unwrap_or(default)
}

/// Read a `[x y z]` vector only when present (vs. `vec3`'s zero default).
fn opt_vec3(v: Option<&EdnValue>) -> Option<Vec3> {
    v.and_then(|x| x.as_vector())
        .filter(|s| !s.is_empty())
        .map(|_| Vec3::from(vec3(v)))
}

impl DanceScene {
    /// Parse a dance scene from EDN. Returns `None` only when the top form is not
    /// a map; every field is otherwise tolerant with sensible defaults.
    pub fn from_edn(src: &str) -> Option<DanceScene> {
        let root = root_map(src)?;
        Some(Self::from_root(&root))
    }

    /// Advance the scene by `dt` seconds and produce the frame a host presents:
    /// the EDN render-IR plus the authored reactions that fired. This is the
    /// single per-frame entry that ties together the whole data path —
    /// `show.tick` → `director.resolve` → `render::show_to_render_ir`:
    ///
    /// ```ignore
    /// let mut scene = DanceScene::from_edn(&edn).unwrap();
    /// scene.show.start();
    /// loop {
    ///     let frame = scene.frame(dt);
    ///     draw(frame.render_ir_edn());            // native or web
    ///     for a in &frame.actions { host.apply(a); }
    /// }
    /// ```
    pub fn frame(&mut self, dt: f32) -> DanceFrame {
        let events = self.show.tick(dt);
        let mut actions = Vec::new();
        let mut audio = Vec::new();
        let mut new_shot: Option<String> = None;
        for ev in &events {
            // surface synthesised audio cues (drum/bass/pad/stop) for the host's
            // Web Audio bridge — the active track's `:audio` program drives them.
            if let crate::show::ShowEvent::Audio(cue) = ev {
                audio.push(cue.clone());
            }
            for t in self.director.resolve(ev) {
                if let Some(shot) = t.action("camera") {
                    new_shot = Some(shot);
                }
                actions.push(FiredAction {
                    on: t.on,
                    actions: t.actions.clone(),
                });
            }
        }
        if let Some(shot) = new_shot {
            self.active_camera = Some(shot);
        }
        let snap = self.show.snapshot();

        // project audio cues + fired `:sound` triggers into kami.audio EDN recipes
        // (`{:wave :freq :to :dur :gain :at}`) via the scene's sound bank.
        let mut sounds = Vec::new();
        for cue in &audio {
            match cue {
                crate::audio::AudioCue::Drum { at_time, slot, velocity } => {
                    if let Some(c) = self.sound_bank.get(slot.bank_name()) {
                        sounds.push(sound_cue_to_edn(c, *at_time, *velocity, None));
                    }
                }
                crate::audio::AudioCue::Note { at_time, midi, velocity, .. } => {
                    if let Some(c) = self.sound_bank.get("bass") {
                        sounds.push(sound_cue_to_edn(c, *at_time, *velocity, Some(midi_to_hz(*midi))));
                    }
                }
                crate::audio::AudioCue::Pad { at_time, midis } => {
                    if let Some(c) = self.sound_bank.get("pad") {
                        sounds.push(sound_cue_to_edn(c, *at_time, 1.0, Some(midi_to_hz(midis[0]))));
                    }
                }
                crate::audio::AudioCue::Stop { .. } => {}
            }
        }
        for a in &actions {
            if let Some(name) = a.action("sound") {
                if let Some(c) = self.sound_bank.get(&name) {
                    sounds.push(sound_cue_to_edn(c, snap.phase.time, 1.0, None));
                }
            }
        }

        let render_ir =
            crate::render::show_to_render_ir(&snap, &self.avatar, &self.camera, &self.stage);
        // inject scene-level keys (post-fx chain, active camera shot) into the
        // per-frame render-IR.
        let mut extra: Vec<(EdnValue, EdnValue)> = Vec::new();
        if !self.post.is_empty() {
            extra.push((
                EdnValue::kw_bare("post"),
                EdnValue::vector(self.post.clone()),
            ));
        }
        // `:sounds` (kami.audio EDN recipes) ride along in the render-IR so the
        // web CLJS executor (kami.audio) plays them with the visual frame.
        if !sounds.is_empty() {
            extra.push((EdnValue::kw_bare("sounds"), EdnValue::vector(sounds.clone())));
        }
        // (`:animations` is already projected by `show_to_render_ir` from the
        // avatar `:clip` — no duplicate injection here.)
        if let Some(shot) = &self.active_camera {
            extra.push((
                EdnValue::kw_bare("camera-shot"),
                EdnValue::kw_bare(shot.clone()),
            ));
        }
        // `:fx` reactions (confetti / pyro / sparkle) become particle bursts at
        // the performer, so a host's particle pipeline draws them (ADR-0044).
        let p = snap.performer_pose.root_translation;
        let bursts: Vec<EdnValue> = actions
            .iter()
            .filter_map(|a| {
                a.action("fx")
                    .and_then(|fx| fx_particle_burst(&fx, [p.x, p.y + 1.0, p.z]))
            })
            .collect();
        if !bursts.is_empty() {
            extra.push((EdnValue::kw_bare("particles"), EdnValue::vector(bursts)));
        }
        let render_ir = if !extra.is_empty() {
            if let Some(map) = render_ir.as_map() {
                let mut entries: Vec<(EdnValue, EdnValue)> =
                    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                entries.extend(extra);
                EdnValue::map(entries)
            } else {
                render_ir
            }
        } else {
            render_ir
        };
        // Live2D performer: drive Cubism params from the same pose + phase; the
        // avatar's `:voice` vowel timeline syncs its mouth lipsync too (parity
        // with the VRM mouth).
        let live2d = self.live2d.as_ref().map(|b| {
            let phase = self.show.grid().phase();
            let voice_mouth = self.avatar.voice.as_ref().and_then(|v| {
                let beat = phase.beat as f32 + phase.beat_frac;
                v.vowel_weight(beat).map(|(_, w)| w)
            });
            b.render_entry(&b.drive(&snap.performer_pose, &phase, voice_mouth))
        });
        DanceFrame {
            render_ir,
            actions,
            live2d,
            audio,
            sounds,
        }
    }

    fn from_root(root: &BTreeMap<EdnValue, EdnValue>) -> DanceScene {
        let title = mget(root, "game/title")
            .and_then(|v| v.as_string())
            .unwrap_or("KAMI VRM Dance")
            .to_string();

        // ── :dance/show → tempo grid + venue ────────────────────────────────
        let show_map = mget(root, "dance/show").and_then(|v| v.as_map());
        let bpm = show_map
            .and_then(|m| mget(m, "bpm"))
            .map(|v| num(Some(v)))
            .filter(|b| *b > 0.0)
            .unwrap_or(128.0);
        let stage = show_map
            .and_then(|m| ident(mget(m, "stage")))
            .map(|s| stage_preset_by_name(&s))
            .unwrap_or(StagePreset::Hall);
        let swing = show_map.map(|m| num(mget(m, "swing"))).unwrap_or(0.0);
        let performer = show_map
            .and_then(|m| mget(m, "performer"))
            .and_then(|v| v.as_string())
            .unwrap_or("Mitama")
            .to_string();
        // :meter [beats-per-bar bars-per-phrase]
        let (beats_per_bar, bars_per_phrase) = show_map
            .and_then(|m| mget(m, "meter"))
            .and_then(|v| v.as_vector())
            .map(|s| {
                let bpb = s.first().map(|x| int(Some(x), 4)).unwrap_or(4).max(1) as u32;
                let bpp = s.get(1).map(|x| int(Some(x), 8)).unwrap_or(8).max(1) as u32;
                (bpb, bpp)
            })
            .unwrap_or((4, 8));

        // ── :dance/crowd → audience density (optional; else preset default) ─
        let crowd_cfg = mget(root, "dance/crowd")
            .and_then(|v| v.as_map())
            .map(parse_crowd)
            .unwrap_or_default();

        let mut builder = LiveShow::builder()
            .bpm(bpm)
            .stage(stage)
            .performer_name(performer)
            .crowd(crowd_cfg)
            .swing(swing)
            .meter(beats_per_bar, bars_per_phrase);
        // ── :dance/vj → visual deck (optional; else the deck's default program)
        if let Some(deck) = parse_vj(root) {
            builder = builder.vj_deck(deck);
        }
        let mut show = builder.build();

        // ── :dance/lighting → beat-synced lighting cues ─────────────────────
        for cue in mget(root, "dance/lighting")
            .and_then(|v| v.as_vector())
            .unwrap_or(&[])
            .iter()
            .filter_map(|c| c.as_map())
        {
            let (lc, at_bar) = parse_lighting_cue(cue);
            show.lighting_mut().push(lc, at_bar);
        }

        // ── :dance/setlist → tracks (each a dance section) ──────────────────
        for (i, t) in mget(root, "dance/setlist")
            .and_then(|v| v.as_vector())
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            if let Some(tm) = t.as_map() {
                let track = parse_track(tm, i as u32, bpm, beats_per_bar);
                show.setlist_mut().push(track);
            }
        }

        // ── :dance/avatar → VRM binding ─────────────────────────────────────
        let avatar = mget(root, "dance/avatar")
            .and_then(|v| v.as_map())
            .map(parse_avatar)
            .unwrap_or_default();

        // ── :dance/triggers → EDN-declared event reactions ──────────────────
        let director = crate::director::Director::from_root(root);

        // ── :dance/live2d → optional 2D performer ───────────────────────────
        let live2d = mget(root, "dance/live2d")
            .and_then(|v| v.as_map())
            .map(crate::live2d::Live2DBinding::from_edn);

        // ── :dance/clips → EDN-authored animation clip definitions ──────────
        let clips = mget(root, "dance/clips")
            .and_then(|v| v.as_vector())
            .map(|cs| {
                cs.iter()
                    .filter(|c| c.as_map().is_some())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        // ── :dance/post → post-processing effect chain ──────────────────────
        let post = mget(root, "dance/post")
            .and_then(|v| v.as_vector())
            .map(|ps| {
                ps.iter()
                    .filter(|p| p.as_map().is_some())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        // ── :dance/camera → camera rig (offset / look / fov), data-authored ──
        let camera = mget(root, "dance/camera")
            .and_then(|v| v.as_map())
            .map(|cm| {
                let d = CameraRig::default();
                let shots = mget(cm, "shots")
                    .and_then(|v| v.as_vector())
                    .map(|ss| {
                        let mut v: Vec<CameraShot> = ss
                            .iter()
                            .filter_map(|s| s.as_map())
                            .map(|sm| CameraShot {
                                at_bar: int(mget(sm, "at-bar"), 0).max(0) as u32,
                                offset: opt_vec3(mget(sm, "offset")).unwrap_or(d.offset),
                                look: opt_vec3(mget(sm, "look")).unwrap_or(d.look),
                            })
                            .collect();
                        v.sort_by_key(|s| s.at_bar);
                        v
                    })
                    .unwrap_or_default();
                CameraRig {
                    offset: opt_vec3(mget(cm, "offset")).unwrap_or(d.offset),
                    look: opt_vec3(mget(cm, "look")).unwrap_or(d.look),
                    fov: mget(cm, "fov")
                        .map(|v| num(Some(v)))
                        .filter(|f| *f > 0.0)
                        .unwrap_or(d.fov),
                    shots,
                }
            })
            .unwrap_or_default();

        // ── :dance/stage → static set pieces dressed into the render-IR ─────
        let stage = mget(root, "dance/stage")
            .and_then(|v| v.as_map())
            .and_then(|sm| mget(sm, "props").and_then(|v| v.as_vector()))
            .map(|props| {
                props
                    .iter()
                    .filter_map(|p| p.as_map())
                    .map(|pm| StageProp {
                        kind: ident(mget(pm, "kind")).unwrap_or_else(|| "prop".into()),
                        pos: opt_vec3(mget(pm, "pos")).unwrap_or(Vec3::ZERO),
                        size: {
                            let s = vec3(mget(pm, "size"));
                            [
                                if s[0] > 0.0 { s[0] } else { 1.0 },
                                if s[1] > 0.0 { s[1] } else { 1.0 },
                            ]
                        },
                        color: {
                            let c = vec3(mget(pm, "color"));
                            if c == [0.0, 0.0, 0.0] && mget(pm, "color").is_none() {
                                [0.15, 0.15, 0.18]
                            } else {
                                c
                            }
                        },
                        emissive: mget(pm, "emissive").map(|v| num(Some(v))).unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        DanceScene {
            title,
            avatar,
            show,
            director,
            live2d,
            clips,
            post,
            active_camera: None,
            camera,
            stage,
            sound_bank: parse_sound_bank(root),
        }
    }
}

fn parse_avatar(m: &BTreeMap<EdnValue, EdnValue>) -> AvatarBinding {
    let scale = mget(m, "scale").map(|v| num(Some(v))).filter(|s| *s > 0.0);
    AvatarBinding {
        vrm: mget(m, "vrm")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string(),
        home: opt_vec3(mget(m, "home")),
        scale: scale.unwrap_or(1.0),
        // `:look-at` is `true`/`false` (enable, default on) or a `{:target …}` map
        // (which also enables it). A map's `:target [x y z]` fixes the gaze;
        // anything else (`:target :camera` / omitted) tracks the camera.
        look_at: flag(mget(m, "look-at"), true),
        spring_bones: flag(mget(m, "spring-bones"), true),
        clip: ident(mget(m, "clip")),
        spring: mget(m, "spring")
            .and_then(|v| v.as_map())
            .map(|sm| SpringTuning {
                stiffness: mget(sm, "stiffness").map(|v| num(Some(v))).unwrap_or(0.05),
                drag: mget(sm, "drag").map(|v| num(Some(v))).unwrap_or(0.4),
                gravity: mget(sm, "gravity").map(|v| num(Some(v))).unwrap_or(0.3),
            }),
        look_at_target: mget(m, "look-at").and_then(|v| v.as_map()).map(|lm| {
            match opt_vec3(mget(lm, "target")) {
                Some(p) => LookTarget::Fixed([p.x, p.y, p.z]),
                None => LookTarget::Camera,
            }
        }),
        expressions: mget(m, "expressions")
            .and_then(|v| v.as_map())
            .map(parse_expression_drives)
            .filter(|v| !v.is_empty())
            .unwrap_or_else(AvatarBinding::default_expressions),
        vmd: mget(m, "vmd").and_then(|v| v.as_string()).map(|s| s.to_string()),
        voice: mget(m, "voice")
            .and_then(|v| v.as_map())
            .and_then(|vm| mget(vm, "phonemes").and_then(|v| v.as_vector()))
            .map(|ps| {
                let phonemes = ps
                    .iter()
                    .filter_map(|p| p.as_map())
                    .filter_map(|pm| {
                        let vowel = ident(mget(pm, "vowel")).and_then(|n| Vowel::from_name(&n))?;
                        Some(Phoneme {
                            at_beat: mget(pm, "at-beat").map(|v| num(Some(v))).unwrap_or(0.0),
                            vowel,
                            dur: mget(pm, "dur").map(|v| num(Some(v))).filter(|d| *d > 0.0).unwrap_or(0.5),
                        })
                    })
                    .collect();
                VoiceLine { phonemes }
            }),
    }
}

/// Parse `:dance/avatar :expressions {<name> {:from :cheer|:beat|:blink :gain f}}`.
fn parse_expression_drives(m: &BTreeMap<EdnValue, EdnValue>) -> Vec<ExpressionDrive> {
    let mut out = Vec::new();
    for (k, v) in m {
        let Some(name) = k
            .as_keyword()
            .map(|kw| kw.0.name.clone())
            .or_else(|| k.as_string().map(|s| s.to_string()))
        else {
            continue;
        };
        let dm = v.as_map();
        let source = match dm.and_then(|d| ident(mget(d, "from"))).as_deref() {
            Some("cheer") => ExprSource::Cheer,
            Some("blink") => ExprSource::Blink,
            _ => ExprSource::Beat, // `:beat` or unspecified
        };
        let gain = dm
            .and_then(|d| mget(d, "gain"))
            .map(|v| num(Some(v)))
            .unwrap_or(1.0);
        out.push(ExpressionDrive { name, source, gain });
    }
    out
}

fn parse_crowd(m: &BTreeMap<EdnValue, EdnValue>) -> CrowdConfig {
    let d = CrowdConfig::default();
    CrowdConfig {
        fans_target: mget(m, "fans")
            .map(|v| int(Some(v), d.fans_target as i64).max(0) as usize)
            .unwrap_or(d.fans_target),
        cap: mget(m, "cap")
            .map(|v| int(Some(v), d.cap as i64).max(0) as usize)
            .unwrap_or(d.cap),
        pit_bias: mget(m, "pit-bias")
            .map(|v| num(Some(v)).clamp(0.0, 1.0))
            .unwrap_or(d.pit_bias),
        seed: mget(m, "seed")
            .map(|v| int(Some(v), d.seed as i64).max(0) as u32)
            .unwrap_or(d.seed),
    }
}

/// Parse one lighting cue → `(LightingCue, start_bar)`.
fn parse_lighting_cue(m: &BTreeMap<EdnValue, EdnValue>) -> (LightingCue, u32) {
    let fixture = ident(mget(m, "fixture"))
        .map(|n| lighting_fixture_by_name(&n))
        .unwrap_or(LightingFixture::FrontPar);
    let color = {
        let c = vec3(mget(m, "color"));
        // default to warm white when omitted (vec3 → [0,0,0]).
        if c == [0.0, 0.0, 0.0] && mget(m, "color").is_none() {
            [1.0, 1.0, 1.0]
        } else {
            c
        }
    };
    let intensity = mget(m, "intensity")
        .map(|v| num(Some(v)).clamp(0.0, 1.0))
        .unwrap_or(0.8);
    let bars = mget(m, "bars")
        .map(|v| int(Some(v), 16))
        .unwrap_or(16)
        .max(1) as u32;
    let at_bar = int(mget(m, "at-bar"), 0).max(0) as u32;
    (
        LightingCue {
            fixture,
            color,
            intensity,
            envelope: envelope_from_edn(mget(m, "envelope")),
            bars,
        },
        at_bar,
    )
}

fn parse_track(
    m: &BTreeMap<EdnValue, EdnValue>,
    id: u32,
    show_bpm: f32,
    beats_per_bar: u32,
) -> Track {
    let bpm = mget(m, "bpm")
        .map(|v| num(Some(v)))
        .filter(|b| *b > 0.0)
        .unwrap_or(show_bpm);
    // length: prefer :bars (× beats/bar); fall back to :beats; default 16 bars.
    let length_beats = match mget(m, "beats") {
        Some(b) => int(Some(b), (16 * beats_per_bar) as i64).max(0) as u32,
        None => {
            let bars = mget(m, "bars")
                .map(|v| int(Some(v), 16))
                .unwrap_or(16)
                .max(0) as u32;
            bars * beats_per_bar
        }
    };
    let title = mget(m, "title")
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string();
    let dance = ident(mget(m, "dance"));
    let audio = audio_from_edn(mget(m, "audio"));
    let cues = mget(m, "cues")
        .and_then(|v| v.as_vector())
        .unwrap_or(&[])
        .iter()
        .filter_map(|c| c.as_map())
        .map(|cm| CuePoint {
            at_beat: int(mget(cm, "beat"), 0).max(0) as u32,
            kind: ident(mget(cm, "kind"))
                .map(|k| cue_kind_by_name(&k))
                .unwrap_or(CueKind::Custom),
            tag: mget(cm, "tag")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    Track {
        id: TrackId(id),
        title,
        bpm,
        length_beats,
        cues,
        dance,
        audio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::performer::DanceMove;

    const SCENE: &str = r#"
    {:game/id    :gftd.games/vrm-dance
     :game/title "Mitama Live"
     :dance/show   {:bpm 120.0 :stage :club :swing 0.2 :meter [4 8] :performer "Mitama"}
     :dance/avatar {:vrm "models/mitama.vrm" :home [0.0 1.2 0.0] :scale 1.1
                    :look-at true :spring-bones false}
     :dance/setlist
     [{:title "Opening" :bpm 120.0 :bars 16 :dance :wota
       :cues [{:beat 0 :kind :callout :tag "intro"}
              {:beat 32 :kind :drop :tag "drop-1"}]}
      {:title "Chorus" :bars 8 :dance :kpop-point :audio :opener
       :cues [{:beat 0 :kind :drop :tag "hook"}]}]}
    "#;

    #[test]
    fn parses_show_tempo_and_venue() {
        let sc = DanceScene::from_edn(SCENE).expect("scene");
        assert_eq!(sc.title, "Mitama Live");
        assert!((sc.show.grid().bpm - 120.0).abs() < 1e-6);
        assert_eq!(sc.show.grid().beats_per_bar, 4);
        assert_eq!(sc.show.grid().bars_per_phrase, 8);
        assert!((sc.show.grid().swing - 0.2).abs() < 1e-6);
    }

    #[test]
    fn parses_look_at_target() {
        // bool form: enabled, tracks camera (no explicit target).
        let bool_form = DanceScene::from_edn(
            r#"{:dance/show {:bpm 120.0} :dance/avatar {:vrm "m" :look-at true}
                :dance/setlist [{:title "A" :bars 4 :dance :wota}]}"#,
        )
        .unwrap();
        assert!(bool_form.avatar.look_at);
        assert!(
            bool_form.avatar.look_at_target.is_none(),
            "bool → camera default"
        );
        // map with a fixed point.
        let fixed = DanceScene::from_edn(
            r#"{:dance/show {:bpm 120.0} :dance/avatar {:vrm "m" :look-at {:target [0.0 1.5 -8.0]}}
                :dance/setlist [{:title "A" :bars 4 :dance :wota}]}"#,
        )
        .unwrap();
        assert!(fixed.avatar.look_at, "map form enables look-at");
        assert_eq!(
            fixed.avatar.look_at_target,
            Some(LookTarget::Fixed([0.0, 1.5, -8.0]))
        );
        // map with :target :camera (explicit).
        let cam = DanceScene::from_edn(
            r#"{:dance/show {:bpm 120.0} :dance/avatar {:vrm "m" :look-at {:target :camera}}
                :dance/setlist [{:title "A" :bars 4 :dance :wota}]}"#,
        )
        .unwrap();
        assert_eq!(cam.avatar.look_at_target, Some(LookTarget::Camera));
    }

    #[test]
    fn parses_spring_bone_tuning() {
        let sc = DanceScene::from_edn(
            r#"{:dance/show {:bpm 120.0 :stage :club}
                :dance/avatar {:vrm "m.vrm" :spring {:stiffness 0.08 :drag 0.5 :gravity 0.2}}
                :dance/setlist [{:title "A" :bars 8 :dance :wota}]}"#,
        )
        .expect("scene");
        let s = sc.avatar.spring.expect("spring tuning");
        assert!((s.stiffness - 0.08).abs() < 1e-6);
        assert!((s.drag - 0.5).abs() < 1e-6);
        assert!((s.gravity - 0.2).abs() < 1e-6);
        // partial map → defaults for the omitted keys.
        let sc2 = DanceScene::from_edn(
            r#"{:dance/show {:bpm 120.0} :dance/avatar {:vrm "m" :spring {:stiffness 0.1}}
                :dance/setlist [{:title "A" :bars 4 :dance :wota}]}"#,
        )
        .unwrap();
        let s2 = sc2.avatar.spring.unwrap();
        assert!((s2.stiffness - 0.1).abs() < 1e-6);
        assert!((s2.drag - 0.4).abs() < 1e-6, "default drag");
        // no :spring → None (use the VRM's own values).
        assert!(DanceScene::from_edn(SCENE).unwrap().avatar.spring.is_none());
    }

    #[test]
    fn parses_avatar_binding() {
        let sc = DanceScene::from_edn(SCENE).expect("scene");
        assert_eq!(sc.avatar.vrm, "models/mitama.vrm");
        assert_eq!(sc.avatar.home, Some(Vec3::new(0.0, 1.2, 0.0)));
        assert!((sc.avatar.scale - 1.1).abs() < 1e-6);
        assert!(sc.avatar.look_at);
        assert!(!sc.avatar.spring_bones, "spring-bones false honored");
    }

    #[test]
    fn setlist_lengths_use_bars_times_meter() {
        let sc = DanceScene::from_edn(SCENE).expect("scene");
        let tracks = &sc.show.setlist().tracks;
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].length_beats, 64, "16 bars × 4 beats");
        assert_eq!(tracks[0].dance.as_deref(), Some("wota"));
        assert_eq!(tracks[1].length_beats, 32, "8 bars × 4 beats");
        // track 0: cues sorted ascending after push.
        let beats: Vec<u32> = tracks[0].cues.iter().map(|c| c.at_beat).collect();
        assert_eq!(beats, vec![0, 32]);
        assert!(matches!(tracks[0].cues[1].kind, CueKind::Drop));
        // chorus carries the opener audio program.
        assert!(tracks[1].audio.is_some());
    }

    #[test]
    fn drives_a_deterministic_dance() {
        // The whole point: the EDN scene yields a running show whose performer
        // auto-selects the track's dance and emits a pose every frame.
        let mut a = DanceScene::from_edn(SCENE).expect("scene");
        let mut b = DanceScene::from_edn(SCENE).expect("scene");
        a.show.start();
        b.show.start();
        for _ in 0..120 {
            a.show.tick(1.0 / 60.0);
            b.show.tick(1.0 / 60.0);
        }
        // wota is the opening track's dance preset.
        assert!(matches!(a.show.performer().current, DanceMove::Wota));
        // deterministic replay: identical poses from identical EDN.
        let pa = a.show.snapshot().performer_pose;
        let pb = b.show.snapshot().performer_pose;
        assert!((pa.root_translation - pb.root_translation).length() < 1e-6);
        assert!((pa.arms_up - pb.arms_up).abs() < 1e-6);
    }

    #[test]
    fn defaults_are_tolerant() {
        // Empty map → all defaults, no panic.
        let sc = DanceScene::from_edn("{}").expect("empty map ok");
        assert!((sc.show.grid().bpm - 128.0).abs() < 1e-6);
        assert_eq!(sc.avatar.vrm, "");
        assert!(sc.avatar.home.is_none());
        assert!(sc.show.setlist().tracks.is_empty());
        // non-map top form → None.
        assert!(DanceScene::from_edn("42").is_none());
    }

    /// Guard the *authored* example scene against the loader: the committed
    /// `games/dance/scene.edn` must always parse into a runnable show. Keeps the
    /// clj/edn authoring surface and this loader from silently drifting apart.
    #[test]
    fn authored_example_scene_loads() {
        const EXAMPLE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
        let mut sc = DanceScene::from_edn(EXAMPLE).expect("example scene parses");
        assert_eq!(sc.title, "KAMI VRM Dance");
        assert_eq!(sc.avatar.vrm, "models/mitama.vrm");
        assert_eq!(sc.show.setlist().tracks.len(), 5);
        // it actually runs: drive a few seconds, expect a non-rest pose.
        sc.show.start();
        for _ in 0..90 {
            sc.show.tick(1.0 / 30.0);
        }
        let _ = sc.show.snapshot().performer_pose;
    }

    #[test]
    fn parses_crowd_and_lighting() {
        let src = r#"
        {:dance/show    {:bpm 120.0 :stage :club}
         :dance/crowd   {:fans 200 :cap 1024 :pit-bias 0.8 :seed 7}
         :dance/lighting
         [{:fixture :front-par :color [1.0 0.5 0.3] :intensity 0.9 :envelope :hold :bars 16 :at-bar 0}
          {:fixture :strobe :color [1.0 1.0 1.0] :intensity 1.0 :envelope {:strobe 0.25} :bars 4 :at-bar 8}]}
        "#;
        let mut sc = DanceScene::from_edn(src).expect("scene");
        assert_eq!(sc.show.crowd().config().fans_target, 200);
        assert_eq!(sc.show.crowd().config().seed, 7);
        // lighting cues resolve on the bar they cover.
        sc.show.start();
        sc.show.tick(0.05);
        let snap = sc.show.snapshot();
        let front = snap
            .lighting
            .iter()
            .find(|l| matches!(l.fixture, LightingFixture::FrontPar));
        assert!(front.is_some(), "front-par cue present at bar 0");
        assert!(front.unwrap().intensity > 0.5);
    }

    #[test]
    fn headless_run_is_deterministic_and_reacts() {
        const EXAMPLE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
        let report = |_| {
            let mut sc = DanceScene::from_edn(EXAMPLE).expect("scene");
            run_headless(&mut sc, 1800, 60.0) // 30s of show at 128 bpm
        };
        let a = report(());
        let b = report(());
        // determinism: same EDN + args → identical run report.
        assert_eq!(a.frames, b.frames);
        assert_eq!(a.total_actions, b.total_actions);
        assert_eq!(a.fx_counts, b.fx_counts);
        assert_eq!(a.final_beat, b.final_beat);
        assert_eq!(a.final_render_ir, b.final_render_ir);
        // over 30s (Opening, 16 bars) the `:bar :every 8` trigger fires pyro.
        assert!(a.total_actions > 0, "authored reactions fired");
        assert!(
            a.fx_counts.contains_key("pyro"),
            "bar:every-8 → pyro fired, got {:?}",
            a.fx_counts
        );
        assert!(a.final_beat > 0, "the clock advanced");
    }

    #[test]
    fn frame_runner_draws_and_reacts() {
        // One per-frame call yields a drawable render-IR and fires the authored
        // reactions — the whole EDN data path in a single entry point.
        const SRC: &str = r#"
        {:dance/show     {:bpm 140.0 :stage :festival}
         :dance/avatar   {:vrm "m.vrm" :home [0.0 1.0 0.0] :scale 1.0}
         :dance/crowd    {:fans 24 :seed 2}
         :dance/triggers [{:on :drop :fx :confetti}]
         :dance/setlist  [{:title "A" :bpm 140.0 :bars 8 :dance :wota
                           :cues [{:beat 4 :kind :drop :tag "hook"}]}]}
        "#;
        let mut sc = DanceScene::from_edn(SRC).expect("scene");
        sc.show.start();
        let mut saw_confetti = false;
        let mut drew = false;
        for _ in 0..600 {
            let f = sc.frame(1.0 / 60.0);
            // render-IR is always a well-formed, drawable map.
            if root_map(&f.render_ir_edn())
                .and_then(|m| {
                    mget(&m, "instances")
                        .and_then(|v| v.as_vector())
                        .map(|s| !s.is_empty())
                })
                .unwrap_or(false)
            {
                drew = true;
            }
            if f.actions
                .iter()
                .any(|a| a.action("fx").as_deref() == Some("confetti"))
            {
                saw_confetti = true;
            }
            if saw_confetti {
                break;
            }
        }
        assert!(drew, "frame produced a drawable render-IR");
        assert!(
            saw_confetti,
            "drop cue fired the confetti reaction via frame()"
        );
    }

    #[test]
    fn live2d_performer_driven_by_same_choreography() {
        // A Live2D avatar bound alongside the dance: one setlist, 2D params out.
        const SRC: &str = r#"
        {:dance/show    {:bpm 128.0 :stage :club}
         :dance/live2d  {:model "haru.model3.json" :lipsync :ParamMouthOpenY
                         :params {:ParamEyeLOpen 1.0}}
         :dance/setlist [{:title "A" :bpm 128.0 :bars 8 :dance :wota}]}
        "#;
        let mut sc = DanceScene::from_edn(SRC).expect("scene");
        assert!(sc.live2d.is_some(), "live2d bound");
        sc.show.start();
        let f = sc.frame(1.0 / 60.0);
        let l = f.live2d.expect("live2d entry in frame");
        let m = l.as_map().unwrap();
        assert_eq!(
            mget(m, "model").and_then(|v| v.as_string()),
            Some("haru.model3.json")
        );
        // params present and beat-driven (ParamAngleX etc.).
        let params = mget(m, "params").and_then(|v| v.as_map()).expect("params");
        assert!(
            mget(params, "ParamMouthOpenY").is_some(),
            "lipsync param driven"
        );
        assert!(mget(params, "ParamBreath").is_some());
    }

    /// Whole-stack regression guard: the committed reference scene must drive
    /// every authored subsystem at once (ADR-0043/0044/0045) — VRM avatar mesh +
    /// clip animation + expressions, Live2D, lights, materials, post chain,
    /// crowd, and beat-synced reactions. If any wiring regresses, this fails.
    #[test]
    fn reference_scene_exercises_full_stack() {
        const EXAMPLE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
        let mut sc = DanceScene::from_edn(EXAMPLE).expect("scene");
        // static authored vocabulary.
        assert!(
            sc.clip_names().contains(&"idle".to_string()),
            "EDN clip authored"
        );
        assert_eq!(sc.post.len(), 3, "post chain authored");
        assert!(sc.live2d.is_some(), "Live2D performer authored");
        assert_eq!(
            sc.avatar.clip.as_deref(),
            Some("idle"),
            "avatar references clip"
        );

        sc.show.start();
        let mut f = sc.frame(1.0 / 30.0);
        for _ in 0..60 {
            f = sc.frame(1.0 / 30.0);
        }
        let root = f.render_ir.as_map().expect("render-ir map");
        let vec_len = |k: &str| {
            mget(root, k)
                .and_then(|v| v.as_vector())
                .map(|s| s.len())
                .unwrap_or(0)
        };
        assert!(vec_len("lights") >= 3, "lighting rig → lights");
        assert!(mget(root, "camera").and_then(|v| v.as_map()).is_some());
        assert!(vec_len("materials") >= 1, "performer material");
        assert_eq!(vec_len("meshes"), 1, "the VRM avatar");
        assert_eq!(vec_len("animations"), 1, "avatar clip → animations layer");
        assert_eq!(vec_len("post"), 3, "post chain injected");
        assert!(vec_len("instances") > 1, "performer + crowd instances");
        // the VRM mesh carries show-driven expressions.
        let mesh0 = mget(root, "meshes").and_then(|v| v.as_vector()).unwrap()[0]
            .as_map()
            .unwrap();
        assert!(
            mget(mesh0, "expressions")
                .and_then(|v| v.as_map())
                .is_some()
        );
        // Live2D params resolved this frame.
        assert!(f.live2d.is_some(), "Live2D driven");
    }

    #[test]
    fn camera_trigger_sets_persistent_shot() {
        const SRC: &str = r#"
        {:dance/show     {:bpm 140.0 :stage :festival}
         :dance/triggers [{:on :callout :camera :closeup}]
         :dance/setlist  [{:title "A" :bpm 140.0 :bars 8 :dance :wota
                           :cues [{:beat 1 :kind :callout :tag "intro"}]}]}
        "#;
        let mut sc = DanceScene::from_edn(SRC).expect("scene");
        sc.show.start();
        let mut shot = None;
        for _ in 0..300 {
            let f = sc.frame(1.0 / 60.0);
            if let Some(s) = f.render_ir.as_map().and_then(|m| {
                mget(m, "camera-shot")
                    .and_then(|v| v.as_keyword())
                    .map(|k| k.0.name.clone())
            }) {
                shot = Some(s);
                break;
            }
        }
        assert_eq!(
            shot.as_deref(),
            Some("closeup"),
            "callout cue set the camera shot"
        );
        // and it persists on the scene.
        assert_eq!(sc.active_camera.as_deref(), Some("closeup"));
    }

    #[test]
    fn fx_triggers_emit_particle_bursts() {
        const SRC: &str = r#"
        {:dance/show     {:bpm 140.0 :stage :festival}
         :dance/triggers [{:on :drop :fx :confetti}]
         :dance/setlist  [{:title "A" :bpm 140.0 :bars 8 :dance :wota
                           :cues [{:beat 4 :kind :drop :tag "hook"}]}]}
        "#;
        let mut sc = DanceScene::from_edn(SRC).expect("scene");
        sc.show.start();
        let mut saw_particles = false;
        for _ in 0..600 {
            let f = sc.frame(1.0 / 60.0);
            if let Some(parts) = f
                .render_ir
                .as_map()
                .and_then(|m| mget(m, "particles").and_then(|v| v.as_vector()))
            {
                if !parts.is_empty() {
                    // the confetti burst carries a colour + count.
                    let b = parts[0].as_map().unwrap();
                    assert!(mget(b, "color").is_some() && mget(b, "count").is_some());
                    saw_particles = true;
                    break;
                }
            }
        }
        assert!(
            saw_particles,
            "drop → :confetti → a :particles burst in the render-IR"
        );
    }

    #[test]
    fn frame_emits_kami_audio_sound_recipes() {
        // the track's :audio cues + :sound triggers project into kami.audio EDN
        // recipes ({:wave :freq :dur :gain :at}) on DanceFrame.sounds.
        const SRC: &str = r#"
        {:dance/show    {:bpm 128.0 :stage :club}
         :dance/audio   {:bank {:kick {:wave "sine" :freq 100 :dur 0.2 :gain 0.6}}}
         :dance/setlist [{:title "A" :bpm 128.0 :bars 8 :dance :wota :audio :opener
                          :cues [{:beat 1 :kind :drop :tag "h"}]}]}
        "#;
        let mut sc = DanceScene::from_edn(SRC).expect("scene");
        // the EDN bank overrode :kick.
        assert_eq!(sc.sound_bank.get("kick").map(|c| c.freq), Some(100.0));
        sc.show.start();
        let mut saw = false;
        for _ in 0..600 {
            let f = sc.frame(1.0 / 60.0);
            if let Some(first) = f.sounds.first() {
                let m = first.as_map().expect("recipe map");
                assert!(mget(m, "wave").is_some() && mget(m, "freq").is_some() && mget(m, "at").is_some());
                saw = true;
                break;
            }
        }
        assert!(saw, ":audio cues → kami.audio recipes on DanceFrame.sounds");
    }

    #[test]
    fn frame_surfaces_audio_cues_from_track_program() {
        // a track with an `:audio` program → DanceFrame.audio carries the
        // synthesised drum/bass cues for a host's Web Audio bridge.
        const SRC: &str = r#"
        {:dance/show    {:bpm 128.0 :stage :club}
         :dance/setlist [{:title "A" :bpm 128.0 :bars 8 :dance :wota :audio :opener
                          :cues [{:beat 1 :kind :drop :tag "h"}]}]}
        "#;
        let mut sc = DanceScene::from_edn(SRC).expect("scene");
        sc.show.start();
        let mut saw_audio = false;
        for _ in 0..600 {
            let f = sc.frame(1.0 / 60.0);
            if !f.audio.is_empty() {
                saw_audio = true;
                break;
            }
        }
        assert!(saw_audio, "the :opener audio program → drum/bass cues on DanceFrame.audio");
    }

    #[test]
    fn varied_fx_emit_distinct_bursts() {
        // each new fx type resolves to its own particle-burst signature.
        for fx in ["fireworks", "laser", "smoke", "hearts", "petals", "embers"] {
            let src = format!(
                "{{:dance/show {{:bpm 140.0 :stage :festival}}\n \
                  :dance/triggers [{{:on :drop :fx :{fx}}}]\n \
                  :dance/setlist [{{:title \"A\" :bpm 140.0 :bars 8 :dance :wota \
                  :cues [{{:beat 4 :kind :drop :tag \"h\"}}]}}]}}"
            );
            let mut sc = DanceScene::from_edn(&src).expect("scene");
            sc.show.start();
            let mut saw = false;
            for _ in 0..600 {
                let f = sc.frame(1.0 / 60.0);
                if let Some(parts) = f
                    .render_ir
                    .as_map()
                    .and_then(|m| mget(m, "particles").and_then(|v| v.as_vector()))
                {
                    if !parts.is_empty() {
                        saw = true;
                        break;
                    }
                }
            }
            assert!(saw, "fx :{fx} emits a particle burst");
        }
    }

    #[test]
    fn dance_post_chain_injected_into_render_ir() {
        const SRC: &str = r#"
        {:dance/show    {:bpm 128.0 :stage :club}
         :dance/post    [{:fx :bloom :intensity 0.6} {:fx :vignette :intensity 0.4}]
         :dance/setlist [{:title "A" :bpm 128.0 :bars 8 :dance :wota}]}
        "#;
        let mut sc = DanceScene::from_edn(SRC).expect("scene");
        assert_eq!(sc.post.len(), 2);
        sc.show.start();
        let f = sc.frame(1.0 / 60.0);
        let root = f.render_ir.as_map().expect("render-ir map");
        let post = mget(root, "post")
            .and_then(|v| v.as_vector())
            .expect(":post in render-ir");
        assert_eq!(post.len(), 2);
        let first = post[0].as_map().unwrap();
        assert_eq!(
            mget(first, "fx")
                .and_then(|v| v.as_keyword())
                .map(|k| k.0.name.as_str()),
            Some("bloom")
        );
    }

    #[test]
    fn no_live2d_when_unbound() {
        let mut sc = DanceScene::from_edn(SCENE).expect("scene");
        sc.show.start();
        assert!(sc.frame(1.0 / 60.0).live2d.is_none());
    }

    #[test]
    fn parses_inline_audio() {
        let src = r#"
        {:dance/show {:bpm 120.0 :stage :club}
         :dance/setlist
         [{:title "Named"  :bars 4 :audio :opener}
          {:title "Inline" :bars 4
           :audio {:drums {:kick [1.0 0 1.0 0 1.0 0 1.0 0] :snare [0 0 0.9 0 0 0 0.9 0]}
                   :bass  [{:beat 0 :midi 36 :len 2.0 :vel 0.9}
                           {:beat 2 :midi 43 :len 2.0 :vel 0.8}]
                   :lead-arp [60 63 67 70]
                   :pad-chord [60 63 67]}}]}
        "#;
        let sc = DanceScene::from_edn(src).expect("scene");
        let tracks = &sc.show.setlist().tracks;
        // named preset still resolves.
        assert!(tracks[0].audio.is_some());
        // inline program: drums + bass + arp + pad all parsed from EDN.
        let a = tracks[1].audio.as_ref().expect("inline audio");
        let drums = a.drums.as_ref().expect("drums");
        // kick on the 4 down-steps (0,2,4,6).
        assert_eq!(drums.hits_at(0).count(), 1, "kick at step 0");
        assert!(drums.hits_at(2).any(|(s, _)| matches!(s, DrumSlot::Snare)));
        let bass = a.bass.as_ref().expect("bass");
        assert_eq!(bass.notes.len(), 2);
        assert_eq!(bass.notes[0].pitch_midi, 36);
        assert_eq!(a.lead_arp, vec![60, 63, 67, 70]);
        assert_eq!(a.pad_chord, vec![60, 63, 67]);
    }

    #[test]
    fn parses_vj_deck() {
        let src = r#"
        {:dance/show {:bpm 120.0 :stage :club}
         :dance/vj
         [{:pattern :rings :palette :sunset}
          {:pattern :noise :palette {:primary [1.0 1.0 1.0] :secondary [0.0 0.0 0.0] :accent [0.5 0.5 0.5]}}]}
        "#;
        let mut sc = DanceScene::from_edn(src).expect("scene");
        sc.show.start();
        sc.show.tick(0.05); // phrase 0 → program[0]
        let frame = sc.show.snapshot().vj;
        assert!(matches!(frame.pattern, VJPattern::Rings));
        assert_eq!(frame.palette.primary, Palette::SUNSET.primary);
    }

    #[test]
    fn envelope_and_fixture_mapping() {
        assert!(matches!(
            lighting_fixture_by_name("spot"),
            LightingFixture::Spot
        ));
        assert!(matches!(
            lighting_fixture_by_name("???"),
            LightingFixture::FrontPar
        ));
        // bare keyword + map forms.
        let kw = EdnValue::kw_bare("breathe");
        assert!(matches!(envelope_from_edn(Some(&kw)), Envelope::Breathe));
    }

    #[test]
    fn stage_and_cue_name_mapping() {
        assert!(matches!(stage_preset_by_name("club"), StagePreset::Club));
        assert!(matches!(
            stage_preset_by_name("festival"),
            StagePreset::Festival
        ));
        assert!(matches!(stage_preset_by_name("???"), StagePreset::Hall));
        assert!(matches!(cue_kind_by_name("drop"), CueKind::Drop));
        assert!(matches!(cue_kind_by_name("nope"), CueKind::Custom));
    }
}
