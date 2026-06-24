//! kami-input-scene — EDN authoring surface for `kami-input`'s default
//! input-binding maps (the device→action tables, ADR-0040).
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-character-scene`
//! for the input system: it turns canonical `:input/maps` EDN (named, *ordered*
//! tables of `[key-code action]` pairs) into the real [`kami_input::InputMap`]
//! engine struct, the same way the hardcoded preset fns build it. It re-uses the
//! tolerant `kami-scene` accessors the same way games parse `scene.edn`
//! (namespaced keywords match on `ns/name`, malformed entries are skipped).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot per-frame input resolution (`InputMap::resolve`, gesture detection,
//! focus routing) stays native Rust (`kami-input`). A default binding map is
//! **init-time CONFIG** — the device→action table read once when an app sets up
//! its input handler — so it is safe to move to EDN. `kami-input` itself stays
//! untouched; the EDN dependency lives only here. The compiled-in
//! `InputMap::{default_fps,default_graph}()` builders remain the
//! [`builtin_input_map`] fallback and are parity-tested against the shipped EDN
//! ([`crate::INPUT_EDN`]).
//!
//! ## EDN shape (see `data/input.edn`)
//!
//! ```edn
//! {:input/maps
//!  {:fps   [["KeyW" :move-up] ["ArrowUp" :move-up] ... ["Escape" :pause]]
//!   :graph [["KeyW" :move-up] ... ["Minus" :zoom-out] ["NumpadSubtract" :zoom-out]]}}
//! ```
//!
//! Each map is an **ordered** vector of `[key-code action-keyword]` pairs (order
//! matters — `resolve` is first-match). `key-code` is the W3C
//! `KeyboardEvent.code` string; `action` is an [`kami_input::Action`] keyword id
//! (hyphenated: `:move-up` / `:zoom-in` / …).

use std::collections::BTreeMap;

use kami_input::{Action, InputMap};
use kami_scene::{kw_key, mget, root_map, EdnValue};

/// The canonical default input-binding CONFIG shipped with this crate. This is
/// the source of truth; the compiled-in preset fns are the parity-tested mirror.
pub const INPUT_EDN: &str = include_str!("../data/input.edn");

/// Names of the input maps shipped as the compiled-in oracle (iteration source
/// for `builtin`/parity). Keeping this list here (not in `kami-input`) keeps the
/// engine crate untouched. Order mirrors the `impl InputMap` declaration order.
pub const ALL_MAP_NAMES: [&str; 2] = ["fps", "graph"];

/// Errors raised while loading input-binding map CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("input EDN root is not a map")]
    NotAMap,
    /// The `:input/maps` table was missing or not a map.
    #[error("`:input/maps` missing or not a map")]
    NoTable,
    /// The requested map id was missing under `:input/maps`.
    #[error("input map `{0}` not found under `:input/maps`")]
    MapNotFound(String),
    /// A binding referenced an unknown action keyword id.
    #[error("unknown action `{0}`")]
    UnknownAction(String),
}

/// The hyphenated keyword id for an [`Action`] variant. Inverse of
/// [`action_from_id`].
pub fn id_from_action(a: Action) -> &'static str {
    match a {
        Action::MoveUp => "move-up",
        Action::MoveDown => "move-down",
        Action::MoveLeft => "move-left",
        Action::MoveRight => "move-right",
        Action::ZoomIn => "zoom-in",
        Action::ZoomOut => "zoom-out",
        Action::PanStart => "pan-start",
        Action::PanEnd => "pan-end",
        Action::PanMove => "pan-move",
        Action::Primary => "primary",
        Action::Secondary => "secondary",
        Action::Cancel => "cancel",
        Action::Confirm => "confirm",
        Action::Jump => "jump",
        Action::Sprint => "sprint",
        Action::Interact => "interact",
        Action::Attack => "attack",
        Action::Pause => "pause",
        Action::Reset => "reset",
        Action::Menu => "menu",
        Action::Fullscreen => "fullscreen",
    }
}

/// Parse an [`Action`] from its hyphenated keyword id. Unknown ids yield `None`
/// (the loader turns that into [`Error::UnknownAction`]) — unlike the scalar
/// fallbacks elsewhere, an unrecognised action is a hard error so a typo never
/// silently drops a binding.
pub fn action_from_id(id: &str) -> Option<Action> {
    Some(match id {
        "move-up" => Action::MoveUp,
        "move-down" => Action::MoveDown,
        "move-left" => Action::MoveLeft,
        "move-right" => Action::MoveRight,
        "zoom-in" => Action::ZoomIn,
        "zoom-out" => Action::ZoomOut,
        "pan-start" => Action::PanStart,
        "pan-end" => Action::PanEnd,
        "pan-move" => Action::PanMove,
        "primary" => Action::Primary,
        "secondary" => Action::Secondary,
        "cancel" => Action::Cancel,
        "confirm" => Action::Confirm,
        "jump" => Action::Jump,
        "sprint" => Action::Sprint,
        "interact" => Action::Interact,
        "attack" => Action::Attack,
        "pause" => Action::Pause,
        "reset" => Action::Reset,
        "menu" => Action::Menu,
        "fullscreen" => Action::Fullscreen,
        _ => return None,
    })
}

