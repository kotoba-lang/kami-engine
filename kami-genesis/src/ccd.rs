//! ccd — continuous collision detection (time-of-impact) to stop tunnelling.
//!
//! Fast bodies (a dropped payload, a thrown part) can pass *through* a thin
//! obstacle in a single discrete step. CCD finds the time-of-impact (TOI) inside
//! the step so the integrator can stop at contact. Two routines, both CPU/WASM,
//! the same class PhysX exposes (`eABP`/conservative advancement):
//!
//! - `sphere_plane_toi` — analytic TOI of a translating sphere vs a half-space.
//! - `conservative_advancement_toi` — TOI of two translating convex polytopes,
//!   using `gjk_distance` as the proximity oracle (Mirtich's CA).
//!
//! Returns the impact fraction in `[0, 1]` of the step, or `None` if no impact.

use crate::convex::{ConvexPoly, gjk_distance};
use glam::Vec3;

impl ConvexPoly {
    /// A copy translated by `off` (CCD advances bodies by translation).
    pub fn translated(&self, off: Vec3) -> ConvexPoly {
        ConvexPoly {
            verts: self.verts.iter().map(|v| *v + off).collect(),
        }
    }
}

/// TOI (fraction of the step) of a sphere of `radius` at `center` moving by
/// `vel·dt` over the step, hitting the half-space `{x : n·x ≥ offset}` boundary
/// plane `n·x = offset` (n unit). `None` if it does not reach the plane.
pub fn sphere_plane_toi(
    center: Vec3,
    radius: f32,
    vel: Vec3,
    n: Vec3,
    offset: f32,
    dt: f32,
) -> Option<f32> {
    let n = n.normalize_or_zero();
    let d0 = n.dot(center) - offset - radius; // signed gap to the surface
    let closing = n.dot(vel) * dt; // change in n·center over the step
    if d0 <= 0.0 {
        return Some(0.0); // already touching/penetrating
    }
    if closing >= -1e-9 {
        return None; // moving away or parallel
    }
    let t = d0 / (-closing); // fraction where gap reaches 0
    if (0.0..=1.0).contains(&t) {
        Some(t)
    } else {
        None
    }
}

