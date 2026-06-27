//! Dead-reckoning state estimator.
//!
//! Integrates body-frame inertial/odometry measurements — longitudinal
//! acceleration and yaw rate (what an IMU + wheel odometry give after gravity
//! compensation) — between sparse **absolute** pose fixes (GNSS / SLAM /
//! lidar-localisation), correcting toward each fix with a complementary filter.
//! This keeps a usable pose estimate when absolute localisation drops out, and
//! is the natural feed for `Autopilot::step` when the true pose isn't directly
//! observable.

use std::f32::consts::PI;

use crate::types::Pose2;

/// Unicycle dead-reckoning estimator with complementary-filter correction.
///
/// ```
/// use kami_autodrive::{StateEstimator, Pose2};
///
/// let mut est = StateEstimator::new(Pose2::new(0.0, 0.0, 0.0));
/// // 1 s of straight-line IMU at 2 m/s² (no turn).
/// for _ in 0..100 {
///     est.predict(2.0, 0.0, 1.0 / 100.0);
/// }
/// assert!((est.speed() - 2.0).abs() < 1e-3);
/// // An absolute fix snaps the estimate back onto truth.
/// est.correct(Pose2::new(0.9, 0.0, 0.0), 1.0);
/// assert!((est.pose().x - 0.9).abs() < 1e-4);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct StateEstimator {
    pose: Pose2,
    speed: f32,
}

impl StateEstimator {
    pub fn new(initial: Pose2) -> Self {
        Self {
            pose: initial,
            speed: 0.0,
        }
    }

    pub fn with_speed(initial: Pose2, speed: f32) -> Self {
        Self {
            pose: initial,
            speed,
        }
    }

    pub fn pose(&self) -> Pose2 {
        self.pose
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Propagate the estimate forward by `dt` under a body longitudinal accel
    /// (m/s²) and yaw rate (rad/s) — one IMU/odometry sample.
    pub fn predict(&mut self, accel: f32, yaw_rate: f32, dt: f32) {
        self.speed += accel * dt;
        self.pose.yaw += yaw_rate * dt;
        let (s, c) = self.pose.yaw.sin_cos();
        self.pose.x += self.speed * c * dt;
        self.pose.y += self.speed * s * dt;
    }

    /// Pull the estimate toward an absolute pose fix (`gain` ∈ [0,1]; 1 = snap
    /// to the fix, 0 = ignore). Yaw uses the shortest-arc blend.
    pub fn correct(&mut self, fix: Pose2, gain: f32) {
        let g = gain.clamp(0.0, 1.0);
        self.pose.x += (fix.x - self.pose.x) * g;
        self.pose.y += (fix.y - self.pose.y) * g;
        self.pose.yaw += wrap_pi(fix.yaw - self.pose.yaw) * g;
    }

    /// Blend a measured speed (e.g. wheel odometry) into the estimate.
    pub fn correct_speed(&mut self, measured: f32, gain: f32) {
        self.speed += (measured - self.speed) * gain.clamp(0.0, 1.0);
    }
}

/// Wrap an angle to (−π, π].
fn wrap_pi(a: f32) -> f32 {
    let mut x = a % (2.0 * PI);
    if x > PI {
        x -= 2.0 * PI;
    } else if x <= -PI {
        x += 2.0 * PI;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predict_reproduces_a_straight_run() {
        let mut e = StateEstimator::new(Pose2::new(0.0, 0.0, 0.0));
        let dt = 1.0 / 100.0;
        for _ in 0..100 {
            e.predict(2.0, 0.0, dt); // const accel, no turn, 1 s
        }
        // v = a·t = 2 m/s; x = ½·a·t² = 1 m.
        assert!((e.speed() - 2.0).abs() < 1e-3, "speed {}", e.speed());
        assert!((e.pose().x - 1.0).abs() < 0.05, "x {}", e.pose().x);
        assert!(e.pose().y.abs() < 1e-4);
    }

    #[test]
    fn correct_converges_to_the_fix() {
        let mut e = StateEstimator::new(Pose2::new(5.0, 5.0, 3.0));
        let fix = Pose2::new(0.0, 0.0, 0.0);
        for _ in 0..50 {
            e.correct(fix, 0.5);
        }
        assert!(e.pose().pos().distance(fix.pos()) < 1e-2, "{:?}", e.pose());
        assert!(e.pose().yaw.abs() < 1e-2, "yaw {}", e.pose().yaw);
    }

    #[test]
    fn yaw_correction_takes_the_short_way_round() {
        // Estimate at +3.0 rad, fix at −3.0 rad: shortest arc is +0.28 rad
        // (through ±π), not −6 rad.
        let mut e = StateEstimator::new(Pose2::new(0.0, 0.0, 3.0));
        e.correct(Pose2::new(0.0, 0.0, -3.0), 1.0);
        // Short arc: 3.0 + 0.283 = 3.283 (≡ −3.0 mod 2π), NOT the long −3.0.
        let y = e.pose().yaw;
        assert!(
            (y - 3.283).abs() < 0.05,
            "should wrap the short way, got {y}"
        );
    }
}
