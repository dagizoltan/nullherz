/// Represents an action to be performed by the audio engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetParam {
        target: String,
        param: String,
        value: f32,
    },
    Play,
    Stop,
    LoadGraph(String),
}

/// A command with an associated timestamp for deterministic execution.
#[derive(Debug, Clone, PartialEq)]
pub struct TimestampedCommand {
    pub timestamp_samples: u64,
    pub command: Command,
}
