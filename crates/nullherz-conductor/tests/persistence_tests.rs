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
        position: Some((100.0, 200.0)),
    });
    state.processor_states.push(ProcessorState {
        node_idx: 1,
        state_data: vec![0, 1, 2, 3],
    });

    state.save_to_file(temp_path).unwrap();

    conductor.load_project(temp_path).expect("Failed to load project");

    fs::remove_file(temp_path).ok();
}

#[test]
fn test_project_persistence_newer_version_error() {
    let temp_path = "test_project_new_version.json";
    fs::remove_file(temp_path).ok();

    // 1. Create a state and save it
    let mut state = ProjectState::empty();
    state.bpm = 140.0;
    state.save_to_file(temp_path).unwrap();

    // 2. Load and verify version is CURRENT_PROJECT_VERSION
    let loaded = ProjectState::load_from_file(temp_path).unwrap();
    assert_eq!(loaded.version, CURRENT_PROJECT_VERSION);

    // 3. Manually parse JSON from disk, change version to a higher number (e.g. 999), and save it back
    let json_content = fs::read_to_string(temp_path).unwrap();
    let mut json_val: serde_json::Value = serde_json::from_str(&json_content).unwrap();

    // Set version to 999
    json_val["version"] = serde_json::Value::from(999u32);

    let modified_json = serde_json::to_string_pretty(&json_val).unwrap();
    fs::write(temp_path, modified_json).unwrap();

    // 4. Try loading the modified JSON, expecting an InvalidData error (due to newer version)
    let load_res = ProjectState::load_from_file(temp_path);
    assert!(load_res.is_err(), "Loading a project with a newer version must fail");
    let err = load_res.err().unwrap();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("saved with a newer version"));

    fs::remove_file(temp_path).ok();
}
