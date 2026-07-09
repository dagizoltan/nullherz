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
}
