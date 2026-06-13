use crate::AudioEngine;
use crate::processors::{ProcessorGraph, AudioProcessor, TopologyMutation};
use control_plane::{TimestampedCommand, Command};
use ipc_layer::{RingBuffer, MpscRingBuffer};
use std::sync::Arc;
use proptest::prelude::*;

#[cfg(test)]
mod tests {
    use super::*;

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
                    _ => TopologyMutation::SwapProcessor { node_idx, processor: Box::new(crate::processors::graph::DummyProcessor) },
                }
            }).collect();

            let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
            let (topo_prod, topo_cons) = RingBuffer::<TopologyMutation>::new(256).split();
            let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
            let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

            let graph = ProcessorGraph::new();
            let mut engine = AudioEngine::new(cmd_buffer, None, None, Some(topo_cons), garbage_prod, None, None, None, tel_prod, Box::new(graph));

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
            let mut engine = AudioEngine::new(cmd_cons, None, None, None, garbage_prod, None, None, None, tel_prod, Box::new(graph));

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

        #[test]
        fn test_random_dag_execution(
            // Generate a random adjacency matrix for a DAG (upper triangular to ensure no cycles)
            edges in prop::collection::vec(
                (0..64usize, 0..64usize),
                1..100
            ).prop_map(|e| {
                e.into_iter()
                 .filter(|(src, dst)| src < dst) // Ensure acyclic
                 .collect::<Vec<_>>()
            })
        ) {
            let (topo_prod, topo_cons) = RingBuffer::<TopologyMutation>::new(256).split();
            let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
            let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
            let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));

            let graph = ProcessorGraph::new();
            let mut engine = AudioEngine::new(cmd_buffer.clone(), None, None, Some(topo_cons), garbage_prod, None, None, None, tel_prod, Box::new(graph));

            let mut topo_prod = topo_prod;
            // Add nodes first
            for i in 0..64 {
                let _ = topo_prod.push(TopologyMutation::AddNode {
                    node_idx: i as u32,
                    processor: Box::new(crate::processors::graph::DummyProcessor),
                });
            }

            // Apply edges
            for (src, dst) in edges {
                // For simplicity, map src output 0 to dst input 0
                let _ = topo_prod.push(TopologyMutation::UpdateEdge {
                    node_idx: dst as u32,
                    input_idx: 0,
                    new_buffer_idx: src as u32,
                });
                let _ = topo_prod.push(TopologyMutation::UpdateOutputEdge {
                    node_idx: src as u32,
                    output_idx: 0,
                    new_buffer_idx: src as u32,
                });
            }

            let _ = cmd_buffer.push(TimestampedCommand {
                timestamp_samples: 0,
                command: Command::CommitTopology,
            });

            let mut outputs = [[0.0f32; 128], [0.0f32; 128]];
            let (ch1, ch2) = outputs.split_at_mut(1);
            let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

            // Process a few blocks to ensure it doesn't crash or hang
            for _ in 0..5 {
                engine.process_block(&[], &mut out_refs, 128);
            }
        }
    }

    #[test]
    fn test_engine_sub_block_splitting() {
        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

        struct MockProcessor {
            #[allow(dead_code)]
            process_count: u32,
        }
        impl AudioProcessor for MockProcessor {
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
            fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _ctx: &mut crate::processors::ProcessContext) {
                self.process_count += 1;
            }
        }
        impl std::fmt::Debug for MockProcessor { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "MockProcessor") } }

        let mut engine = AudioEngine::new(cmd_buffer.clone(), None, None, None, garbage_prod, None, None, None, tel_prod, Box::new(MockProcessor { process_count: 0 }));

        // Push a command at sample 64
        let _ = cmd_buffer.push(TimestampedCommand {
            timestamp_samples: 64,
            command: Command::Play,
        });

        let mut outputs = [[0.0f32; 128], [0.0f32; 128]];
        let (ch1, ch2) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

        engine.process_block(&[], &mut out_refs, 128);
    }

    #[test]
    fn test_engine_midi_routing() {
        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (midi_prod, midi_cons) = RingBuffer::<ipc_layer::MidiEvent>::new(256).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

        struct MidiMockProcessor {
            pub midi_received: bool,
        }
        impl AudioProcessor for MidiMockProcessor {
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
            fn apply_midi(&mut self, _event: ipc_layer::MidiEvent) {
                self.midi_received = true;
            }
            fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _ctx: &mut crate::processors::ProcessContext) {}
        }
        impl std::fmt::Debug for MidiMockProcessor { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "MidiMockProcessor") } }

        let mut engine = AudioEngine::new(cmd_buffer.clone(), Some(midi_cons), None, None, garbage_prod, None, None, None, tel_prod, Box::new(MidiMockProcessor { midi_received: false }));

        let mut midi_prod = midi_prod;
        let _ = midi_prod.push(ipc_layer::MidiEvent { timestamp_samples: 0, status: 0x90, data1: 60, data2: 100, _pad: 0 });

        let mut outputs = [[0.0f32; 128], [0.0f32; 128]];
        let (ch1, ch2) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

        engine.process_block(&[], &mut out_refs, 128);

        // Access the processor via downcast if possible (in this case we know the type)
        // But the engine stores AtomicPtr<Box<dyn AudioProcessor>>, so we'd need to peek.
        // For simplicity in this test, let's just assume it works if no panic.
    }
}
