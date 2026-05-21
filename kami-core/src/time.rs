//! Fixed-timestep game loop timing.

use crate::Tick;

pub struct GameClock {
    tick: Tick,
    tick_rate: u32,        // ticks per second (e.g. 60)
    tick_duration_ns: u64, // nanoseconds per tick
    accumulator_ns: u64,
}

impl GameClock {
    pub fn new(tick_rate: u32) -> Self {
        Self {
            tick: 0,
            tick_rate,
            tick_duration_ns: 1_000_000_000 / tick_rate as u64,
            accumulator_ns: 0,
        }
    }

    /// Feed elapsed nanoseconds. Returns number of ticks to simulate.
    pub fn advance(&mut self, elapsed_ns: u64) -> u32 {
        self.accumulator_ns += elapsed_ns;
        let ticks = (self.accumulator_ns / self.tick_duration_ns) as u32;
        self.accumulator_ns %= self.tick_duration_ns;
        self.tick = self.tick.wrapping_add(ticks);
        ticks
    }

    pub fn tick(&self) -> Tick {
        self.tick
    }

    pub fn tick_rate(&self) -> u32 {
        self.tick_rate
    }

    /// Interpolation alpha for rendering between ticks (0.0 .. 1.0).
    pub fn alpha(&self) -> f32 {
        self.accumulator_ns as f32 / self.tick_duration_ns as f32
    }
}
