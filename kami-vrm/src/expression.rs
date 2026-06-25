//! ExpressionManager — resolve VRM expression weights into the concrete per-
//! frame changes the renderer applies: morph-target weights, material-colour
//! overrides, UV transforms, and the blink/lookAt/mouth override state.
//!
//! `kami-vrm` already *parses* expressions (morph/material/UV binds + override
//! flags); this is the runtime applier (the `@pixiv/three-vrm`
//! `VRMExpressionManager` analogue). Given a set of expression weights — driven
//! from EDN (e.g. a dance scene's `:morphs`/expression state) — it accumulates
//! the binds and applies VRM 1.0 override semantics so e.g. a `happy` expression
//! that `Block`s blink suppresses the blink track this frame.

use std::collections::BTreeMap;

use crate::vrm_types::{ExpressionPreset, OverrideType, VrmExpression};

/// The accumulated material-colour change for one (material, property).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorOverride {
    /// Weight-normalised target colour.
    pub target: [f32; 4],
    /// Total applied weight in [0,1] — host blends `base.lerp(target, weight)`.
    pub weight: f32,
}

/// The accumulated UV transform for one material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvOverride {
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub weight: f32,
}

/// The resolved expression state for a frame.
#[derive(Debug, Clone, Default)]
pub struct ResolvedExpression {
    /// `(mesh_index, morph_index)` → accumulated morph weight.
    pub morphs: BTreeMap<(usize, usize), f32>,
    /// `(material_index, property)` → colour override.
    pub material_colors: BTreeMap<(usize, String), ColorOverride>,
    /// `material_index` → UV transform override.
    pub uv_transforms: BTreeMap<usize, UvOverride>,
    /// Multiplier on the procedural blink (and blink-preset expressions): 1 =
    /// full, 0 = fully blocked by an active overriding expression.
    pub blink_factor: f32,
    pub lookat_factor: f32,
    pub mouth_factor: f32,
}

fn is_blink(p: Option<ExpressionPreset>) -> bool {
    matches!(
        p,
        Some(ExpressionPreset::Blink | ExpressionPreset::BlinkLeft | ExpressionPreset::BlinkRight)
    )
}
fn is_lookat(p: Option<ExpressionPreset>) -> bool {
    matches!(
        p,
        Some(
            ExpressionPreset::LookUp
                | ExpressionPreset::LookDown
                | ExpressionPreset::LookLeft
                | ExpressionPreset::LookRight
        )
    )
}
fn is_mouth(p: Option<ExpressionPreset>) -> bool {
    matches!(
        p,
        Some(
            ExpressionPreset::Aa
                | ExpressionPreset::Ih
                | ExpressionPreset::Ou
                | ExpressionPreset::Ee
                | ExpressionPreset::Oh
        )
    )
}

/// Resolves expression weights against a VRM's expression definitions.
pub struct ExpressionManager<'a> {
    expressions: &'a [VrmExpression],
}

impl<'a> ExpressionManager<'a> {
    pub fn new(expressions: &'a [VrmExpression]) -> Self {
        Self { expressions }
    }

    /// Look up an expression by name.
    fn find(&self, name: &str) -> Option<&VrmExpression> {
        self.expressions.iter().find(|e| e.name == name)
    }

