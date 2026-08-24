use std::time::Duration;

/// Accumulator-based fixed-step clock. It prevents display refresh rate from
/// changing the order or count of physics/Lua updates.
#[derive(Debug, Clone)]
pub struct FixedClock {
    step: Duration,
    accumulator: Duration,
    maximum_steps: u32,
}

impl FixedClock {
    pub fn new(hertz: u32, maximum_steps: u32) -> Self {
        assert!(hertz > 0);
        Self {
            step: Duration::from_secs_f64(1.0 / hertz as f64),
            accumulator: Duration::ZERO,
            maximum_steps,
        }
    }

    pub fn advance(&mut self, elapsed: Duration, mut update: impl FnMut(Duration)) -> u32 {
        self.accumulator = self.accumulator.saturating_add(elapsed);
        let mut steps = 0;
        while self.accumulator >= self.step && steps < self.maximum_steps {
            update(self.step);
            self.accumulator -= self.step;
            steps += 1;
        }
        if steps == self.maximum_steps && self.accumulator >= self.step {
            self.accumulator = self.accumulator.min(self.step);
        }
        steps
    }

    pub fn interpolation_alpha(&self) -> f32 {
        (self.accumulator.as_secs_f64() / self.step.as_secs_f64()) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_at_a_fixed_rate() {
        let mut clock = FixedClock::new(60, 8);
        let mut calls = 0;
        let steps = clock.advance(Duration::from_millis(51), |_| calls += 1);
        assert_eq!(steps, 3);
        assert_eq!(calls, 3);
    }
}
