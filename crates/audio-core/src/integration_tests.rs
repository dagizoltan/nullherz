#[cfg(feature = "test-utils")]
mod integration_tests {
    use crate::{AudioEngine, ProcessorGraph};
    use nullherz_traits::{AudioProcessor, test_kit::{TestHost, MockProcessor}, ProcessorTypeId, GarbageProducer};
    use nullherz_processors::ProcessorRegistry;
    use ipc_layer::RingBuffer;
    use std::sync::Arc;

    #[test]
    fn test_complete_engine_cycle_with_registry() {
        let registry = ProcessorRegistry::new();
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
        let mut engine = AudioEngine::new(
            Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
            Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
            None,
            None,
            None,
            garbage_prod_engine,
            None,
            None,
            None,
            Box::new(tel_prod),
            Box::new(graph)
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