    /// Resolve `weights` (expression name → [0,1]) into the per-frame changes.
    pub fn resolve(&self, weights: &BTreeMap<String, f32>) -> ResolvedExpression {
        // 1) override factors from active expressions that override a category.
        let mut blink_factor = 1.0f32;
        let mut lookat_factor = 1.0f32;
        let mut mouth_factor = 1.0f32;
        let apply = |factor: &mut f32, ov: Option<OverrideType>, w: f32| match ov {
            Some(OverrideType::Block) => {
                if w > 0.0 {
                    *factor = 0.0;
                }
            }
            Some(OverrideType::Blend) => *factor *= 1.0 - w.clamp(0.0, 1.0),
            _ => {}
        };
        for (name, &w) in weights {
            if w <= 0.0 {
                continue;
            }
            if let Some(e) = self.find(name) {
                apply(&mut blink_factor, e.override_blink, w);
                apply(&mut lookat_factor, e.override_look_at, w);
                apply(&mut mouth_factor, e.override_mouth, w);
            }
        }

        // 2) accumulate binds with the category factor applied.
        let mut out = ResolvedExpression {
            blink_factor,
            lookat_factor,
            mouth_factor,
            ..Default::default()
        };
        // material colour / UV accumulate weighted then normalise.
        let mut color_acc: BTreeMap<(usize, String), ([f32; 4], f32)> = BTreeMap::new();
        let mut uv_acc: BTreeMap<usize, ([f32; 2], [f32; 2], f32)> = BTreeMap::new();

        for (name, &raw) in weights {
            let Some(e) = self.find(name) else { continue };
            let mut w = raw.clamp(0.0, 1.0);
            if is_blink(e.preset) {
                w *= blink_factor;
            } else if is_lookat(e.preset) {
                w *= lookat_factor;
            } else if is_mouth(e.preset) {
                w *= mouth_factor;
            }
            if e.is_binary {
                w = if w >= 0.5 { 1.0 } else { 0.0 };
            }
            if w <= 0.0 {
                continue;
            }
            for b in &e.morph_target_binds {
                *out.morphs.entry((b.mesh_index, b.morph_index)).or_insert(0.0) += w * b.weight;
            }
            for b in &e.material_color_binds {
                let entry = color_acc.entry((b.material_index, b.property.clone())).or_insert(([0.0; 4], 0.0));
                for i in 0..4 {
                    entry.0[i] += b.target_value[i] * w;
                }
                entry.1 += w;
            }
            for b in &e.texture_transform_binds {
                let entry = uv_acc.entry(b.material_index).or_insert(([0.0; 2], [0.0; 2], 0.0));
                for i in 0..2 {
                    entry.0[i] += b.offset[i] * w;
                    entry.1[i] += b.scale[i] * w;
                }
                entry.2 += w;
            }
        }

        for (k, (sum, tw)) in color_acc {
            if tw > 0.0 {
                out.material_colors.insert(
                    k,
                    ColorOverride {
                        target: [sum[0] / tw, sum[1] / tw, sum[2] / tw, sum[3] / tw],
                        weight: tw.min(1.0),
                    },
                );
            }
        }
        for (k, (off, scl, tw)) in uv_acc {
            if tw > 0.0 {
                out.uv_transforms.insert(
                    k,
                    UvOverride {
                        offset: [off[0] / tw, off[1] / tw],
                        scale: [scl[0] / tw, scl[1] / tw],
                        weight: tw.min(1.0),
                    },
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vrm_types::{MaterialColorBind, MorphTargetBind, TextureTransformBind};

    fn expr(
        name: &str,
        preset: Option<ExpressionPreset>,
        morphs: Vec<(usize, usize, f32)>,
        ov_blink: Option<OverrideType>,
    ) -> VrmExpression {
        VrmExpression {
            name: name.into(),
            preset,
            is_binary: false,
            morph_target_binds: morphs
                .into_iter()
                .map(|(m, i, w)| MorphTargetBind { mesh_index: m, morph_index: i, weight: w })
                .collect(),
            material_color_binds: vec![],
            texture_transform_binds: vec![],
            override_blink: ov_blink,
            override_look_at: None,
            override_mouth: None,
        }
    }

    fn weights(pairs: &[(&str, f32)]) -> BTreeMap<String, f32> {
        pairs.iter().map(|(n, w)| (n.to_string(), *w)).collect()
    }

    #[test]
    fn accumulates_morph_binds_by_weight() {
        let exprs = vec![
            expr("happy", Some(ExpressionPreset::Happy), vec![(0, 1, 1.0), (0, 2, 0.5)], None),
            expr("aa", Some(ExpressionPreset::Aa), vec![(0, 5, 1.0)], None),
        ];
        let mgr = ExpressionManager::new(&exprs);
        let r = mgr.resolve(&weights(&[("happy", 0.5), ("aa", 1.0)]));
        assert!((r.morphs[&(0, 1)] - 0.5).abs() < 1e-6); // happy@0.5 × bind 1.0
        assert!((r.morphs[&(0, 2)] - 0.25).abs() < 1e-6); // happy@0.5 × bind 0.5
        assert!((r.morphs[&(0, 5)] - 1.0).abs() < 1e-6); // aa@1.0 × bind 1.0
    }

    #[test]
    fn block_override_suppresses_blink() {
        // a "surprised" expression that Blocks blink, active alongside blink.
        let exprs = vec![
            expr("surprised", Some(ExpressionPreset::Surprised), vec![], Some(OverrideType::Block)),
            expr("blink", Some(ExpressionPreset::Blink), vec![(0, 9, 1.0)], None),
        ];
        let mgr = ExpressionManager::new(&exprs);
        let r = mgr.resolve(&weights(&[("surprised", 1.0), ("blink", 1.0)]));
        assert_eq!(r.blink_factor, 0.0, "blink blocked");
        assert!(r.morphs.get(&(0, 9)).copied().unwrap_or(0.0) < 1e-6, "blink morph suppressed");
    }

    #[test]
    fn blend_override_attenuates_blink() {
        let exprs = vec![
            expr("happy", Some(ExpressionPreset::Happy), vec![], Some(OverrideType::Blend)),
            expr("blink", Some(ExpressionPreset::Blink), vec![(0, 9, 1.0)], None),
        ];
        let mgr = ExpressionManager::new(&exprs);
        let r = mgr.resolve(&weights(&[("happy", 0.25), ("blink", 1.0)]));
        assert!((r.blink_factor - 0.75).abs() < 1e-6, "blink attenuated by 1-0.25");
        assert!((r.morphs[&(0, 9)] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn material_color_and_uv_resolve() {
        let mut e = expr("angry", Some(ExpressionPreset::Angry), vec![], None);
        e.material_color_binds = vec![MaterialColorBind {
            material_index: 2,
            property: "emissionColor".into(),
            target_value: [1.0, 0.0, 0.0, 1.0],
        }];
        e.texture_transform_binds = vec![TextureTransformBind {
            material_index: 2,
            offset: [0.1, 0.0],
            scale: [1.0, 1.0],
        }];
        let mgr = ExpressionManager::new(std::slice::from_ref(&e));
        let r = mgr.resolve(&weights(&[("angry", 0.5)]));
        let c = r.material_colors[&(2, "emissionColor".into())];
        assert_eq!(c.target, [1.0, 0.0, 0.0, 1.0]);
        assert!((c.weight - 0.5).abs() < 1e-6);
        let uv = r.uv_transforms[&2];
        assert!((uv.offset[0] - 0.1).abs() < 1e-6);
        assert!((uv.weight - 0.5).abs() < 1e-6);
    }

    #[test]
    fn binary_expression_snaps() {
        let exprs = vec![expr("blink", Some(ExpressionPreset::Blink), vec![(0, 9, 1.0)], None)];
        let mut binary = exprs;
        binary[0].is_binary = true;
        let mgr = ExpressionManager::new(&binary);
        assert!(mgr.resolve(&weights(&[("blink", 0.4)])).morphs.get(&(0, 9)).is_none(), "below 0.5 → off");
        assert!((mgr.resolve(&weights(&[("blink", 0.6)])).morphs[&(0, 9)] - 1.0).abs() < 1e-6, "above 0.5 → on");
    }

    #[test]
    fn unknown_expression_is_ignored() {
        let exprs = vec![expr("happy", Some(ExpressionPreset::Happy), vec![(0, 1, 1.0)], None)];
        let mgr = ExpressionManager::new(&exprs);
        let r = mgr.resolve(&weights(&[("nonexistent", 1.0)]));
        assert!(r.morphs.is_empty());
        assert_eq!(r.blink_factor, 1.0);
    }
}
