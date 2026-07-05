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
