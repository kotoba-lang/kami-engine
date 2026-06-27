//! Anim-blueprint data tier — `kami-character`'s MetaHuman default animation blueprint
//! (`AnimBlueprint::metahuman_default()`) as parity-tested EDN.
//!
//! The state-machine **evaluation** (`update` / transitions / blend) stays native Rust;
//! only the init-time **description** — parameters, layers, states, blend spaces,
//! transitions, blend profiles — moves to EDN (ADR-0046 / ADR-0038). [`blueprint_from_edn`]
//! rebuilds the *same* [`kami_character::anim_blueprint::AnimBlueprint`], asserted
//! component-for-component `==` the compiled-in `metahuman_default()` in
//! `tests/anim_blueprint_parity.rs`.
//!
//! `param-type` is DERIVED (Bool when the name starts with `is_`, else Float), mirroring
//! the builtin, so each parameter only carries `{:name :default}`.

use std::collections::{BTreeMap, HashMap};

use kami_character::anim_blueprint::{
    AnimBlueprint, AnimLayer, AnimParam, AnimParamType, AnimState, AnimStateType, AnimTransition,
    BlendCurve, BlendProfile, BlendSpace2DEntry, BlendSpaceEntry, Comparison, LayerBlendMode,
    TransitionCondition,
};
use kami_scene::{EdnValue, kw_key, mget, num, root_map};

/// The canonical anim-blueprint CONFIG shipped with this crate.
pub const ANIM_BLUEPRINT_EDN: &str = include_str!("../data/anim_blueprint.edn");

/// Errors raised while loading the anim-blueprint CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum AnimBlueprintError {
    /// The EDN source did not parse to a top-level map.
    #[error("anim-blueprint EDN root is not a map")]
    NotAMap,
    /// The `:character/anim-blueprint` table was missing or not a map.
    #[error("`:character/anim-blueprint` missing or not a map")]
    NoBlueprint,
}

type Map = BTreeMap<EdnValue, EdnValue>;

// --- tolerant scalar readers ---------------------------------------------------------

fn str_at(m: &Map, key: &str) -> String {
    mget(m, key)
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string()
}
fn bool_at(m: &Map, key: &str) -> bool {
    mget(m, key).and_then(|v| v.as_bool()).unwrap_or(false)
}
fn int_at(m: &Map, key: &str) -> i64 {
    mget(m, key).and_then(|v| v.as_integer()).unwrap_or(0)
}
fn usize_at(m: &Map, key: &str) -> usize {
    int_at(m, key).max(0) as usize
}
/// Read a keyword-valued field as its `ns/name` string (e.g. `:ease-in-out` → `"ease-in-out"`).
fn kw_at(m: &Map, key: &str) -> String {
    mget(m, key).and_then(kw_key).unwrap_or_default()
}
fn vec_at<'a>(m: &'a Map, key: &str) -> &'a [EdnValue] {
    mget(m, key).and_then(|v| v.as_vector()).unwrap_or(&[])
}

// --- enum mappers --------------------------------------------------------------------

fn blend_mode(id: &str) -> LayerBlendMode {
    match id {
        "additive" => LayerBlendMode::Additive,
        _ => LayerBlendMode::Override,
    }
}
fn blend_curve(id: &str) -> BlendCurve {
    match id {
        "ease-in" => BlendCurve::EaseIn,
        "ease-out" => BlendCurve::EaseOut,
        "ease-in-out" => BlendCurve::EaseInOut,
        "cubic" => BlendCurve::Cubic,
        _ => BlendCurve::Linear,
    }
}
fn comparison(id: &str) -> Comparison {
    match id {
        "less" => Comparison::Less,
        "equal" => Comparison::Equal,
        "not-equal" => Comparison::NotEqual,
        "greater-equal" => Comparison::GreaterEqual,
        "less-equal" => Comparison::LessEqual,
        _ => Comparison::Greater,
    }
}

// --- sub-parsers ---------------------------------------------------------------------

fn state_type(m: &Map) -> AnimStateType {
    let t = match mget(m, "type").and_then(|v| v.as_map()) {
        Some(t) => t,
        None => return AnimStateType::PoseSnapshot,
    };
    match kw_at(t, "kind").as_str() {
        "clip" => AnimStateType::Clip {
            clip_name: str_at(t, "clip-name"),
        },
        "blend-space-1d" => AnimStateType::BlendSpace1D {
            axis_param: str_at(t, "axis-param"),
            entries: vec_at(t, "entries")
                .iter()
                .filter_map(|e| e.as_map())
                .map(|e| BlendSpaceEntry {
                    clip_name: str_at(e, "clip"),
                    position: num(mget(e, "position")),
                })
                .collect(),
        },
        "blend-space-2d" => AnimStateType::BlendSpace2D {
            x_param: str_at(t, "x-param"),
            y_param: str_at(t, "y-param"),
            entries: vec_at(t, "entries")
                .iter()
                .filter_map(|e| e.as_map())
                .map(|e| BlendSpace2DEntry {
                    clip_name: str_at(e, "clip"),
                    x: num(mget(e, "x")),
                    y: num(mget(e, "y")),
                })
                .collect(),
        },
        "layered-blend-per-bone" => AnimStateType::LayeredBlendPerBone {
            base_clip: str_at(t, "base-clip"),
            overlay_clip: str_at(t, "overlay-clip"),
            bone_filter: vec_at(t, "bone-filter")
                .iter()
                .filter_map(|b| b.as_string().map(str::to_string))
                .collect(),
            blend_param: str_at(t, "blend-param"),
        },
        _ => AnimStateType::PoseSnapshot,
    }
}

