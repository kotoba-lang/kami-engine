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
        ((curvature * self.turn_radius_ref).clamp(-1.0, 1.0), target_idx)
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
        Self { kp, ki, kd, integral: 0.0, prev_err: 0.0 }
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
        let deriv = if dt > 0.0 { (err - self.prev_err) / dt } else { 0.0 };
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
