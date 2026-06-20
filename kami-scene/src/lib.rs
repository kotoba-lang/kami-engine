//! kami-scene — EDN accessor helpers for the Datomic-shaped `scene.edn` a kami-clj
//! game ships (ADR-0036). The native players (`kami-clj-play`, `kami-clj-play3d`)
//! parse the *same* tolerant way: missing keys fall back to defaults rather than
//! panic, namespaced keywords match on `ns/name`, and numbers coerce int↔float.
//! Extracted here so the rule set lives in one tested place, not duplicated per bin.

use std::collections::BTreeMap;

pub use kotoba_edn::EdnValue;

/// Full key string for a map key keyword: `"render/profiles"` for `:render/profiles`,
/// `"world"` for `:world`. `None` if the key isn't a keyword.
pub fn kw_key(k: &EdnValue) -> Option<String> {
    k.as_keyword().map(|kw| match &kw.0.namespace {
        Some(ns) => format!("{ns}/{}", kw.0.name),
        None => kw.0.name.clone(),
    })
}

/// Look up `key` (a `"ns/name"` or bare `"name"` string) in a parsed EDN map.
pub fn mget<'a>(m: &'a BTreeMap<EdnValue, EdnValue>, key: &str) -> Option<&'a EdnValue> {
    m.iter()
        .find_map(|(k, v)| if kw_key(k).as_deref() == Some(key) { Some(v) } else { None })
}

/// Read a number as `f32`, coercing integers; `0.0` when absent or non-numeric.
pub fn num(v: Option<&EdnValue>) -> f32 {
    v.and_then(|x| {
        x.as_float()
            .map(|f| f as f32)
            .or_else(|| x.as_integer().map(|i| i as f32))
    })
    .unwrap_or(0.0)
}

/// Read a 3-vector `[r g b]` / `[x y z]`; missing components default to `0.0`,
/// and a non-vector yields `[0,0,0]`.
pub fn vec3(v: Option<&EdnValue>) -> [f32; 3] {
    let s = v.and_then(|x| x.as_vector()).unwrap_or(&[]);
    let g = |i: usize| s.get(i).map(|x| num(Some(x))).unwrap_or(0.0);
    [g(0), g(1), g(2)]
}

/// Parse `src` and return the top-level map (the scene root), if it is one.
pub fn root_map(src: &str) -> Option<BTreeMap<EdnValue, EdnValue>> {
    kotoba_edn::parse_all(src)
        .ok()?
        .into_iter()
        .next()
        .and_then(|f| f.as_map().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(src: &str) -> BTreeMap<EdnValue, EdnValue> {
        root_map(src).expect("map")
    }

    #[test]
    fn keyword_keys_bare_and_namespaced() {
        let m = map("{:world 1 :render/profiles 2}");
        assert!(mget(&m, "world").is_some());
        assert!(mget(&m, "render/profiles").is_some(), "namespaced key resolves");
        assert!(mget(&m, "profiles").is_none(), "namespace must match too");
        assert!(mget(&m, "missing").is_none());
    }

    #[test]
    fn num_coerces_int_and_float_and_defaults() {
        let m = map("{:a 240.0 :b 5 :c \"x\"}");
        assert_eq!(num(mget(&m, "a")), 240.0);
        assert_eq!(num(mget(&m, "b")), 5.0, "integer coerces to f32");
        assert_eq!(num(mget(&m, "c")), 0.0, "non-number → 0");
        assert_eq!(num(mget(&m, "absent")), 0.0, "missing → 0");
    }

    #[test]
    fn vec3_pads_short_and_handles_non_vector() {
        let m = map("{:full [0.1 0.2 0.3] :short [9.0] :scalar 4.0 :ints [1 2 3]}");
        assert_eq!(vec3(mget(&m, "full")), [0.1, 0.2, 0.3]);
        assert_eq!(vec3(mget(&m, "short")), [9.0, 0.0, 0.0], "short pads with 0");
        assert_eq!(vec3(mget(&m, "scalar")), [0.0, 0.0, 0.0], "non-vector → zeros");
        assert_eq!(vec3(mget(&m, "ints")), [1.0, 2.0, 3.0], "int vector coerces");
        assert_eq!(vec3(mget(&m, "absent")), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn nested_maps_round_trip() {
        let m = map("{:world {:player-speed 300.0 :arena 460.0}}");
        let w = mget(&m, "world").and_then(|v| v.as_map()).expect("nested map");
        assert_eq!(num(mget(w, "player-speed")), 300.0);
        assert_eq!(num(mget(w, "arena")), 460.0);
    }

    #[test]
    fn bad_input_is_graceful() {
        assert!(root_map("not a map, just a number 5").map_or(true, |_| true)); // no panic
        assert!(root_map("{:unclosed 1").is_none(), "malformed EDN → None, no panic");
        assert!(root_map("42").is_none(), "non-map top form → None");
    }
}
