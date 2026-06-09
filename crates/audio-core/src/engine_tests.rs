use crate::AudioEngine;
use crate::processors::{ProcessorGraph, AudioProcessor};
use control_plane::{TimestampedCommand, Command};
use ipc_layer::{RingBuffer, MpscRingBuffer};
use std::sync::Arc;
use proptest::prelude::*;

#[cfg(test)]
proptest! {
    #[test]
    fn test_engine_robustness_with_random_commands(
        commands in prop::collection::vec(
            (0..1000u64, prop_oneof![
                Just(Command::Play),
                Just(Command::Stop),
                (0..64u64, 0..10u32, -1.0f32..1.0f32).prop_map(|(target_id, param_id, value)|
                    Command::SetParam { target_id, param_id, value, ramp_duration_samples: 0 }
                ),
            ]),
            1..100
        )
    ) {
        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let cmd_cons = cmd_buffer.clone();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

        let graph = ProcessorGraph::new();
        let mut engine = AudioEngine::new(cmd_cons, None, None, garbage_prod, None, tel_prod, Box::new(graph));

        for (ts, cmd) in commands {
            let _ = cmd_buffer.push(TimestampedCommand { timestamp_samples: ts, command: cmd });
        }

        let mut outputs = [[0.0f32; 128], [0.0f32; 128]];
        let (ch1, ch2) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

        // Run several blocks to process commands
        for _ in 0..10 {
            engine.process_block(&[], &mut out_refs, 128);
        }
    }
}
