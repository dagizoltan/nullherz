use audio_core::AudioProcessor;

pub struct BroadcastSidecar {
    is_active: bool,
    #[allow(dead_code)]
    sample_rate: f32,
}

impl BroadcastSidecar {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            is_active: false,
            sample_rate,
        }
    }
}

impl AudioProcessor for BroadcastSidecar {
    fn process(&mut self, inputs: &[&[f32]], _out: &mut [&mut [f32]]) {
        if !self.is_active || inputs.is_empty() { return; }

        // In a full implementation, we would encode the buffers to MP3/Opus
        // and push them to a TCP/UDP socket or a local icecast mount.
        // For now, we simulate activity.
    }

    fn apply_command(&mut self, cmd: &control_plane::Command) {
        match cmd {
            control_plane::Command::Play => self.is_active = true,
            control_plane::Command::Stop => self.is_active = false,
            _ => {}
        }
    }
}
