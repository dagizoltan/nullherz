use nullherz_conductor::orchestrator::Conductor;
use nullherz_conductor::pattern_manager::{SongArrangement, PatternTrigger};
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
    let mut conductor = Conductor::new();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let producer = MockProducer { commands: commands.clone() };

    conductor.engine_coordinator.command_producer = Some(Box::new(producer));

    let arrangement = SongArrangement {
        triggers: vec![
            PatternTrigger { beat: 1.0, node_idx: 10, pattern_idx: 1 },
            PatternTrigger { beat: 4.0, node_idx: 10, pattern_idx: 2 },
        ],
    };

    conductor.pattern_manager.set_arrangement(arrangement);

    // Tick at beat 0.5 - no triggers
    conductor.pattern_manager.tick(0.5, &conductor.engine_coordinator.command_producer);
    assert_eq!(commands.lock().unwrap().len(), 0);

    // Tick at beat 1.5 - first trigger should fire
    conductor.pattern_manager.tick(1.5, &conductor.engine_coordinator.command_producer);
    assert_eq!(commands.lock().unwrap().len(), 1);
    if let Command::SetParam { target_id, param_id, value, .. } = commands.lock().unwrap()[0] {
        assert_eq!(target_id, 10);
        assert_eq!(param_id, 0);
        assert_eq!(value, 1.0);
    } else {
        panic!("Wrong command type");
    }

    // Tick at beat 4.5 - second trigger should fire
    conductor.pattern_manager.tick(4.5, &conductor.engine_coordinator.command_producer);
    assert_eq!(commands.lock().unwrap().len(), 2);
    if let Command::SetParam { target_id, param_id, value, .. } = commands.lock().unwrap()[1] {
        assert_eq!(target_id, 10);
        assert_eq!(param_id, 0);
        assert_eq!(value, 2.0);
    } else {
        panic!("Wrong command type");
    }
}
