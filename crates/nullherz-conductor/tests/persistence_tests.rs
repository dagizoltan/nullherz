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
fn test_conductor_undo_redo() {
    use nullherz_traits::Command;
    let mut conductor = Conductor::new();

    // Initialize the topology mutation producer so handle_topology_command works in unit/integration test mode
    let mpsc_buf = std::sync::Arc::new(ipc_layer::MpscRingBuffer::new(128));
    conductor.topology_manager.topo_producer = Some(ipc_layer::NonRtProducer::from_mpsc(mpsc_buf));

    // 1. Initial State: Empty Graph. Save/checkpoint this initial state.
    // Ensure active_node_types is empty
    assert!(conductor.topology_manager.active_node_types.is_empty());

    // 2. Perform a structural topology edit: AddNode GAIN (node 0)
    // Note: apply_mixer_commands automatically calls checkpoint() for Topology commands!
    let cmd_add = Command::Topology(nullherz_traits::TopologyCommand::AddNode {
        processor_type_id: ProcessorTypeId::GAIN,
        node_idx: 0,
    });
    conductor.apply_mixer_commands(vec![cmd_add]);

    // Verify Node 0 is now active
    assert!(conductor.topology_manager.active_node_types.contains_key(&0));

    // Ensure undo stack has exactly 1 checkpoint (the empty state before addition)
    assert_eq!(conductor.undo_stack.len(), 1);

    // 3. Call undo()!
    let undone = conductor.undo();
    assert!(undone);

    // Verify Node 0 is now GONE (restored back to empty state, exercise node-removal path)
    assert!(!conductor.topology_manager.active_node_types.contains_key(&0));
    assert!(conductor.topology_manager.active_node_types.is_empty());

    // Ensure redo stack has exactly 1 checkpoint (the state with Node 0)
    assert_eq!(conductor.redo_stack.len(), 1);

    // 4. Call redo()!
    let redone = conductor.redo();
    assert!(redone);

    // Verify Node 0 is BACK!
    assert!(conductor.topology_manager.active_node_types.contains_key(&0));
}

#[test]
fn test_project_state_restore_node_removal() {
    use nullherz_traits::Command;
    let mut conductor = Conductor::new();

    // Initialize the topology mutation producer so handle_topology_command works in unit/integration test mode
    let mpsc_buf = std::sync::Arc::new(ipc_layer::MpscRingBuffer::new(128));
    conductor.topology_manager.topo_producer = Some(ipc_layer::NonRtProducer::from_mpsc(mpsc_buf));

    // 1. Build a small graph with 1 node (e.g., node 0, GAIN)
    let cmd_add = Command::Topology(nullherz_traits::TopologyCommand::AddNode {
        processor_type_id: ProcessorTypeId::GAIN,
        node_idx: 0,
    });
    conductor.topology_manager.handle_topology_command(&cmd_add);
    conductor.topology_manager.handle_topology_command(&Command::Core(nullherz_traits::CoreCommand::CommitTopology));

    // Ensure the node is there
    assert!(conductor.topology_manager.active_node_types.contains_key(&0));

    // 2. Capture the current state (manually built state_before representing the captured state with only Node 0)
    let mut state_before = ProjectState::empty();
    state_before.nodes.push(NodeState {
        id: 0,
        type_id: ProcessorTypeId::GAIN.0,
        params: vec![],
        position: None,
    });

    // 3. Add another node (e.g., node 1, SAMPLER)
    let cmd_add_2 = Command::Topology(nullherz_traits::TopologyCommand::AddNode {
        processor_type_id: ProcessorTypeId::SAMPLER,
        node_idx: 1,
    });
    conductor.topology_manager.handle_topology_command(&cmd_add_2);
    conductor.topology_manager.handle_topology_command(&Command::Core(nullherz_traits::CoreCommand::CommitTopology));

    // Ensure BOTH nodes are there
    assert!(conductor.topology_manager.active_node_types.contains_key(&0));
    assert!(conductor.topology_manager.active_node_types.contains_key(&1));

    // 4. Restore/apply the captured state_before
    state_before.apply(&mut conductor).expect("Failed to apply state");

    // 5. Verify the added node (node 1) is gone, and node 0 remains
    assert!(conductor.topology_manager.active_node_types.contains_key(&0));
    assert!(!conductor.topology_manager.active_node_types.contains_key(&1));
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
