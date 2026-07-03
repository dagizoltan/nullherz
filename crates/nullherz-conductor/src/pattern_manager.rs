use serde::{Serialize, Deserialize};
use nullherz_traits::Command;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct ArrangementEvent {
    pub beat: f64,
    pub command: Command,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct SongArrangement {
    pub events: Vec<ArrangementEvent>,
}

pub struct PatternManager {
    pub arrangement: SongArrangement,
    pub last_triggered_idx: usize,
    last_processed_beat: f64,
}

impl Default for PatternManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternManager {
    pub fn new() -> Self {
        Self {
            arrangement: SongArrangement::default(),
            last_triggered_idx: 0,
            last_processed_beat: 0.0,
        }
    }

    pub fn tick(&mut self, current_beat: f64) -> Vec<Command> {
        let mut triggered = Vec::new();

        if current_beat < self.last_processed_beat {
            // We jumped back, reset search.
            // In a more advanced version, we'd seek to the correct idx.
            self.last_triggered_idx = 0;
        }
        self.last_processed_beat = current_beat;

        while self.last_triggered_idx < self.arrangement.events.len() {
            let event = &self.arrangement.events[self.last_triggered_idx];
            if current_beat >= event.beat {
                triggered.push(event.command);
                self.last_triggered_idx += 1;
            } else {
                break;
            }
        }
        triggered
    }

    pub fn reset(&mut self) {
        self.last_triggered_idx = 0;
        self.last_processed_beat = 0.0;
    }

    pub fn set_arrangement(&mut self, arrangement: SongArrangement) {
        self.arrangement = arrangement;
        // Sort events by beat to ensure correct sequential processing
        self.arrangement.events.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
        self.reset();
    }
}
