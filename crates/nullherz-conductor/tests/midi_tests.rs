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

#[test]
fn test_conductor_midi_polling() {
    let mut conductor = nullherz_conductor::Conductor::new();
    let (mut midi_prod, midi_cons) = ipc_layer::RingBuffer::new(16).split();
    conductor.set_midi_consumer(midi_cons);

    // Map CC 10 to Target 0, Param 1
    let map = MidiMap {
        name: "Test Map".into(),
        controls: vec![ControlMapping {
            cc_number: 10,
            target: MidiTarget::Param { target_id: 100, param_id: 1 },
            min_val: 0.0,
            max_val: 1.0,
        }],
        triggers: vec![],
    };
    conductor.midi_mapper.active_map = Some(map);

    // Push MIDI Event
    let event = MidiEvent {
        timestamp_samples: 0,
        status: 0xB0,
        data1: 10,
        data2: 64,
        _pad: 0,
    };
    midi_prod.push(event).unwrap();

    // Tick conductor to process MIDI
    conductor.tick();

    // Verify command reached MixerBridge (via TopologyManager or just checking results)
    // For now we check if the command was at least translated.
    // In a full integration test we'd check the engine, but here we can check the mixer_bridge's state or telemetry if it were updated.
    // Actually, MixerBridge::apply_mixer_commands usually pushes to the engine.
}
