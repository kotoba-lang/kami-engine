//! kami-character-scene — EDN authoring surface for `kami-character`
//! HAIR-STYLE presets.
//!
//! The data-tier counterpart of `kami-vehicle-scene` / `kami-postfx-scene` /
//! `kami-autodrive-scene` for the parametric hair generator: it turns canonical
//! `:character/hair-styles` EDN (a table of named [`kami_character::HairStyle`]
//! parameter maps) into the real engine struct, the same way the hardcoded preset
//! fns build it. It re-uses the tolerant `kami-scene` accessors the same way games
//! parse `scene.edn` (missing keys fall back to defaults, namespaced keywords match
//! on `ns/name`, ints coerce to floats).
//!
//! ## Why this is safe (ADR-0038)
//!
//! Hot procedural geometry generation (`generate_groom` / `generate_hair_cards` /
//! `generate_hair_mesh`) stays native Rust (`kami-character::hair_gen`). A hair-style
//! preset is **init-time CONFIG** — the parameter struct read once when geometry is
//! generated — so it is safe to move to EDN. `kami-character` itself stays untouched;
//! the EDN dependency lives only here. The compiled-in
//! `HairStyle::{blonde_long,dark_short,red_wavy,brown_curly,afro}()` builders remain
//! the [`builtin_hair_style`] fallback and are parity-tested against the shipped EDN
//! ([`crate::HAIR_EDN`]).
//!
//! ## EDN shape (see `data/hair.edn`)
//!
//! ```edn
//! {:character/hair-styles
//!  {:blonde-long {:style :straight :length 0.7 :density 0.8 :volume 0.5 :curl 0.03
//!                 :part-side 0.1 :bangs-length 0.3 :bangs-width 0.5
//!                 :color [0.93 0.86 0.72] :highlight-color [0.97 0.92 0.82]
//!                 :highlight-ratio 0.35 :root-darken 0.7
//!                 :head-radius 0.09 :head-center-y 1.43}
//!   :dark-short {...} :red-wavy {...} :brown-curly {...} :afro {...}}}
//! ```
//!
//! Each style is a map of the 14 [`kami_character::HairStyle`] fields, hyphenated.
//! `:style` is a [`kami_character::HairType`] keyword id (`:straight` / `:wavy` /
//! `:curly` / `:afro` / `:braided`); colours are `[r g b]` via `kami_scene::vec3`.
//! Because the preset fns use `..Self::default()`, the shipped EDN reproduces the
//! **resolved** values (the omitted fields are the resolved defaults).

use std::collections::BTreeMap;

use kami_character::{HairStyle, HairType};
use kami_scene::{mget, num, root_map, vec3, EdnValue};

/// The canonical hair-style preset CONFIG shipped with this crate. This is the source
/// of truth; the compiled-in preset fns are the parity-tested mirror.
pub const HAIR_EDN: &str = include_str!("../data/hair.edn");

/// Names of the hair styles shipped as the compiled-in oracle (iteration source for
/// `builtin`/parity). Keeping this list here (not in `kami-character`) keeps the engine
/// crate untouched. Order mirrors the `impl HairStyle` declaration order.
pub const ALL_HAIR_STYLE_NAMES: [&str; 5] =
    ["blonde-long", "dark-short", "red-wavy", "brown-curly", "afro"];

/// Errors raised while loading hair-style preset CONFIG from EDN.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN source did not parse to a top-level map.
    #[error("hair EDN root is not a map")]
    NotAMap,
    /// The `:character/hair-styles` table was missing or not a map.
    #[error("`:character/hair-styles` missing or not a map")]
    NoTable,
    /// The requested style id was missing under `:character/hair-styles`.
    #[error("hair style `{0}` not found under `:character/hair-styles`")]
    StyleNotFound(String),
}

/// The hyphenated `:style` keyword id for a [`HairType`] variant. Inverse of
/// [`hair_type_from_id`].
pub fn id_from_hair_type(t: HairType) -> &'static str {
    match t {
        HairType::Straight => "straight",
        HairType::Wavy => "wavy",
        HairType::Curly => "curly",
        HairType::Afro => "afro",
        HairType::Braided => "braided",
    }
}

