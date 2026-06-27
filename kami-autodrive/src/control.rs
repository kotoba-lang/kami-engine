//! Tracking controllers: pure-pursuit lateral + PID longitudinal.

use glam::Vec2;

use crate::types::Pose2;

/// Pure-pursuit lateral controller. Picks a lookahead point on the path and
/// commands a normalised steer that arcs the vehicle onto it.
///
/// The steer is `curvature × turn_radius_ref` (full deflection when the
/// required arc radius equals the vehicle's reference turn radius). Decoupling
/// from a bicycle wheelbase lets the same controller drive the car, a rudder
/// ship, a banking aircraft, and a yawing multirotor. A target abeam-or-behind
/// commands a hard turn toward it, so plants that can slow/stop recover instead
/// of flying straight off.
#[derive(Debug, Clone, Copy)]
pub struct PurePursuit {
    /// Base lookahead distance (m).
    pub lookahead: f32,
    /// Extra lookahead per m/s of speed (s).
    pub lookahead_gain: f32,
    /// Reference (≈ minimum practical) turning radius (m).
    pub turn_radius_ref: f32,
}

impl PurePursuit {
    /// Result: normalised steer in `[-1, 1]` (positive = left) and the index of
    /// the waypoint used as the target. Returns `(0, last)` for a trivial path.
    pub fn steer(&self, pose: Pose2, speed: f32, path: &[Vec2]) -> (f32, usize) {
        if path.len() < 2 {
            return (0.0, path.len().saturating_sub(1));
        }
        let ld = (self.lookahead + self.lookahead_gain * speed).max(0.5);
        let target_idx = self.lookahead_target(pose.pos(), path, ld);
        let target = path[target_idx];

        // Target in body frame: +x forward, +y left.
        let local = pose.to_local(target);
        if local.x <= 0.0 {
            // Abeam or behind — arc geometry is invalid; turn hard toward it.
            let dir = if local.y >= 0.0 { 1.0 } else { -1.0 };
            return (dir, target_idx);
        }
        let ld2 = local.length_squared().max(1e-3);
        // Curvature of the arc through the origin to `local`: kappa = 2*y / Ld^2.
        let curvature = 2.0 * local.y / ld2;
        (
            (curvature * self.turn_radius_ref).clamp(-1.0, 1.0),
            target_idx,
        )
    }

    /// First waypoint at least `ld` ahead of `pos` along the path; falls back to
    /// the closest-then-forward waypoint, and finally the last point.
    fn lookahead_target(&self, pos: Vec2, path: &[Vec2], ld: f32) -> usize {
        // Index of the closest waypoint, to start the forward scan.
        let mut closest = 0;
        let mut best = f32::INFINITY;
        for (i, p) in path.iter().enumerate() {
            let d = p.distance_squared(pos);
            if d < best {
                best = d;
                closest = i;
            }
        }
        for (i, p) in path.iter().enumerate().skip(closest) {
            if p.distance(pos) >= ld {
                return i;
            }
        }
        path.len() - 1
    }
}

/// PID longitudinal controller mapping speed error to throttle/brake.
#[derive(Debug, Clone)]
pub struct SpeedController {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    integral: f32,
    prev_err: f32,
}