/// Build one real [`InputMap`] from an ordered vector of `[key-code action]`
/// pairs (the EDN value of one `:input/maps` entry).
///
/// The bindings are rebuilt **in order** — `resolve` is first-match, so order is
/// load-bearing. The key-code is read as a string; the action is read as a
/// keyword id and resolved via [`action_from_id`] (an unknown action id is a
/// hard [`Error::UnknownAction`]). A pair that is malformed in *shape* (not a
/// 2-element vector / non-string key / non-keyword action) is skipped, matching
/// how the rest of the data tier degrades on shape errors.
pub fn input_map_from_pairs(pairs: &[EdnValue]) -> Result<InputMap, Error> {
    let mut bindings: Vec<(String, Action)> = Vec::with_capacity(pairs.len());
    for pair in pairs {
        // Each binding is a [key-code action-keyword] 2-vector.
        let Some(slots) = pair.as_vector() else { continue };
        let (Some(key), Some(act)) = (slots.first(), slots.get(1)) else {
            continue;
        };
        // key-code: a string. Shape error → skip.
        let Some(code) = key.as_string() else { continue };
        // action: a keyword id. Shape error → skip; unknown id → hard error.
        let Some(id) = kw_key(act) else { continue };
        let action = action_from_id(&id).ok_or_else(|| Error::UnknownAction(id.clone()))?;
        bindings.push((code.to_string(), action));
    }
    Ok(InputMap { bindings })
}

/// The compiled-in fallback / parity oracle: the real
/// `InputMap::{default_fps,default_graph}()`. Returns `None` for an unknown name.
/// This is what the shipped EDN is parity-tested against.
pub fn builtin_input_map(name: &str) -> Option<InputMap> {
    Some(match name {
        "fps" => InputMap::default_fps(),
        "graph" => InputMap::default_graph(),
        _ => return None,
    })
}

/// Parse the whole `:input/maps` table from EDN `src` into a map keyed by the
/// map id, each value the rebuilt [`InputMap`] (bindings in order).
pub fn input_maps_from_edn(src: &str) -> Result<BTreeMap<String, InputMap>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = mget(&root, "input/maps")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kw_key(k) else { continue };
        let Some(pairs) = v.as_vector() else { continue };
        by_id.insert(id, input_map_from_pairs(pairs)?);
    }
    Ok(by_id)
}

/// Look up & rebuild a single input map by id from EDN `src`. Errors if the table
/// or the named map is absent (or a binding references an unknown action).
pub fn input_map_from_edn(src: &str, name: &str) -> Result<InputMap, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = mget(&root, "input/maps")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable)?;
    let pairs = table
        .iter()
        .find_map(|(k, v)| (kw_key(k).as_deref() == Some(name)).then_some(v))
        .and_then(|v| v.as_vector())
        .ok_or_else(|| Error::MapNotFound(name.to_string()))?;
    input_map_from_pairs(pairs)
}

/// Convenience: load & rebuild all input maps from the crate-shipped [`INPUT_EDN`].
pub fn shipped_input_maps() -> Result<BTreeMap<String, InputMap>, Error> {
    input_maps_from_edn(INPUT_EDN)
}

/// Convenience: load & rebuild one input map from the shipped EDN.
pub fn shipped_input_map(name: &str) -> Result<InputMap, Error> {
    input_map_from_edn(INPUT_EDN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_maps() {
        let m = shipped_input_maps().expect("input.edn parse");
        assert_eq!(m.len(), 2);
        for name in ALL_MAP_NAMES {
            assert!(m.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn unknown_builtin_map_is_none() {
        assert!(builtin_input_map("does-not-exist").is_none());
    }

    #[test]
    fn unknown_map_from_edn_is_an_error() {
        assert!(matches!(
            input_map_from_edn(INPUT_EDN, "vehicle"),
            Err(Error::MapNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(input_maps_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            input_maps_from_edn("{:other 1}"),
            Err(Error::NoTable)
        ));
    }

    #[test]
    fn unknown_action_is_an_error() {
        assert!(matches!(
            input_map_from_edn(
                "{:input/maps {:m [[\"KeyW\" :fly-to-moon]]}}",
                "m"
            ),
            Err(Error::UnknownAction(a)) if a == "fly-to-moon"
        ));
    }

    #[test]
    fn action_id_round_trips() {
        for a in [
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::ZoomIn,
            Action::ZoomOut,
            Action::PanStart,
            Action::PanEnd,
            Action::PanMove,
            Action::Primary,
            Action::Secondary,
            Action::Cancel,
            Action::Confirm,
            Action::Jump,
            Action::Sprint,
            Action::Interact,
            Action::Attack,
            Action::Pause,
            Action::Reset,
            Action::Menu,
            Action::Fullscreen,
        ] {
            assert_eq!(action_from_id(id_from_action(a)), Some(a));
        }
    }
}
