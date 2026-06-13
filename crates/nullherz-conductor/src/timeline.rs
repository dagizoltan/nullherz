pub struct Timeline {
    pub bpm: f32,
    pub signature_num: u32,
    pub signature_den: u32,
    pub current_beat: f64,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            signature_num: 4,
            signature_den: 4,
            current_beat: 0.0,
        }
    }
}

impl Timeline {
    pub fn update(&mut self, telemetry: &audio_core::Telemetry) {
        // Sync conductor timeline with engine reality
        self.current_beat = telemetry.sample_counter as f64 / 44100.0 * (self.bpm as f64 / 60.0);
    }

    pub fn quantize_beat(&self, beat: f64, grid: f64) -> f64 {
        (beat / grid).ceil() * grid
    }
}
