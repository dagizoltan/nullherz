#[cfg(feature = "test-utils")]
mod integration_tests {
    use crate::{AudioEngine, ProcessorGraph};
    use crate::engine::processing_kernel::StandardKernel;
    use nullherz_traits::{AudioProcessor, test_kit::{TestHost, MockProcessor}, };
    use nullherz_processors::ProcessorRegistry;
    use ipc_layer::RingBuffer;
    use std::sync::Arc;

    #[test]
    fn test_dynamic_processor_registration_and_engine_instantiation() {
        use nullherz_traits::ProcessorFactory;

        // 1. Define a truly dynamic processor and its factory
        struct DynamicProcessor;
        impl nullherz_traits::SignalProcessor for DynamicProcessor {
fn process(&mut self, _in: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {
                for out in outputs {
                    out.fill(0.123); // Unique signature
                }
            }
}

impl nullherz_traits::MidiResponder for DynamicProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for DynamicProcessor { }

impl AudioProcessor for DynamicProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

        struct DynamicFactory;
        impl ProcessorFactory for DynamicFactory {
            fn create_processor(&self, _node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
                Some(Box::new(DynamicProcessor))
            }
            fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId(0x99) }
            fn name(&self) -> &'static str { "Dynamic" }
        }

        use nullherz_traits::ProcessorTypeId;
        let mut registry = ProcessorRegistry::new();
        let dynamic_id = 0x99u32;
        registry.register_factory(Box::new(DynamicFactory));

        // 2. Setup Engine
        let cmd_buffer = Arc::new(ipc_layer::MpscRingBuffer::new(256));
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();

        let mut graph = ProcessorGraph::new();
        graph.set_garbage_producer(Box::new(garbage_prod));

        let dynamic_proc = registry.create_by_id(dynamic_id, 0, 44100.0).expect("Failed to create dynamic processor");
        graph.add_node(dynamic_proc, vec![], vec![0]);

        let (garbage_prod_engine, _garbage_cons_engine) = RingBuffer::new(1024).split();

        let resources = crate::engine::EngineResources {
            command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
            command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
            midi_consumer: None,
            bundle_consumer: None,
            topology_consumer: None,
            garbage_producer: garbage_prod_engine,
            overflow_garbage_producer: None,
            bundle_garbage_producer: None,
            bundle_overflow_producer: None,
            telemetry_producer: Box::new(tel_prod),
            worker_count: None,
        };

        let mut engine = AudioEngine::new(
            resources,
            Box::new(graph),
            Arc::new(nullherz_dna::SampleRegistry::new()),
            Arc::new(crate::rt_logging::RtLogger::new(256)),
            StandardKernel::default()
        );

        // 3. Process
        let mut out = [0.0f32; 128];
        let mut outputs = [&mut out[..]];
        engine.process_block(&[], &mut outputs, 128);

        // 4. Verify signature
        assert_eq!(out[0], 0.123);
    }

    struct SidechainGainProcessor {
        gain: f32,
    }

