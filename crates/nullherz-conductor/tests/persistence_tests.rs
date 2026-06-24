use nullherz_conductor::orchestrator::Conductor;
use nullherz_conductor::persistence::*;
use nullherz_traits::ProcessorTypeId;
use std::fs;

#[test]
fn test_project_persistence_roundtrip() {
    let mut conductor = Conductor::new();
    let temp_path = "test_project.json";

    let mut state = ProjectState::empty();
    state.bpm = 140.0;
    state.nodes.push(NodeState {
        id: 0,
        type_id: ProcessorTypeId::SAMPLER.into(),
        params: vec![(1, 1.0)],
    });
    state.sequencers.push(SequencerNodeState {
        node_idx: 1,
        patterns: vec![SequencerPatternState { grid: [[true; 16]; 16], len: 16 }; 8],
        active_pattern: 0,
    });

    state.save_to_file(temp_path).unwrap();

    conductor.load_project(temp_path).expect("Failed to load project");

    fs::remove_file(temp_path).ok();
}
