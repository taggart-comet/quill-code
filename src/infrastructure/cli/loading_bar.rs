use std::time::Instant;

#[derive(Debug, Clone)]
pub struct LoadingBar {
    start: Instant,
    seconds_per_half: f64,
}

impl LoadingBar {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            seconds_per_half: 0.8,
        }
    }

    pub fn ratio(&self) -> f64 {
        let elapsed = self.start.elapsed().as_secs_f64();
        let phase = (elapsed / self.seconds_per_half) % 2.0;
        if phase <= 1.0 {
            phase
        } else {
            2.0 - phase
        }
    }
}