/// Conservative advancement TOI of two convex polytopes translating by `va·dt`
/// and `vb·dt` over the step. `None` if they never get within `margin`.
pub fn conservative_advancement_toi(
    a: &ConvexPoly,
    b: &ConvexPoly,
    va: Vec3,
    vb: Vec3,
    dt: f32,
    margin: f32,
) -> Option<f32> {
    let rel = (va - vb) * dt; // relative displacement over the whole step
    let speed = rel.length();
    if speed < 1e-9 {
        return None; // no relative motion
    }
    let mut t = 0.0_f32;
    for _ in 0..64 {
        let at = a.translated(rel * t);
        let dist = gjk_distance(&at, b);
        if dist <= margin {
            return Some(t);
        }
        // advance by the most we can without skipping contact: the closing speed
        // is bounded by `speed`, so a step of (dist−margin)/speed is safe.
        let adv = (dist - margin) / speed;
        t += adv.max(1e-4);
        if t >= 1.0 {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    #[test]
    fn fast_sphere_does_not_tunnel_plane() {
        // sphere at z=5 falling at 100 m/s; dt=0.1 ⇒ would move −10 (through z=0).
        let toi = sphere_plane_toi(
            Vec3::new(0.0, 0.0, 5.0),
            0.5,
            Vec3::new(0.0, 0.0, -100.0),
            Vec3::Z,
            0.0,
            0.1,
        );
        let t = toi.expect("impact");
        // gap 4.5, closing 10 ⇒ t = 0.45
        assert!((t - 0.45).abs() < 1e-3, "toi={t}");
    }

    #[test]
    fn slow_sphere_no_impact_this_step() {
        let toi = sphere_plane_toi(
            Vec3::new(0.0, 0.0, 5.0),
            0.5,
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::Z,
            0.0,
            0.1,
        );
        assert!(toi.is_none());
    }

    #[test]
    fn ca_fast_box_finds_toi() {
        // a at x=−5 moving +x at 100; b fixed at x=0. Surfaces gap = 5−1 = 4.
        let a = ConvexPoly::box_at(Vec3::new(-5.0, 0.0, 0.0), Vec3::splat(0.5), Quat::IDENTITY);
        let b = ConvexPoly::box_at(Vec3::ZERO, Vec3::splat(0.5), Quat::IDENTITY);
        let toi =
            conservative_advancement_toi(&a, &b, Vec3::new(100.0, 0.0, 0.0), Vec3::ZERO, 0.1, 0.02);
        let t = toi.expect("impact");
        // closing 10 over the step, gap 4 ⇒ t ≈ 0.4
        assert!((t - 0.4).abs() < 0.05, "toi={t}");
    }

    #[test]
    fn ca_slow_box_no_impact() {
        let a = ConvexPoly::box_at(Vec3::new(-5.0, 0.0, 0.0), Vec3::splat(0.5), Quat::IDENTITY);
        let b = ConvexPoly::box_at(Vec3::ZERO, Vec3::splat(0.5), Quat::IDENTITY);
        let toi =
            conservative_advancement_toi(&a, &b, Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO, 0.1, 0.02);
        assert!(toi.is_none(), "unexpected toi={toi:?}");
    }

    #[test]
    fn ca_toi_depends_only_on_relative_velocity() {
        // Galilean invariance — the property that defines a correct CA: the TOI
        // (a step fraction) must be identical for any velocity split with the
        // same relative velocity, since CA solves in the other body's frame.
        let a = ConvexPoly::box_at(Vec3::new(-5.0, 0.0, 0.0), Vec3::splat(0.5), Quat::IDENTITY);
        let b = ConvexPoly::box_at(Vec3::ZERO, Vec3::splat(0.5), Quat::IDENTITY);
        let (dt, margin) = (0.1, 0.02);
        let only_a = conservative_advancement_toi(
            &a,
            &b,
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::ZERO,
            dt,
            margin,
        )
        .expect("impact");
        // both bodies moving, same relative velocity (+100 x):
        let split = conservative_advancement_toi(
            &a,
            &b,
            Vec3::new(60.0, 0.0, 0.0),
            Vec3::new(-40.0, 0.0, 0.0),
            dt,
            margin,
        )
        .expect("impact");
        // only b moving toward a (relative velocity still +100 x):
        let only_b = conservative_advancement_toi(
            &a,
            &b,
            Vec3::ZERO,
            Vec3::new(-100.0, 0.0, 0.0),
            dt,
            margin,
        )
        .expect("impact");
        assert!(
            (only_a - split).abs() < 1e-3,
            "split differs: {only_a} vs {split}"
        );
        assert!(
            (only_a - only_b).abs() < 1e-3,
            "b-move differs: {only_a} vs {only_b}"
        );
    }

    #[test]
    fn ca_toi_handles_diagonal_approach() {
        // a closes on b along the xy diagonal; the AABB faces meet on both axes
        // at the same fraction (per-axis gap 4, closing 10/step) ⇒ corner contact
        // at TOI ≈ 0.4 — exercises a 2-axis relative displacement, not single-axis.
        let a = ConvexPoly::box_at(Vec3::new(-5.0, -5.0, 0.0), Vec3::splat(0.5), Quat::IDENTITY);
        let b = ConvexPoly::box_at(Vec3::ZERO, Vec3::splat(0.5), Quat::IDENTITY);
        let toi = conservative_advancement_toi(
            &a,
            &b,
            Vec3::new(100.0, 100.0, 0.0),
            Vec3::ZERO,
            0.1,
            0.02,
        )
        .expect("impact");
        assert!((toi - 0.4).abs() < 0.05, "toi={toi}");
    }
}
