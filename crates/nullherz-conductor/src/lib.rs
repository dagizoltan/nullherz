use control_plane::TimestampedCommand;

pub struct Transport {
    bpm: f32,
    sample_rate: f32,
    current_sample: u64,
}

impl Transport {
    pub fn new(bpm: f32, sample_rate: f32) -> Self {
        Self { bpm, sample_rate, current_sample: 0 }
    }

    pub fn set_bpm(&mut self, bpm: f32) { self.bpm = bpm; }

    pub fn samples_per_beat(&self) -> f32 {
        (60.0 / self.bpm) * self.sample_rate
    }

    pub fn beat_to_sample(&self, beat: f32) -> u64 {
        (beat * self.samples_per_beat()) as u64
    }
}

pub struct Conductor {
    transport: Transport,
    command_queue: Vec<TimestampedCommand>,
}

impl Conductor {
    pub fn new(bpm: f32, sample_rate: f32) -> Self {
        Self {
            transport: Transport::new(bpm, sample_rate),
            command_queue: Vec::new(),
        }
    }

    pub fn schedule_event(&mut self, beat: f32, command: control_plane::Command) {
        let sample = self.transport.beat_to_sample(beat);
        self.command_queue.push(TimestampedCommand {
            timestamp_samples: sample,
            command,
        });
        // Keep queue sorted by timestamp
        self.command_queue.sort_by_key(|c| c.timestamp_samples);
    }

    pub fn get_pending_commands(&mut self, start_sample: u64, end_sample: u64) -> Vec<TimestampedCommand> {
        let mut pending = Vec::new();
        let mut i = 0;
        while i < self.command_queue.len() {
            if self.command_queue[i].timestamp_samples >= start_sample && self.command_queue[i].timestamp_samples < end_sample {
                pending.push(self.command_queue.remove(i));
            } else if self.command_queue[i].timestamp_samples < start_sample {
                // Command in the past, discard or handle
                self.command_queue.remove(i);
            } else {
                i += 1;
            }
        }
        pending
    }
}
