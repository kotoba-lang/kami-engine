//! spatial.rs — 6-D spatial-vector ("Plücker") algebra for the 3-D
//! reduced-coordinate articulated-body solver (`articulation3d`).
//!
//! This is the clean-room building block for the same algorithm class NVIDIA
//! PhysX uses for its reduced-coordinate `Articulation` (Featherstone spatial
//! algebra). No NVIDIA / PhysX / Isaac code is linked or referenced — only the
//! textbook math (Featherstone, *Rigid Body Dynamics Algorithms*, 2008, Ch. 2).
//!
//! Conventions:
//!   - A spatial **motion** vector is `[ω(3); v(3)]` (angular on top, linear of
//!     the body-fixed point at the frame origin).
//!   - A spatial **force** vector is `[n(3); f(3)]` (moment on top, linear).
//!   - Transforms / inertias / cross-products are explicit 6×6 matrices. For
//!     the n ≤ ~8 DOF arms we target, the dense form costs nothing and is far
//!     easier to verify than the block-optimized variants. The decisive
//!     correctness gate is `articulation3d`'s planar cross-check against the
//!     independently-validated `planar_chain` solver.

use glam::{Mat3, Vec3};

/// Spatial vector (motion or force), `[a0,a1,a2, l0,l1,l2]`.
pub type Sv = [f32; 6];
/// 6×6 matrix, row-major.
pub type M6 = [[f32; 6]; 6];

pub const ZERO_SV: Sv = [0.0; 6];
pub const ZERO_M6: M6 = [[0.0; 6]; 6];

/// Skew-symmetric matrix `[v]×` such that `[v]× w = v × w`.
pub fn skew(v: Vec3) -> Mat3 {
    // glam Mat3 is column-major: from_cols(col0, col1, col2).
    Mat3::from_cols(
        Vec3::new(0.0, v.z, -v.y),
        Vec3::new(-v.z, 0.0, v.x),
        Vec3::new(v.y, -v.x, 0.0),
    )
}

fn m3(r: usize, c: usize, m: &Mat3) -> f32 {
    m.col(c)[r]
}

/// Assemble a 6×6 from four 3×3 blocks: `[[tl, tr], [bl, br]]`.
pub fn from_blocks(tl: Mat3, tr: Mat3, bl: Mat3, br: Mat3) -> M6 {
    let mut out = ZERO_M6;
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = m3(r, c, &tl);
            out[r][c + 3] = m3(r, c, &tr);
            out[r + 3][c] = m3(r, c, &bl);
            out[r + 3][c + 3] = m3(r, c, &br);
        }
    }
    out
}

pub fn sv(top: Vec3, bot: Vec3) -> Sv {
    [top.x, top.y, top.z, bot.x, bot.y, bot.z]
}
pub fn sv_top(s: &Sv) -> Vec3 {
    Vec3::new(s[0], s[1], s[2])
}
pub fn sv_bot(s: &Sv) -> Vec3 {
    Vec3::new(s[3], s[4], s[5])
}

pub fn mat_vec(m: &M6, v: &Sv) -> Sv {
    let mut out = ZERO_SV;
    for r in 0..6 {
        let mut s = 0.0;
        for c in 0..6 {
            s += m[r][c] * v[c];
        }
        out[r] = s;
    }
    out
}

pub fn mat_mul(a: &M6, b: &M6) -> M6 {
    let mut out = ZERO_M6;
    for r in 0..6 {
        for c in 0..6 {
            let mut s = 0.0;
            for k in 0..6 {
                s += a[r][k] * b[k][c];
            }
            out[r][c] = s;
        }
    }
    out
}

pub fn transpose(a: &M6) -> M6 {
    let mut out = ZERO_M6;
    for r in 0..6 {
        for c in 0..6 {
            out[c][r] = a[r][c];
        }
    }
    out
}

pub fn add(a: &M6, b: &M6) -> M6 {
    let mut out = ZERO_M6;
    for r in 0..6 {
        for c in 0..6 {
            out[r][c] = a[r][c] + b[r][c];
        }
    }
    out
}

pub fn dot(a: &Sv, b: &Sv) -> f32 {
    let mut s = 0.0;
    for i in 0..6 {
        s += a[i] * b[i];
    }
    s
}

pub fn axpy(scale: f32, x: &Sv, y: &Sv) -> Sv {
    let mut out = ZERO_SV;
    for i in 0..6 {
        out[i] = scale * x[i] + y[i];
    }
    out
}

