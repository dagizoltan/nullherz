use nullherz_traits::{MidiEvent, Command, MidiMap, ControlMapping, MidiTarget};
use nullherz_conductor::midi_mapper::MidiMapper;

#[test]
fn test_midi_cc_translation() {
    let mut mapper = MidiMapper::new();

    // Map CC 10 to Target 0, Param 1
    let map = MidiMap {
        name: "Test Map".into(),
        controls: vec![ControlMapping {
            cc_number: 10,
            target: MidiTarget::Param { target_id: 0, param_id: 1 },
            min_val: 0.0,
            max_val: 1.0,
        }],
        triggers: vec![],
    };

    mapper.active_map = Some(map);

    // Simulate CC 10 with value 64 (~0.5)
    let event = MidiEvent {
        timestamp_samples: 0,
        status: 0xB0, // CC
        data1: 10,
        data2: 64,
        _pad: 0,
    };

    let cmds = mapper.translate(&event);
    assert_eq!(cmds.len(), 1);

    if let Command::SetParam { target_id, param_id, value, .. } = cmds[0] {
        assert_eq!(target_id, 0);
        assert_eq!(param_id, 1);
        assert!((value - 0.5039).abs() < 0.01);
    } else {
        panic!("Incorrect command type translated from MIDI CC");
    }
}
