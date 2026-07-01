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
    state.processor_states.push(ProcessorState {
        node_idx: 1,
        state_data: vec![0, 1, 2, 3],
    });

    state.save_to_file(temp_path).unwrap();

    conductor.load_project(temp_path).expect("Failed to load project");

    fs::remove_file(temp_path).ok();
}