/// Plücker **motion** transform `X` built from a child frame whose orientation
/// (mapping parent-frame vectors into child-frame vectors) is `e` and whose
/// origin sits at `r` expressed in the parent frame.
///
/// `X · m_parent = m_child`. Force transform up the tree is `Xᵀ`
/// (matrix transpose), per Featherstone — that is *exactly* the identity the
/// RNEA force pass and CRBA inertia pass rely on.
///
///   X = [[ E,        0 ],
///        [ -E·[r]×,  E ]]
pub fn plucker(e: Mat3, r: Vec3) -> M6 {
    let neg_e_rx = -(e * skew(r));
    from_blocks(e, Mat3::ZERO, neg_e_rx, e)
}

/// Inverse of a `plucker(e, r)` transform: `plucker(eᵀ, -e·r)`.
pub fn plucker_inv(e: Mat3, r: Vec3) -> M6 {
    plucker(e.transpose(), -(e * r))
}

/// Spatial inertia (6×6) of a body with mass `m`, centre of mass `c` (in the
/// frame the inertia is expressed in), and rotational inertia `i_c` about the
/// COM. Spatial momentum is `I · v`.
///
///   I = [[ i_c − m[c]×[c]× ,  m[c]× ],
///        [ −m[c]× ,           m·1₃ ]]
pub fn spatial_inertia(m: f32, c: Vec3, i_c: Mat3) -> M6 {
    let cx = skew(c);
    let tl = i_c - (cx * cx) * m; // parallel-axis term: −m[c]×[c]× = m(|c|²1 − ccᵀ)
    let tr = cx * m;
    let bl = cx * (-m);
    let br = Mat3::IDENTITY * m;
    from_blocks(tl, tr, bl, br)
}

/// Motion cross-product matrix `crm(v)` s.t. `crm(v)·s = v ×ₘ s`.
///
///   crm(v) = [[ [ω]×,  0   ],
///             [ [u]×,  [ω]× ]]   with v = [ω; u]
pub fn crm(v: &Sv) -> M6 {
    let w = skew(sv_top(v));
    let u = skew(sv_bot(v));
    from_blocks(w, Mat3::ZERO, u, w)
}

/// Force cross-product matrix `crf(v) = −crm(v)ᵀ` s.t. `crf(v)·f = v ×_f f`.
///
///   crf(v) = [[ [ω]×,  [u]× ],
///             [ 0,     [ω]× ]]
pub fn crf(v: &Sv) -> M6 {
    let w = skew(sv_top(v));
    let u = skew(sv_bot(v));
    from_blocks(w, u, Mat3::ZERO, w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn skew_is_cross_product() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(-4.0, 5.0, 6.0);
        let got = skew(a) * b;
        let want = a.cross(b);
        assert!((got - want).length() < 1e-6, "got {got:?} want {want:?}");
    }

    #[test]
    fn plucker_inverse_round_trips() {
        let e = Mat3::from_rotation_x(0.7) * Mat3::from_rotation_z(-0.4);
        let r = Vec3::new(0.3, -1.2, 0.8);
        let x = plucker(e, r);
        let xi = plucker_inv(e, r);
        let id = mat_mul(&x, &xi);
        for i in 0..6 {
            for j in 0..6 {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!(approx(id[i][j], want, 1e-5), "({i},{j})={}", id[i][j]);
            }
        }
    }

    #[test]
    fn point_mass_spatial_inertia_momentum() {
        // A point mass m at offset c, pure angular velocity ω about origin:
        // linear momentum = m (ω × c); angular momentum = m c × (ω × c).
        let m = 2.0;
        let c = Vec3::new(0.4, 0.0, 0.0);
        let inertia = spatial_inertia(m, c, Mat3::ZERO);
        let w = Vec3::new(0.0, 0.0, 1.5);
        let v = sv(w, Vec3::ZERO);
        let h = mat_vec(&inertia, &v);
        let lin = sv_bot(&h);
        let ang = sv_top(&h);
        let want_lin = (w.cross(c)) * m;
        let want_ang = c.cross(w.cross(c)) * m;
        assert!(
            (lin - want_lin).length() < 1e-5,
            "lin {lin:?} vs {want_lin:?}"
        );
        assert!(
            (ang - want_ang).length() < 1e-5,
            "ang {ang:?} vs {want_ang:?}"
        );
    }

    #[test]
    fn crf_is_neg_crm_transpose() {
        let v = sv(Vec3::new(0.2, -1.0, 0.5), Vec3::new(1.1, 0.3, -0.7));
        let a = crf(&v);
        let b = transpose(&crm(&v));
        for i in 0..6 {
            for j in 0..6 {
                assert!(approx(a[i][j], -b[i][j], 1e-6), "({i},{j})");
            }
        }
    }
}