/// Parse a [`HairType`] from its hyphenated keyword id; unknown / missing ids degrade
/// to the engine default ([`HairType::Straight`], matching `HairStyle::default`).
pub fn hair_type_from_id(id: &str) -> HairType {
    match id {
        "wavy" => HairType::Wavy,
        "curly" => HairType::Curly,
        "afro" => HairType::Afro,
        "braided" => HairType::Braided,
        // "straight" and anything unknown → the Default's HairType.
        _ => HairType::Straight,
    }
}

/// A `PartialEq` mirror of [`kami_character::HairStyle`] — every field, projected so the
/// data tier can assert parity directly.
///
/// `kami_character::HairStyle` derives only `Debug, Clone, Serialize, Deserialize` (no
/// `PartialEq`), so the data tier cannot assert `loaded == blonde_long()` without
/// touching the engine crate. `HairStyleSpec` is a `PartialEq` projection of all 14
/// fields (`HairType` is `PartialEq`), so parity (every field, exact f32 equality) is
/// asserted here additively, with `kami-character` left untouched.
#[derive(Debug, Clone, PartialEq)]
pub struct HairStyleSpec {
    pub style: HairType,
    pub length: f32,
    pub density: f32,
    pub volume: f32,
    pub curl: f32,
    pub part_side: f32,
    pub bangs_length: f32,
    pub bangs_width: f32,
    pub color: [f32; 3],
    pub highlight_color: [f32; 3],
    pub highlight_ratio: f32,
    pub root_darken: f32,
    pub head_radius: f32,
    pub head_center_y: f32,
}

impl HairStyleSpec {
    /// Project a real [`HairStyle`] into the comparable [`HairStyleSpec`] (field-for-field).
    pub fn from_hair_style(h: &HairStyle) -> Self {
        HairStyleSpec {
            style: h.style,
            length: h.length,
            density: h.density,
            volume: h.volume,
            curl: h.curl,
            part_side: h.part_side,
            bangs_length: h.bangs_length,
            bangs_width: h.bangs_width,
            color: h.color,
            highlight_color: h.highlight_color,
            highlight_ratio: h.highlight_ratio,
            root_darken: h.root_darken,
            head_radius: h.head_radius,
            head_center_y: h.head_center_y,
        }
    }

    /// Reconstruct the real engine [`HairStyle`] from this spec (inverse of
    /// [`HairStyleSpec::from_hair_style`]).
    pub fn to_hair_style(&self) -> HairStyle {
        HairStyle {
            style: self.style,
            length: self.length,
            density: self.density,
            volume: self.volume,
            curl: self.curl,
            part_side: self.part_side,
            bangs_length: self.bangs_length,
            bangs_width: self.bangs_width,
            color: self.color,
            highlight_color: self.highlight_color,
            highlight_ratio: self.highlight_ratio,
            root_darken: self.root_darken,
            head_radius: self.head_radius,
            head_center_y: self.head_center_y,
        }
    }
}

/// Build one real [`HairStyle`] from a hair-style map.
///
/// Every field is read with the tolerant accessors, so a key a map omits degrades to the
/// engine default (`HairStyle::default()`'s value for that field) — `..Self::default()`
/// in EDN form. `:style` is read as a [`HairType`] keyword id; missing → `:straight`.
pub fn to_hair_style(m: &BTreeMap<EdnValue, EdnValue>) -> HairStyle {
    let d = HairStyle::default();

    // `:style` keyword id → HairType; absent → the Default's HairType.
    let style = match mget(m, "style").and_then(kami_scene::kw_key) {
        Some(id) => hair_type_from_id(&id),
        None => d.style,
    };

    // Each scalar field: present → the EDN value; absent → the resolved default (mirrors
    // `..Self::default()`). `num` coerces ints → f32 and yields 0.0 for non-numbers, so a
    // present-but-malformed value degrades the same way the rest of the data tier does.
    let f = |key: &str, default: f32| match mget(m, key) {
        Some(v) => num(Some(v)),
        None => default,
    };
    let c = |key: &str, default: [f32; 3]| match mget(m, key) {
        Some(v) => vec3(Some(v)),
        None => default,
    };

    HairStyle {
        style,
        length: f("length", d.length),
        density: f("density", d.density),
        volume: f("volume", d.volume),
        curl: f("curl", d.curl),
        part_side: f("part-side", d.part_side),
        bangs_length: f("bangs-length", d.bangs_length),
        bangs_width: f("bangs-width", d.bangs_width),
        color: c("color", d.color),
        highlight_color: c("highlight-color", d.highlight_color),
        highlight_ratio: f("highlight-ratio", d.highlight_ratio),
        root_darken: f("root-darken", d.root_darken),
        head_radius: f("head-radius", d.head_radius),
        head_center_y: f("head-center-y", d.head_center_y),
    }
}

