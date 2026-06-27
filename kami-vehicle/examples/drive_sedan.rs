//! 4-second wide-open-throttle launch on flat ground; prints telemetry.
//!
//!     cargo run -p kami-vehicle --example drive_sedan

use kami_vehicle::{
    ground::FlatGround,
    models::sedan::{SedanSpec, sedan},
};

fn main() {
    let mut car = sedan(&SedanSpec::default());
    let ground = FlatGround::new(0.0);
    car.powertrain.gearbox.shift_to(1);
    car.controls.throttle = 1.0;
    car.controls.clutch_pedal = 0.0;
    car.controls.steer = 0.0;

    println!(
        "{:>5} {:>9} {:>9} {:>9} {:>9}",
        "t", "speed_kmh", "rpm", "wheel_l_w", "wheel_r_w"
    );
    let dt = 1.0 / 60.0;
    for step in 0..240 {
        car.step(dt, &ground);
        if step % 12 == 0 {
            let kmh = car.speed() * 3.6;
            let rpm = car.engine_rpm();
            let wl = car
                .wheels
                .first()
                .map(|w| w.angular_velocity)
                .unwrap_or(0.0);
            let wr = car.wheels.get(1).map(|w| w.angular_velocity).unwrap_or(0.0);
            println!(
                "{:>5.2} {:>9.2} {:>9.0} {:>9.2} {:>9.2}",
                step as f32 * dt,
                kmh,
                rpm,
                wl,
                wr
            );
        }
        // Auto-shift up when revs hit 6000.
        if car.engine_rpm() > 6000.0
            && car.powertrain.gearbox.current_gear < 6
            && car.powertrain.gearbox.shift_progress >= 1.0
        {
            car.powertrain
                .gearbox
                .shift_to(car.powertrain.gearbox.current_gear + 1);
        }
    }
}
