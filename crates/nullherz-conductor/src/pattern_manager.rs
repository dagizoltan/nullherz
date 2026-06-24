use serde::{Serialize, Deserialize};
use nullherz_traits::{Command, TimestampedCommand, CommandProducer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternTrigger {
    pub beat: f64,
    pub node_idx: u32,
    pub pattern_idx: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SongArrangement {
    pub triggers: Vec<PatternTrigger>,
}

pub struct PatternManager {
    pub arrangement: SongArrangement,
    pub last_triggered_idx: usize,
    last_processed_beat: f64,
}

impl PatternManager {
    pub fn new() -> Self {
        Self {
            arrangement: SongArrangement::default(),
            last_triggered_idx: 0,
            last_processed_beat: 0.0,
        }
    }

    pub fn tick(&mut self, current_beat: f64, producer: &Option<Box<dyn CommandProducer>>) {
        if current_beat < self.last_processed_beat {
            // We jumped back, reset search.
            // In a more advanced version, we'd seek to the correct idx.
            self.last_triggered_idx = 0;
        }
        self.last_processed_beat = current_beat;

        if let Some(prod) = producer {
            while self.last_triggered_idx < self.arrangement.triggers.len() {
                let trigger = &self.arrangement.triggers[self.last_triggered_idx];
                if current_beat >= trigger.beat {
                    let _ = prod.push_command(TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::SetParam {
                            target_id: trigger.node_idx as u64,
                            param_id: 0, // ACTIVE_PATTERN
                            value: trigger.pattern_idx as f32,
                            ramp_duration_samples: 0,
                        },
                    });
                    self.last_triggered_idx += 1;
                } else {
                    break;
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.last_triggered_idx = 0;
        self.last_processed_beat = 0.0;
    }

    pub fn set_arrangement(&mut self, arrangement: SongArrangement) {
        self.arrangement = arrangement;
        // Sort triggers by beat to ensure correct sequential processing
        self.arrangement.triggers.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
        self.reset();
    }
}
