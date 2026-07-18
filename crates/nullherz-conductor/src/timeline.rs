pub struct Timeline {
    pub bpm: f32,
    pub sample_rate: f32,
    pub signature_num: u32,
    pub signature_den: u32,
    pub current_beat: f64,
    pub last_breeding_secs: u64,
    pub last_matchmaking_secs: u64,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            sample_rate: 44100.0,
            signature_num: 4,
            signature_den: 4,
            current_beat: 0.0,
            last_breeding_secs: 0,
            last_matchmaking_secs: 0,
        }
    }
}

impl Timeline {
    pub fn update(&mut self, telemetry: &audio_core::Telemetry) {
        // Sync conductor timeline with engine reality.
        // We use the engine's reported sample counter for ground truth.
        // Latency correction could be applied here by subtracting measured latency.
        self.current_beat = telemetry.sample_counter as f64 / self.sample_rate as f64 * (self.bpm as f64 / 60.0);
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.bpm = bpm;
    }

    pub fn set_signature(&mut self, num: u32, den: u32) {
        self.signature_num = num;
        self.signature_den = den;
    }

    pub fn quantize_beat(&self, beat: f64, grid: f64) -> f64 {
        (beat / grid).ceil() * grid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_derives_beat_from_engine_sample_counter() {
        let mut tl = Timeline { bpm: 120.0, sample_rate: 44100.0, ..Default::default() };
        let mut tel = audio_core::Telemetry::default();

        // 120 BPM = 2 beats/sec; 44100 samples = 1 second = 2 beats.
        tel.sample_counter = 44100;
        tl.update(&tel);
        assert!((tl.current_beat - 2.0).abs() < 1e-9);

        // Ground truth follows the engine, not accumulated local time.
        tel.sample_counter = 22050;
        tl.update(&tel);
        assert!((tl.current_beat - 1.0).abs() < 1e-9, "timeline must track the engine even backwards");
    }

    #[test]
    fn test_quantize_beat_snaps_up_to_grid() {
        let tl = Timeline::default();
        assert_eq!(tl.quantize_beat(4.1, 1.0), 5.0, "late by a little -> next grid line");
        assert_eq!(tl.quantize_beat(4.0, 1.0), 4.0, "exactly on the grid stays");
        assert_eq!(tl.quantize_beat(0.3, 0.25), 0.5, "fractional grids");
        assert_eq!(tl.quantize_beat(0.0, 4.0), 0.0, "origin stays at origin");
    }
}