impl SpeedController {
    pub fn new(kp: f32, ki: f32, kd: f32) -> Self {
        Self {
            kp,
            ki,
            kd,
            integral: 0.0,
            prev_err: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.prev_err = 0.0;
    }

    /// Returns `(throttle, brake)`, each in `[0, 1]`. A positive control effort
    /// is throttle; negative is brake.
    pub fn update(&mut self, target_speed: f32, current_speed: f32, dt: f32) -> (f32, f32) {
        let err = target_speed - current_speed;
        self.integral = (self.integral + err * dt).clamp(-5.0, 5.0);
        let deriv = if dt > 0.0 {
            (err - self.prev_err) / dt
        } else {
            0.0
        };
        self.prev_err = err;
        let effort = self.kp * err + self.ki * self.integral + self.kd * deriv;
        if effort >= 0.0 {
            (effort.clamp(0.0, 1.0), 0.0)
        } else {
            (0.0, (-effort).clamp(0.0, 1.0))
        }
    }
}

/// Speed limit from path curvature: slower through tight turns.
/// `lateral_accel_limit` is the comfort/grip cap (m/s²).
pub fn curvature_speed_limit(path: &[Vec2], idx: usize, lateral_accel_limit: f32) -> f32 {
    if path.len() < 3 || idx == 0 || idx >= path.len() - 1 {
        return f32::INFINITY;
    }
    let a = path[idx - 1];
    let b = path[idx];
    let c = path[idx + 1];
    let kappa = menger_curvature(a, b, c);
    if kappa < 1e-4 {
        return f32::INFINITY;
    }
    (lateral_accel_limit / kappa).sqrt()
}

/// Menger curvature of the triangle (a, b, c) = 1 / circumradius.
fn menger_curvature(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    let area2 = ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs();
    let denom = a.distance(b) * b.distance(c) * c.distance(a);
    if denom < 1e-6 {
        0.0
    } else {
        2.0 * area2 / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pp() -> PurePursuit {
        PurePursuit {
            lookahead: 3.0,
            lookahead_gain: 0.0,
            turn_radius_ref: 4.0,
        }
    }

    #[test]
    fn pursuit_steers_left_toward_a_left_target() {
        let pose = Pose2::new(0.0, 0.0, 0.0); // facing +x
        let path = [Vec2::new(0.0, 0.0), Vec2::new(5.0, 5.0)];
        let (steer, _) = pp().steer(pose, 0.0, &path);
        assert!(
            steer > 0.0,
            "left target → positive (left) steer, got {steer}"
        );
    }

    #[test]
    fn pursuit_hard_turns_when_target_is_behind() {
        let pose = Pose2::new(0.0, 0.0, 0.0); // facing +x
        let left_behind = [Vec2::new(0.0, 0.0), Vec2::new(-5.0, 1.0)];
        let right_behind = [Vec2::new(0.0, 0.0), Vec2::new(-5.0, -1.0)];
        assert_eq!(pp().steer(pose, 0.0, &left_behind).0, 1.0);
        assert_eq!(pp().steer(pose, 0.0, &right_behind).0, -1.0);
    }

    #[test]
    fn speed_controller_throttles_then_brakes() {
        let mut sc = SpeedController::new(0.6, 0.0, 0.0);
        let (thr, brk) = sc.update(10.0, 0.0, 0.1);
        assert!(thr > 0.0 && brk == 0.0, "under-speed → throttle");
        let (thr, brk) = sc.update(0.0, 5.0, 0.1);
        assert!(thr == 0.0 && brk > 0.0, "over-speed → brake");
    }

    #[test]
    fn menger_curvature_matches_known_values() {
        // Collinear → zero curvature.
        let k0 = menger_curvature(
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(2.0, 0.0),
        );
        assert!(k0 < 1e-6, "collinear curvature {k0}");
        // Three points on a radius-2 circle → curvature 0.5.
        let k = menger_curvature(
            Vec2::new(2.0, 0.0),
            Vec2::new(0.0, 2.0),
            Vec2::new(-2.0, 0.0),
        );
        assert!((k - 0.5).abs() < 1e-5, "R=2 circle curvature {k}");
    }

    #[test]
    fn curvature_speed_limit_slows_in_a_bend() {
        let path = [
            Vec2::new(2.0, 0.0),
            Vec2::new(0.0, 2.0),
            Vec2::new(-2.0, 0.0),
        ];
        let v = curvature_speed_limit(&path, 1, 3.0); // a_lat=3, kappa=0.5
        assert!((v - (3.0f32 / 0.5).sqrt()).abs() < 1e-4, "v={v}");
        // A straight (degenerate) path imposes no limit.
        let straight = [Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0)];
        assert!(curvature_speed_limit(&straight, 0, 3.0).is_infinite());
    }
}
