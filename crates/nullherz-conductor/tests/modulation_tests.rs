use nullherz_conductor::Conductor;
use nullherz_traits::Command;

#[test]
fn test_macro_modulation_expansion() {
    let mut conductor = Conductor::with_library_path("test_lib_1.redb");
    conductor.setup_engine();

    // 1. Setup a modulation mapping
    // Macro 0 -> Node 10, Param 1, Scaling 2.0
    // Macro 0 -> Node 20, Param 3, Scaling 0.5
    let commands = vec![
        Command::AddModMapping {
            macro_id: 0,
            target_id: 10,
            param_id: 1,
            scaling: 2.0,
        },
        Command::AddModMapping {
            macro_id: 0,
            target_id: 20,
            param_id: 3,
            scaling: 0.5,
        },
    ];
    conductor.apply_mixer_commands(commands);

    // 2. Trigger Macro
    let macro_cmd = vec![
        Command::SetMacro {
            macro_id: 0,
            value: 0.8,
        },
    ];

    // We need to verify that MixerBridge expanded this.
    // Conductor's apply_mixer_commands calls mixer_bridge.apply_mixer_commands
    // which pushes to bundle_producer.

    conductor.apply_mixer_commands(macro_cmd);

    // Since we are in a test, let's check the modulation_matrix state directly
    assert_eq!(conductor.modulation_matrix.mappings.get(&0).unwrap().len(), 2);

    let expanded = conductor.modulation_matrix.expand_macro(0, 0.8);
    assert_eq!(expanded.len(), 2);

    match &expanded[0] {
        Command::SetParam { target_id, param_id, value, .. } => {
            assert_eq!(*target_id, 10);
            assert_eq!(*param_id, 1);
            assert!((*value - 1.6).abs() < 1e-6);
        }
        _ => panic!("Expected SetParam"),
    }

    match &expanded[1] {
        Command::SetParam { target_id, param_id, value, .. } => {
            assert_eq!(*target_id, 20);
            assert_eq!(*param_id, 3);
            assert!((*value - 0.4).abs() < 1e-6);
        }
        _ => panic!("Expected SetParam"),
    }
}

#[test]
fn test_modulation_matrix_persistence() {
    let mut conductor = Conductor::with_library_path("test_lib_2.redb");
    conductor.modulation_matrix.add_mapping(1, 100, 5, 1.5);

    let test_path = "test_mod_project.json";
    conductor.save_project(test_path).unwrap();

    let mut conductor2 = Conductor::new();
    conductor2.load_project(test_path).unwrap();

    assert_eq!(conductor2.modulation_matrix.mappings.get(&1).unwrap().len(), 1);
    let mapping = &conductor2.modulation_matrix.mappings.get(&1).unwrap()[0];
    assert_eq!(mapping.target_id, 100);
    assert_eq!(mapping.param_id, 5);
    assert_eq!(mapping.scaling, 1.5);

    std::fs::remove_file(test_path).unwrap();
}
