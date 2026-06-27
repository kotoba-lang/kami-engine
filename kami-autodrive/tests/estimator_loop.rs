//! Dead-reckoning robustness: a `StateEstimator` integrates noisy IMU between
//! sparse absolute fixes. Pure dead-reckoning drifts unboundedly; periodic
//! complementary correction keeps the estimate locked to ground truth.

use kami_autodrive::{BicycleModel, Command, Plant, Pose2, StateEstimator, VehicleClass};

/// Deterministic LCG → uniform noise in [-amp, amp].
fn noise(state: &mut u64, amp: f32) -> f32 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let u = ((*state >> 40) as f32) / ((1u64 << 24) as f32);
    (u * 2.0 - 1.0) * amp
}

#[test]
fn periodic_fixes_keep_dead_reckoning_bounded() {
    let dt = 1.0 / 50.0;
    let start = Pose2::new(0.0, 0.0, 0.0);

    // Ground-truth vehicle driving a gentle curve.
    let mut truth = BicycleModel::new(start, VehicleClass::Car.limits());
    let cmd = Command {
        throttle: 0.4,
        brake: 0.0,
        steer: 0.2,
        handbrake: 0.0,
        reverse: false,
    };

    let mut corrected = StateEstimator::new(start);
    let mut free = StateEstimator::new(start); // dead-reckon only, never corrected

    let mut rng: u64 = 0xfeed_face_dead_beef;
    let (accel_bias, gyro_bias) = (0.15, 0.03); // constant IMU bias

    let mut prev_speed = truth.speed();
    let mut prev_yaw = truth.pose().yaw;
    let mut max_corrected_err = 0.0f32;

    for step in 0..1000 {
        truth.step(cmd, dt);
        let t = truth.pose();

        // Synthesize a noisy IMU sample from the true motion.
        let true_accel = (truth.speed() - prev_speed) / dt;
        let true_yaw_rate = (t.yaw - prev_yaw) / dt;
        prev_speed = truth.speed();
        prev_yaw = t.yaw;
        let imu_accel = true_accel + accel_bias + noise(&mut rng, 0.4);
        let imu_yaw_rate = true_yaw_rate + gyro_bias + noise(&mut rng, 0.02);

        corrected.predict(imu_accel, imu_yaw_rate, dt);
        free.predict(imu_accel, imu_yaw_rate, dt);

        // Absolute fix once a second (every 50 ticks).
        if step % 50 == 49 {
            corrected.correct(t, 0.6);
            corrected.correct_speed(truth.speed(), 0.6);
        }

        if step > 100 {
            max_corrected_err = max_corrected_err.max(corrected.pose().pos().distance(t.pos()));
        }
    }

    let truth_pos = truth.pose().pos();
    let corrected_err = corrected.pose().pos().distance(truth_pos);
    let free_err = free.pose().pos().distance(truth_pos);

    // Corrected estimate stays locked on; free dead-reckoning has drifted far.
    assert!(
        max_corrected_err < 2.0,
        "corrected estimate should stay bounded (max err {max_corrected_err:.2} m)"
    );
    assert!(
        free_err > 3.0 * corrected_err.max(0.1),
        "uncorrected dead-reckoning should drift much further (free {free_err:.1} vs corrected {corrected_err:.2})"
    );
}
