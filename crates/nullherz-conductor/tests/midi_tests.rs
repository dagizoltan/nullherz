use nullherz_conductor::midi_mapper::MidiMapper;
use nullherz_traits::{MidiMap, ControlMapping, MidiTarget, MidiEvent, Command, MixerCommand};

#[test]
fn test_midi_mapping_translation() {
    let mut mapper = MidiMapper::new();
    let map = MidiMap {
        name: "Test Map".into(),
        controls: vec![
            ControlMapping {
                cc_number: 10,
                target: MidiTarget::Param { target_id: 50, param_id: 1 },
                min_val: 0.0,
                max_val: 100.0,
            }
        ],
        triggers: vec![],
    };
    mapper.active_map = Some(map);

    let event = MidiEvent {
        timestamp_samples: 0,
        status: 0xB0,
        data1: 10,
        data2: 64, // ~50%
        _pad: 0,
    };

    let node_names = std::collections::HashMap::new();
    let cmds = mapper.translate(&event, &node_names, None);
    assert_eq!(cmds.len(), 1);

    if let Command::Mixer(MixerCommand::SetParam { target_id, param_id, value, .. }) = cmds[0] {
        assert_eq!(target_id, 50);
        assert_eq!(param_id, 1);
        assert!(value > 49.0 && value < 51.0);
    } else {
        panic!("Expected Mixer SetParam command");
    }
}

#[test]
fn test_json_midi_profile_loading() {
    let profiles = ["default", "keyboard", "pioneer_ddj400", "akai_mpk_mini"];
    for profile in profiles {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = format!("mappings/{}.json", profile);
        let manifest_path = format!("{}/../../mappings/{}.json", manifest_dir, profile);

        let content = std::fs::read_to_string(&path)
            .or_else(|_| std::fs::read_to_string(&manifest_path))
            .unwrap_or_else(|_| panic!("Failed to read mapping file: {} or {}", path, manifest_path));
        let mut mapper = MidiMapper::new();
        assert!(
            mapper.load_from_json(&content).is_ok(),
            "Failed to parse MIDI JSON map for profile: {}",
            profile
        );
        assert!(mapper.active_map.is_some());
    }
}
