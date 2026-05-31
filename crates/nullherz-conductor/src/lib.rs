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

pub struct MidiBinding {
    pub node_id: u64,
    pub param_id: u32,
}

pub enum GridDivision {
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl GridDivision {
    pub fn to_beats(&self) -> f32 {
        match self {
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::ThirtySecond => 0.125,
        }
    }
}

pub struct Conductor {
    transport: Transport,
    command_queue: Vec<TimestampedCommand>,
    midi_map: std::collections::HashMap<(u8, u8), MidiBinding>, // (Channel, CC) -> Binding
    pub midi_learning: Option<(u64, u32)>, // Some(Node_ID, Param_ID) if in learn mode
    pub grid: GridDivision,
}

impl Conductor {
    pub fn new(bpm: f32, sample_rate: f32) -> Self {
        Self {
            transport: Transport::new(bpm, sample_rate),
            command_queue: Vec::new(),
            midi_map: std::collections::HashMap::new(),
            midi_learning: None,
            grid: GridDivision::Sixteenth,
        }
    }

    pub fn schedule_quantized(&mut self, current_beat: f32, command: control_plane::Command) {
        let division = self.grid.to_beats();
        let next_beat = ((current_beat / division).floor() + 1.0) * division;
        self.schedule_event(next_beat, command);
    }

    pub fn bind_midi_cc(&mut self, channel: u8, cc: u8, node_id: u64, param_id: u32) {
        self.midi_map.insert((channel, cc), MidiBinding { node_id, param_id });
    }

    pub fn handle_midi_cc(&mut self, channel: u8, cc: u8, value: u8) -> Option<control_plane::Command> {
        if let Some((node_id, param_id)) = self.midi_learning.take() {
            println!("MIDI LEARN: Bound (Ch {}, CC {}) to Node {}, Param {}", channel, cc, node_id, param_id);
            self.bind_midi_cc(channel, cc, node_id, param_id);
        }

        if let Some(binding) = self.midi_map.get(&(channel, cc)) {
            let normalized_val = value as f32 / 127.0;
            Some(control_plane::Command::SetParam {
                target_id: binding.node_id,
                param_id: binding.param_id,
                value: normalized_val,
                ramp_duration_samples: 0,
            })
        } else {
            None
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