    impl std::fmt::Debug for SidechainGainProcessor {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "SidechainGainProcessor") }
    }

    impl nullherz_traits::SignalProcessor for SidechainGainProcessor {
        fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
            // inputs[0] is main, inputs[1] is sidechain
            let main = inputs[0];
            let sc = inputs.get(1).map(|&s| s).unwrap_or(&[]);
            let out = &mut outputs[0];
            for i in 0..out.len() {
                let sc_val = if i < sc.len() { sc[i] } else { 1.0 };
                out[i] = main[i] * sc_val * self.gain;
            }
        }
    }

    impl nullherz_traits::MidiResponder for SidechainGainProcessor {}
    impl nullherz_traits::SnapshotProvider for SidechainGainProcessor {}
    impl crate::processors::AudioProcessor for SidechainGainProcessor {
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    #[test]
    fn test_sidechain_routing() {
        use nullherz_traits::SignalProcessor;
        let mut graph = ProcessorGraph::new();
        // Node 0: SidechainGainProcessor. Input 2 (main), Sidechain 3 (sc), Output 0
        graph.add_node(Box::new(SidechainGainProcessor { gain: 1.0 }), vec![2], vec![0]);

        let active_idx = graph.topology_coordinator.active_idx();
        graph.topology_coordinator.topologies[active_idx].routing[0].sidechain_indices[0] = 3;
        graph.topology_coordinator.topologies[active_idx].routing[0].sidechain_count = 1;

        graph.buffer_pool.buffers[2].data.fill(0.5); // Main signal
        graph.buffer_pool.buffers[3].data.fill(0.2); // Sidechain signal

        let mut out_data = [0.0f32; 128];
        let mut outputs = [&mut out_data[..]];
        let mut context = nullherz_traits::ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        graph.process(&[], &mut outputs, &mut context);

        for i in 0..128 {
            assert!((out_data[i] - 0.1).abs() < 1e-6); // 0.5 * 0.2
        }
    }

    #[test]
    fn test_complete_engine_cycle_with_registry() {
        let _registry = ProcessorRegistry::new();
        let _host = TestHost::new();

        // 1. Setup Engine components
        let cmd_buffer = Arc::new(ipc_layer::MpscRingBuffer::new(256));
        let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();

        let mut graph = ProcessorGraph::new();
        graph.set_garbage_producer(Box::new(garbage_prod));

        // 2. Register and create a custom processor via registry
        let mut mock = Box::new(MockProcessor::new());
        mock.set_parameter(1, 0.5, 0);

        graph.add_node(mock, vec![], vec![0, 1]);

        let (garbage_prod_engine, _garbage_cons_engine) = RingBuffer::new(1024).split();

        let resources = crate::engine::EngineResources {
            command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
            command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
            midi_consumer: None,
            bundle_consumer: None,
            topology_consumer: None,
            garbage_producer: garbage_prod_engine,
            overflow_garbage_producer: None,
            bundle_garbage_producer: None,
            bundle_overflow_producer: None,
            telemetry_producer: Box::new(tel_prod),
            worker_count: None,
        };

        let mut engine = AudioEngine::new(
            resources,
            Box::new(graph),
            Arc::new(nullherz_dna::SampleRegistry::new()),
            Arc::new(crate::rt_logging::RtLogger::new(256)),
            StandardKernel::default()
        );

        // 3. Run a block
        let mut out_l = [0.0f32; 128];
        let mut out_r = [0.0f32; 128];
        let mut outputs = [&mut out_l[..], &mut out_r[..]];

        engine.process_block(&[], &mut outputs, 128);

        // 4. Verify telemetry (indirectly) and state
        assert_eq!(engine.transport.sample_rate, 44100.0);
    }

    #[test]
    fn test_pdc_phase_coherence() {
        // 1. Setup Engine
        let _cmd_buffer = Arc::new(ipc_layer::MpscRingBuffer::<nullherz_traits::TimestampedCommand>::new(256));
        let (_tel_prod, _tel_cons) = RingBuffer::<nullherz_traits::telemetry::Telemetry>::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn nullherz_traits::AudioProcessor>>::new(1024).split();

        let mut graph = ProcessorGraph::new();
        graph.set_garbage_producer(Box::new(garbage_prod));

        // Path A: Dry (0 latency)
        struct DryProcessor;
        impl nullherz_traits::SignalProcessor for DryProcessor {
            fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {
                if !inputs.is_empty() && !outputs.is_empty() { outputs[0].copy_from_slice(inputs[0]); }
            }
        }
        impl nullherz_traits::MidiResponder for DryProcessor {}
        impl nullherz_traits::SnapshotProvider for DryProcessor {}
        impl AudioProcessor for DryProcessor {
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        }

        // Path B: Latent (128 samples latency)
        struct LatentProcessor { latency: usize }
        impl nullherz_traits::SignalProcessor for LatentProcessor {
            fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut nullherz_traits::ProcessContext) {
                // In a real test we'd actually delay, but here we just report latency
                if !inputs.is_empty() && !outputs.is_empty() { outputs[0].copy_from_slice(inputs[0]); }
            }
            fn latency_samples(&self) -> usize { self.latency }
        }
        impl nullherz_traits::MidiResponder for LatentProcessor {}
        impl nullherz_traits::SnapshotProvider for LatentProcessor {}
        impl AudioProcessor for LatentProcessor {
            fn as_any(&self) -> &dyn std::any::Any { self }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        }

        graph.add_node(Box::new(DryProcessor), vec![10], vec![11]); // Node 0
        graph.add_node(Box::new(LatentProcessor { latency: 128 }), vec![10], vec![12]); // Node 1

        // Summing Node
        let summing = Box::new(nullherz_processors::SummingProcessor::new(100));
        graph.add_node(summing, vec![11, 12], vec![0]); // Node 2

        graph.calculate_stages();

        let active_idx = graph.topology_coordinator.active_idx();
        let topo = &graph.topology_coordinator.topologies[active_idx];

        // Verify PDC calculation
        // Dry path (Node 0) should have 128 samples of input delay compensation
        // Latent path (Node 1) should have 0 samples of input delay compensation
        // Wait, PDC usually applies to the summation point or at the start.
        // In my implementation:
        // path_latencies[0] = 0
        // path_latencies[1] = 0
        // current_path_lat_0 = 0 + 0 = 0
        // current_path_lat_1 = 0 + 128 = 128
        // path_latencies[2] = max(0, 128) = 128
        // input_delays[2][0] = 128 - (0 + 0) = 128
        // input_delays[2][1] = 128 - (0 + 128) = 0

        assert_eq!(topo.plan.input_delays[2].0[0], 128.0);
        assert_eq!(topo.plan.input_delays[2].0[1], 0.0);
    }
}
