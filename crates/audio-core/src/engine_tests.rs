use crate::AudioEngine;
use crate::engine::processing_kernel::StandardKernel;
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
                Arc::new(crate::rt_logging::RtLogger::new(256)),
                StandardKernel::default()
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
            Arc::new(crate::rt_logging::RtLogger::new(256)),
            StandardKernel::default()
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
                        Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id: target, param_id: param, value: val, ramp_duration_samples: 0 })),
                    Just(Command::Core(nullherz_traits::CoreCommand::Play)),
                    Just(Command::Core(nullherz_traits::CoreCommand::Stop)),
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
                Arc::new(crate::rt_logging::RtLogger::new(256)),
                StandardKernel::default()
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
        impl nullherz_traits::SignalProcessor for MockProcessor {
fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {
                self.process_count += 1;
            }
}

impl nullherz_traits::MidiResponder for MockProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for MockProcessor { }

impl AudioProcessor for MockProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();

        // Push a command at sample 50
        let _ = cmd_buffer.push(TimestampedCommand {
            timestamp_samples: 50,
            command: Command::Core(nullherz_traits::CoreCommand::Play),
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
            Arc::new(crate::rt_logging::RtLogger::new(256)),
            StandardKernel::default()
        );

        let mut out = [0.0f32; 128];
        let mut outputs = [&mut out[..]];
        engine.process_block(&[], &mut outputs, 128);

        // Should have processed 0-50 and 50-128
        let graph = unsafe { engine.graph_manager.get_active_graph_mut().as_any().downcast_ref::<MockProcessor>().unwrap() };
        assert_eq!(graph.process_count, 2);
    }

    #[test]
    fn test_engine_midi_routing() {
        struct MidiMockProcessor {
            midi_received: bool,
        }
        impl nullherz_traits::SignalProcessor for MidiMockProcessor {
fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {}
}

impl nullherz_traits::MidiResponder for MidiMockProcessor { fn apply_midi(&mut self, _event: ipc_layer::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { self.midi_received = true; } }

impl nullherz_traits::SnapshotProvider for MidiMockProcessor { }

impl AudioProcessor for MidiMockProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
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
            Arc::new(crate::rt_logging::RtLogger::new(256)),
            StandardKernel::default()
        );

        let mut out = [0.0f32; 128];
        let mut outputs = [&mut out[..]];
        engine.process_block(&[], &mut outputs, 128);

        let graph = unsafe { engine.graph_manager.get_active_graph_mut().as_any().downcast_ref::<MidiMockProcessor>().unwrap() };
        assert!(graph.midi_received);
    }

    #[test]
    fn test_engine_bundle_command_application() {
        struct ParamMockProcessor {
            param_value: f32,
            apply_count: usize,
            id: u64,
        }
        impl nullherz_traits::SignalProcessor for ParamMockProcessor {
fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {}
}

impl nullherz_traits::MidiResponder for ParamMockProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for ParamMockProcessor { }

impl AudioProcessor for ParamMockProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, cmd: &Command) {
                if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, value, .. }) = cmd {
                    if *target_id == self.id {
                        self.param_value = *value;
                        self.apply_count += 1;
                    }
                }
            }
}

        let proc_id = 12345u64;
        let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();

        // Create a bundle with one command
        let mut bundle_data = [0u8; 128];
        bundle_data[0..8].copy_from_slice(&proc_id.to_le_bytes());
        bundle_data[8..12].copy_from_slice(&0u32.to_le_bytes()); // param_id
        bundle_data[12..16].copy_from_slice(&0.5f32.to_le_bytes());

        let _ = cmd_buffer.push(TimestampedCommand {
            timestamp_samples: 0,
            command: Command::Mixer(nullherz_traits::MixerCommand::Bundle { count: 1, data: bundle_data }),
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
            Box::new(ParamMockProcessor { param_value: 0.0, apply_count: 0, id: proc_id }),
            Arc::new(crate::rt_logging::RtLogger::new(256)),
            StandardKernel::default()
        );

        let mut out = [0.0f32; 128];
        let mut outputs = [&mut out[..]];
        engine.process_block(&[], &mut outputs, 128);

        let graph = unsafe { engine.graph_manager.get_active_graph_mut().as_any().downcast_ref::<ParamMockProcessor>().unwrap() };
        assert_eq!(graph.param_value, 0.5);
        // BUG-07 check: should only be applied once
        assert_eq!(graph.apply_count, 1);
    }
}
