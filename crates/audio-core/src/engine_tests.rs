use crate::AudioEngine;
use crate::processors::{ProcessorGraph, AudioProcessor, TopologyMutation};
use nullherz_traits::{TimestampedCommand, Command};
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
            let resources = crate::engine::EngineResources {
                command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
                command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
                midi_consumer: None,
                bundle_consumer: None,
                topology_consumer: Some(Box::new(topo_cons)),
                garbage_producer: garbage_prod,
                overflow_garbage_producer: None,
                bundle_garbage_producer: None,
                bundle_overflow_producer: None,
                telemetry_producer: Box::new(tel_prod),
            };
            let mut engine = AudioEngine::new(
                resources,
                Box::new(graph),
                Arc::new(crate::rt_logging::RtLogger::new(256))
            );

            let mut topo_prod_cloned = topo_prod;
            for mutation in mutations {
                let _ = topo_prod_cloned.push(mutation);
            }

            let mut out_l = [0.0f32; 128];
            let mut out_r = [0.0f32; 128];
            let mut outputs = [&mut out_l[..], &mut out_r[..]];
            engine.process_block(&[], &mut outputs, 128);
        }
    }

    #[test]
    fn test_random_dag_execution() {
        let mut graph = ProcessorGraph::new();
        // Setup a non-trivial DAG
        graph.add_node(Box::new(crate::processors::graph::DummyProcessor), vec![], vec![2, 3]);
        graph.add_node(Box::new(crate::processors::graph::DummyProcessor), vec![2], vec![4]);
        graph.add_node(Box::new(crate::processors::graph::DummyProcessor), vec![3, 4], vec![0, 1]);

        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();

        let resources = crate::engine::EngineResources {
            command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
            command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
            midi_consumer: None,
            bundle_consumer: None,
            topology_consumer: None,
            garbage_producer: garbage_prod,
            overflow_garbage_producer: None,
            bundle_garbage_producer: None,
            bundle_overflow_producer: None,
            telemetry_producer: Box::new(tel_prod),
        };
        let mut engine = AudioEngine::new(
            resources,
            Box::new(graph),
            Arc::new(crate::rt_logging::RtLogger::new(256))
        );

        let mut out_l = [0.0f32; 128];
        let mut out_r = [0.0f32; 128];
        let mut outputs = [&mut out_l[..], &mut out_r[..]];
        engine.process_block(&[], &mut outputs, 128);
    }

    proptest! {
        #[test]
        fn test_engine_robustness_with_random_commands(
            commands in prop::collection::vec(
                prop_oneof![
                    (0..100u64, 0..10u32, 0.0f32..1.0f32).prop_map(|(target, param, val)|
                        Command::SetParam { target_id: target, param_id: param, value: val, ramp_duration_samples: 0 }
                    ),
                    prop_oneof![Just(Command::Play), Just(Command::Stop)],
                ],
                1..20
            )
        ) {
            let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
            let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
            let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();

            for (i, cmd) in commands.into_iter().enumerate() {
                let _ = cmd_buffer.push(TimestampedCommand {
                    timestamp_samples: i as u64 * 10,
                    command: cmd,
                });
            }

            let graph = ProcessorGraph::new();
            let resources = crate::engine::EngineResources {
                command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
                command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
                midi_consumer: None,
                bundle_consumer: None,
                topology_consumer: None,
                garbage_producer: garbage_prod,
                overflow_garbage_producer: None,
                bundle_garbage_producer: None,
                bundle_overflow_producer: None,
                telemetry_producer: Box::new(tel_prod),
            };
            let mut engine = AudioEngine::new(
                resources,
                Box::new(graph),
                Arc::new(crate::rt_logging::RtLogger::new(256))
            );

            let mut out_l = [0.0f32; 128];
            let mut out_r = [0.0f32; 128];
            let mut outputs = [&mut out_l[..], &mut out_r[..]];
            engine.process_block(&[], &mut outputs, 128);
        }
    }

    #[test]
    fn test_engine_sub_block_splitting() {
        struct MockProcessor {
            process_count: usize,
        }
        impl AudioProcessor for MockProcessor {
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
            fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {
                self.process_count += 1;
            }
        }

        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();

        // Push a command at sample 50
        let _ = cmd_buffer.push(TimestampedCommand {
            timestamp_samples: 50,
            command: Command::Play,
        });

        let resources = crate::engine::EngineResources {
            command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
            command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
            midi_consumer: None,
            bundle_consumer: None,
            topology_consumer: None,
            garbage_producer: garbage_prod,
            overflow_garbage_producer: None,
            bundle_garbage_producer: None,
            bundle_overflow_producer: None,
            telemetry_producer: Box::new(tel_prod),
        };
        let mut engine = AudioEngine::new(
            resources,
            Box::new(MockProcessor { process_count: 0 }),
            Arc::new(crate::rt_logging::RtLogger::new(256))
        );

        let mut out = [0.0f32; 128];
        let mut outputs = [&mut out[..]];
        engine.process_block(&[], &mut outputs, 128);

        // Should have processed 0-50 and 50-128
        let graph = engine.graph_manager.get_active_graph().as_any().downcast_ref::<MockProcessor>().unwrap();
        assert_eq!(graph.process_count, 2);
    }

    #[test]
    fn test_engine_midi_routing() {
        struct MidiMockProcessor {
            midi_received: bool,
        }
        impl AudioProcessor for MidiMockProcessor {
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
            fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {}
            fn apply_midi(&mut self, _ev: ipc_layer::MidiEvent) {
                self.midi_received = true;
            }
        }

        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (midi_prod, midi_cons) = RingBuffer::new(256).split();
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();

        let mut midi_prod = midi_prod;
        let _ = midi_prod.push(ipc_layer::MidiEvent { timestamp_samples: 0, status: 0x90, data1: 60, data2: 127, _pad: 0 });

        let resources = crate::engine::EngineResources {
            command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
            command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
            midi_consumer: Some(Box::new(midi_cons)),
            bundle_consumer: None,
            topology_consumer: None,
            garbage_producer: garbage_prod,
            overflow_garbage_producer: None,
            bundle_garbage_producer: None,
            bundle_overflow_producer: None,
            telemetry_producer: Box::new(tel_prod),
        };
        let mut engine = AudioEngine::new(
            resources,
            Box::new(MidiMockProcessor { midi_received: false }),
            Arc::new(crate::rt_logging::RtLogger::new(256))
        );

        let mut out = [0.0f32; 128];
        let mut outputs = [&mut out[..]];
        engine.process_block(&[], &mut outputs, 128);

        let graph = engine.graph_manager.get_active_graph().as_any().downcast_ref::<MidiMockProcessor>().unwrap();
        assert!(graph.midi_received);
    }
}
