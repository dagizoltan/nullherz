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
        // Sync conductor timeline with engine reality.
        // We use the engine's reported sample counter for ground truth.
        // Latency correction could be applied here by subtracting measured latency.
        let sample_rate = 44100.0; // Should be dynamic in a full implementation
        self.current_beat = telemetry.sample_counter as f64 / sample_rate * (self.bpm as f64 / 60.0);
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
