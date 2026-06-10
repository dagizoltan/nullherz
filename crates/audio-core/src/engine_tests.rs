use crate::AudioEngine;
use crate::processors::{ProcessorGraph, AudioProcessor, TopologyMutation};
use control_plane::{TimestampedCommand, Command};
use ipc_layer::{RingBuffer, MpscRingBuffer};
use std::sync::Arc;
use proptest::prelude::*;

#[cfg(test)]
proptest! {
    #[test]
    fn test_engine_robustness_with_topology_mutations(
        mutations in prop::collection::vec(
            prop_oneof![
                (0..64u32, 0..16u32, 0..64u32).prop_map(|(node_idx, input_idx, new_buffer_idx)|
                    (node_idx, input_idx, new_buffer_idx, 0)
                ),
                (0..64u32, 0..16u32, 0..64u32).prop_map(|(node_idx, output_idx, new_buffer_idx)|
                    (node_idx, output_idx, new_buffer_idx, 1)
                ),
                (0..64u32).prop_map(|node_idx|
                    (node_idx, 0, 0, 2)
                ),
            ],
            1..20
        )
    ) {
        let mutations: Vec<TopologyMutation> = mutations.into_iter().map(|(node_idx, idx, buf, kind)| {
            match kind {
                0 => TopologyMutation::UpdateEdge { node_idx, input_idx: idx, new_buffer_idx: buf },
                1 => TopologyMutation::UpdateOutputEdge { node_idx, output_idx: idx, new_buffer_idx: buf },
                _ => TopologyMutation::SwapProcessor { node_idx, processor: Box::new(crate::processors::standard::GainProcessor::new(1, 1.0)) },
            }
        }).collect();

        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (topo_prod, topo_cons) = RingBuffer::<TopologyMutation>::new(256).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

        let graph = ProcessorGraph::new();
        let mut engine = AudioEngine::new(cmd_buffer, None, None, Some(topo_cons), garbage_prod, None, tel_prod, Box::new(graph));

        let mut topo_prod_cloned = topo_prod;
        for mutation in mutations {
            let _ = topo_prod_cloned.push(mutation);
        }

        let mut outputs = [[0.0f32; 128], [0.0f32; 128]];
        let (ch1, ch2) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

        for _ in 0..5 {
            engine.process_block(&[], &mut out_refs, 128);
        }
    }

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
        let mut engine = AudioEngine::new(cmd_cons, None, None, None, garbage_prod, None, tel_prod, Box::new(graph));

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
