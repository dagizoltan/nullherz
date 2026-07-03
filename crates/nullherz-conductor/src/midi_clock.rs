use std::time::{Instant, Duration};

pub struct MidiClockTracker {
    last_clock_time: Option<Instant>,
    clock_intervals: Vec<Duration>,
    pub current_bpm: f32,
    pub clock_count: u32,
    pub is_running: bool,
}

impl MidiClockTracker {
    pub fn new() -> Self {
        Self {
            last_clock_time: None,
            clock_intervals: Vec::with_capacity(24),
            current_bpm: 120.0,
            clock_count: 0,
            is_running: false,
        }
    }

    pub fn handle_event(&mut self, status: u8) -> Option<f32> {
        match status {
            0xF8 => { // Timing Clock
                let now = Instant::now();
                if let Some(last) = self.last_clock_time {
                    let interval = now.duration_since(last);
                    if interval.as_millis() < 1000 { // Ignore gaps > 1s
                        self.clock_intervals.push(interval);
                        if self.clock_intervals.len() > 24 {
                            self.clock_intervals.remove(0);
                        }

                        if self.clock_intervals.len() >= 12 {
                            let avg_micros: u64 = self.clock_intervals.iter().map(|d| d.as_micros() as u64).sum::<u64>() / self.clock_intervals.len() as u64;
                            // 24 clocks per quarter note
                            // BPM = 60,000,000 / (avg_micros * 24)
                            let new_bpm = 60_000_000.0 / (avg_micros as f32 * 24.0);
                            if (new_bpm - self.current_bpm).abs() > 0.1 {
                                self.current_bpm = new_bpm;
                                self.last_clock_time = Some(now);
                                return Some(new_bpm);
                            }
                        }
                    } else {
                        self.clock_intervals.clear();
                    }
                }
                self.last_clock_time = Some(now);
                self.clock_count = (self.clock_count + 1) % 24;
            }
            0xFA | 0xFB => { // Start or Continue
                self.is_running = true;
                self.clock_count = 0;
                self.clock_intervals.clear();
            }
            0xFC => { // Stop
                self.is_running = false;
            }
            _ => {}
        }
        None
    }
}