fn anim_state(m: &Map) -> AnimState {
    AnimState {
        name: str_at(m, "name"),
        state_type: state_type(m),
        play_rate: num(mget(m, "play-rate")),
        looping: bool_at(m, "looping"),
    }
}

fn transition(m: &Map) -> AnimTransition {
    AnimTransition {
        source: usize_at(m, "source"),
        target: usize_at(m, "target"),
        duration: num(mget(m, "duration")),
        blend_curve: blend_curve(&kw_at(m, "curve")),
        conditions: vec_at(m, "conditions")
            .iter()
            .filter_map(|c| c.as_map())
            .map(|c| TransitionCondition {
                param_name: str_at(c, "param"),
                comparison: comparison(&kw_at(c, "cmp")),
                threshold: num(mget(c, "threshold")),
            })
            .collect(),
        priority: int_at(m, "priority").max(0) as u32,
    }
}

fn layer(m: &Map) -> AnimLayer {
    AnimLayer {
        name: str_at(m, "name"),
        blend_mode: blend_mode(&kw_at(m, "blend-mode")),
        weight: num(mget(m, "weight")),
        states: vec_at(m, "states")
            .iter()
            .filter_map(|s| s.as_map())
            .map(anim_state)
            .collect(),
        transitions: vec_at(m, "transitions")
            .iter()
            .filter_map(|t| t.as_map())
            .map(transition)
            .collect(),
        // The default blueprint starts at state 0, not mid-transition. These are
        // runtime fields the builtin initialises to 0 / None; a preset may override
        // :active-state but otherwise inherits the resting values.
        active_state: usize_at(m, "active-state"),
        transition_progress: num(mget(m, "transition-progress")),
        transition_target: mget(m, "transition-target")
            .and_then(|v| v.as_integer())
            .map(|i| i.max(0) as usize),
    }
}

fn blend_profile(m: &Map) -> BlendProfile {
    let bone_weights = mget(m, "bone-weights")
        .and_then(|v| v.as_map())
        .map(|bw| {
            bw.iter()
                .filter_map(|(k, v)| kw_key(k).map(|name| (name, num(Some(v)))))
                .collect::<HashMap<String, f32>>()
        })
        .unwrap_or_default();
    BlendProfile {
        name: str_at(m, "name"),
        bone_weights,
    }
}

fn anim_param(m: &Map) -> AnimParam {
    let name = str_at(m, "name");
    let default = num(mget(m, "default"));
    // DERIVED, mirroring metahuman_default(): is_* → Bool, else Float.
    let param_type = if name.starts_with("is_") {
        AnimParamType::Bool
    } else {
        AnimParamType::Float
    };
    AnimParam {
        name,
        param_type,
        value: default,
        default_value: default,
    }
}

// --- public API ----------------------------------------------------------------------

/// Build the real [`AnimBlueprint`] from EDN `src`.
pub fn blueprint_from_edn(src: &str) -> Result<AnimBlueprint, AnimBlueprintError> {
    let root = root_map(src).ok_or(AnimBlueprintError::NotAMap)?;
    let bp = mget(&root, "character/anim-blueprint")
        .and_then(|v| v.as_map())
        .ok_or(AnimBlueprintError::NoBlueprint)?;

    let parameters: HashMap<String, AnimParam> = vec_at(bp, "parameters")
        .iter()
        .filter_map(|p| p.as_map())
        .map(anim_param)
        .map(|p| (p.name.clone(), p))
        .collect();

    let layers: Vec<AnimLayer> = vec_at(bp, "layers")
        .iter()
        .filter_map(|l| l.as_map())
        .map(layer)
        .collect();

    let blend_profiles: Vec<BlendProfile> = vec_at(bp, "blend-profiles")
        .iter()
        .filter_map(|b| b.as_map())
        .map(blend_profile)
        .collect();

    Ok(AnimBlueprint {
        parameters,
        layers,
        blend_profiles,
    })
}

/// Convenience: build the blueprint from the crate-shipped [`ANIM_BLUEPRINT_EDN`].
pub fn shipped_blueprint() -> Result<AnimBlueprint, AnimBlueprintError> {
    blueprint_from_edn(ANIM_BLUEPRINT_EDN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_two_layers_and_eleven_params() {
        let bp = shipped_blueprint().expect("anim_blueprint.edn parses");
        assert_eq!(bp.layers.len(), 2);
        assert_eq!(bp.layers[0].name, "body");
        assert_eq!(bp.layers[1].name, "face");
        assert_eq!(bp.parameters.len(), 11);
        assert!(bp.parameters.contains_key("jaw_open"));
    }

    #[test]
    fn is_prefixed_param_is_bool() {
        let bp = shipped_blueprint().unwrap();
        assert!(matches!(
            bp.parameters["is_moving"].param_type,
            AnimParamType::Bool
        ));
        assert!(matches!(
            bp.parameters["speed"].param_type,
            AnimParamType::Float
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(
            blueprint_from_edn("42"),
            Err(AnimBlueprintError::NotAMap)
        ));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            blueprint_from_edn("{:other 1}"),
            Err(AnimBlueprintError::NoBlueprint)
        ));
    }
}
