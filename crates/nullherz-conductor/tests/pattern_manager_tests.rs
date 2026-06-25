use nullherz_conductor::orchestrator::Conductor;
use nullherz_conductor::pattern_manager::{SongArrangement, ArrangementEvent};
use nullherz_traits::{Command, CommandProducer, TimestampedCommand};
use std::sync::{Arc, Mutex};

struct MockProducer {
    commands: Arc<Mutex<Vec<Command>>>,
}

impl CommandProducer for MockProducer {
    fn push_command(&self, cmd: TimestampedCommand) -> Result<(), Command> {
        self.commands.lock().unwrap().push(cmd.command);
        Ok(())
    }
}

impl Clone for MockProducer {
    fn clone(&self) -> Self {
        Self { commands: self.commands.clone() }
    }
}

#[test]
fn test_pattern_manager_triggering() {
    let mut conductor = Conductor::with_library_path("test_pat_1.redb");
    let commands = Arc::new(Mutex::new(Vec::new()));
    let producer = MockProducer { commands: commands.clone() };

    conductor.engine_coordinator.command_producer = Some(Box::new(producer));

    let arrangement = SongArrangement {
        events: vec![
            ArrangementEvent {
                beat: 1.0,
                command: Command::SetParam { target_id: 10, param_id: 0, value: 1.0, ramp_duration_samples: 0 }
            },
            ArrangementEvent {
                beat: 4.0,
                command: Command::SetParam { target_id: 10, param_id: 0, value: 2.0, ramp_duration_samples: 0 }
            },
        ],
    };

    conductor.pattern_manager.set_arrangement(arrangement);

    // Tick at beat 0.5 - no triggers
    let cmds = conductor.pattern_manager.tick(0.5);
    assert_eq!(cmds.len(), 0);

    // Tick at beat 1.5 - first trigger should fire
    let cmds = conductor.pattern_manager.tick(1.5);
    assert_eq!(cmds.len(), 1);
    if let Command::SetParam { target_id, param_id, value, .. } = cmds[0] {
        assert_eq!(target_id, 10);
        assert_eq!(param_id, 0);
        assert_eq!(value, 1.0);
    } else {
        panic!("Wrong command type");
    }

    // Tick at beat 4.5 - second trigger should fire
    let cmds = conductor.pattern_manager.tick(4.5);
    assert_eq!(cmds.len(), 1);
    if let Command::SetParam { target_id, param_id, value, .. } = cmds[0] {
        assert_eq!(target_id, 10);
        assert_eq!(param_id, 0);
        assert_eq!(value, 2.0);
    } else {
        panic!("Wrong command type");
    }
}

#[test]
fn test_generic_arrangement_commands() {
    let mut conductor = Conductor::with_library_path("test_pat_2.redb");
    let commands = Arc::new(Mutex::new(Vec::new()));
    let producer = MockProducer { commands: commands.clone() };
    conductor.engine_coordinator.command_producer = Some(Box::new(producer));

    let arrangement = SongArrangement {
        events: vec![
            ArrangementEvent {
                beat: 2.0,
                command: Command::SetMacro { macro_id: 5, value: 0.75 }
            },
            ArrangementEvent {
                beat: 8.0,
                command: Command::Play
            },
        ],
    };

    conductor.pattern_manager.set_arrangement(arrangement);

    let cmds = conductor.pattern_manager.tick(3.0);
    assert_eq!(cmds.len(), 1);
    if let Command::SetMacro { macro_id, value } = cmds[0] {
        assert_eq!(macro_id, 5);
        assert!((value - 0.75).abs() < 1e-6);
    } else {
        panic!("Expected SetMacro");
    }

    let cmds = conductor.pattern_manager.tick(9.0);
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0], Command::Play);
}