/// The compiled-in fallback / parity oracle: the real
/// `HairStyle::{blonde_long,dark_short,red_wavy,brown_curly,afro}()`. Returns `None` for
/// an unknown name. This is what the shipped EDN is parity-tested against.
pub fn builtin_hair_style(name: &str) -> Option<HairStyle> {
    Some(match name {
        "blonde-long" => HairStyle::blonde_long(),
        "dark-short" => HairStyle::dark_short(),
        "red-wavy" => HairStyle::red_wavy(),
        "brown-curly" => HairStyle::brown_curly(),
        "afro" => HairStyle::afro(),
        _ => return None,
    })
}

/// Parse the whole `:character/hair-styles` table from EDN `src` into a map keyed by the
/// (hyphenated) style id, each value the rebuilt [`HairStyle`].
pub fn hair_styles_from_edn(src: &str) -> Result<BTreeMap<String, HairStyle>, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = mget(&root, "character/hair-styles")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable)?;

    let mut by_id = BTreeMap::new();
    for (k, v) in table.iter() {
        let Some(id) = kami_scene::kw_key(k) else { continue };
        let Some(map) = v.as_map() else { continue };
        by_id.insert(id, to_hair_style(map));
    }
    Ok(by_id)
}

/// Look up & rebuild a single hair style by (hyphenated) id from EDN `src`. Errors if the
/// table or the named style is absent.
pub fn hair_style_from_edn(src: &str, name: &str) -> Result<HairStyle, Error> {
    let root = root_map(src).ok_or(Error::NotAMap)?;
    let table = mget(&root, "character/hair-styles")
        .and_then(|v| v.as_map())
        .ok_or(Error::NoTable)?;
    let map = table
        .iter()
        .find_map(|(k, v)| (kami_scene::kw_key(k).as_deref() == Some(name)).then_some(v))
        .and_then(|v| v.as_map())
        .ok_or_else(|| Error::StyleNotFound(name.to_string()))?;
    Ok(to_hair_style(map))
}

/// Convenience: load & rebuild all hair styles from the crate-shipped [`HAIR_EDN`].
pub fn shipped_hair_styles() -> Result<BTreeMap<String, HairStyle>, Error> {
    hair_styles_from_edn(HAIR_EDN)
}

/// Convenience: load & rebuild one hair style from the shipped EDN.
pub fn shipped_hair_style(name: &str) -> Result<HairStyle, Error> {
    hair_style_from_edn(HAIR_EDN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_all_styles() {
        let h = shipped_hair_styles().expect("hair.edn parse");
        assert_eq!(h.len(), 5);
        for name in ALL_HAIR_STYLE_NAMES {
            assert!(h.contains_key(name), "{name} present in EDN");
        }
    }

    #[test]
    fn unknown_builtin_style_is_none() {
        assert!(builtin_hair_style("does-not-exist").is_none());
    }

    #[test]
    fn unknown_style_from_edn_is_an_error() {
        assert!(matches!(
            hair_style_from_edn(HAIR_EDN, "rainbow-mohawk"),
            Err(Error::StyleNotFound(_))
        ));
    }

    #[test]
    fn non_map_root_is_an_error() {
        assert!(matches!(hair_styles_from_edn("42"), Err(Error::NotAMap)));
    }

    #[test]
    fn missing_table_is_an_error() {
        assert!(matches!(
            hair_styles_from_edn("{:other 1}"),
            Err(Error::NoTable)
        ));
    }

    #[test]
    fn hair_type_id_round_trips() {
        for t in [
            HairType::Straight,
            HairType::Wavy,
            HairType::Curly,
            HairType::Afro,
            HairType::Braided,
        ] {
            assert_eq!(hair_type_from_id(id_from_hair_type(t)), t);
        }
    }

    #[test]
    fn spec_round_trips_through_hair_style() {
        let h = HairStyle::red_wavy();
        let spec = HairStyleSpec::from_hair_style(&h);
        let back = spec.to_hair_style();
        assert_eq!(HairStyleSpec::from_hair_style(&back), spec);
    }
}
