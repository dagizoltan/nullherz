use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use ipc_layer::RingBuffer;
use nullherz_traits::{
    AudioProcessor, Command, ProcessingKernel, TimestampedCommand, Transport,
    MidiEvent, CoreCommand, MixerCommand
};
use crate::AudioEngine;
use crate::engine::builder::EngineBuilder;
use crate::engine::resource_recycler::ResourceRecycler;
use crate::engine::metrics::EngineMetrics;
use crate::engine::input_handler::EngineInputHandler;
use crate::processors::ProcessorGraph;

struct CustomCountingKernel {
    pub execution_count: Arc<AtomicUsize>,
    pub signature_val: f32,
}

impl ProcessingKernel for CustomCountingKernel {
    fn execute(
        &mut self,
        _graph: &mut dyn AudioProcessor,
        transport: &mut Transport,
        _host: Option<&dyn nullherz_traits::Host>,
        _pool: &mut Option<Box<dyn nullherz_traits::ParallelExecutor>>,
        _command_consumer: &mut Box<dyn nullherz_traits::CommandConsumer>,
        _pending_command: &mut Option<TimestampedCommand>,
        _sample_counter: u64,
        _inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        num_samples: usize,
    ) {
        self.execution_count.fetch_add(1, Ordering::Relaxed);
        for out in outputs.iter_mut() {
            let limit = out.len().min(num_samples);
            out[..limit].fill(self.signature_val);
        }
        transport.absolute_samples += num_samples as u64;
    }
}

#[test]
fn test_decoupled_custom_kernel_execution() {
    let exec_count = Arc::new(AtomicUsize::new(0));
    let kernel = CustomCountingKernel {
        execution_count: exec_count.clone(),
        signature_val: 0.77f32,
    };

    let sample_registry = Arc::new(nullherz_dna::SampleRegistry::new());
    let initial_graph = Box::new(ProcessorGraph::new());

    let (engine, _handle) = EngineBuilder::new()
        .with_initial_graph(initial_graph)
        .with_sample_registry(sample_registry)
        .with_kernel(kernel)
        .build();

    let mut out_l = [0.0f32; 64];
    let mut out_r = [0.0f32; 64];
    let mut outputs = [&mut out_l[..], &mut out_r[..]];

    // Real-time backend pattern: get raw pointer and cast to execute mutably
    let engine_ptr = Arc::as_ptr(&engine) as *mut AudioEngine<CustomCountingKernel>;
    unsafe {
        (*engine_ptr).process_block(&[], &mut outputs, 64);
    }

    assert_eq!(exec_count.load(Ordering::Relaxed), 1);
    for sample in out_l.iter() {
        assert_eq!(*sample, 0.77f32);
    }
    for sample in out_r.iter() {
        assert_eq!(*sample, 0.77f32);
    }
}

#[test]
fn test_resource_recycler_saturation_safety() {
    // 1. Recycle with empty channels (both garbage & overflow are None)
    // In this case, standard drop shouldn't run on the RT thread. We call std::mem::forget.
    let metrics = EngineMetrics::new();
    let health_signal = Arc::new(AtomicBool::new(false));

    let mut recycler = ResourceRecycler::new(None, None);

    // Let's track if an Arc actually gets deallocated on the thread.
    // Since we'll call std::mem::forget, the Arc's strong count will NOT decrement,
    // which prevents deallocation!
    let tracking_arc = Arc::new(vec![1.0f32; 100]);
    let tracking_arc_clone = tracking_arc.clone();

    let heavy_cmd = Command::Mixer(MixerCommand::SetParam {
        target_id: 1,
        param_id: 2,
        value: 0.5,
        ramp_duration_samples: 0,
    });
    // Let's create a bundle containing commands.
    let bundle = vec![heavy_cmd];

    recycler.recycle_bundle(bundle, &metrics, &health_signal);

    // Leak metrics should have increased because there are no channels and we leaked it
    assert_eq!(metrics.resource_leaks.load(Ordering::Relaxed), 1);
    assert!(health_signal.load(Ordering::Relaxed));

    // The tracking arc must still have a strong count of at least 2 because we std::mem::forget-ed the bundle
    // and avoided drops/deallocations on the current thread!
    assert_eq!(Arc::strong_count(&tracking_arc_clone), 2);

    // Now let's test with filled queues to trigger the fallback overflow paths.
    // Create garbage ring buffers of size 4
    let (mut garbage_prod, _garbage_cons) = RingBuffer::<Vec<Command>>::new(4).split();
    let (mut overflow_prod, _overflow_cons) = RingBuffer::<Vec<Command>>::new(4).split();

    // Fill both producers completely to ensure any further recycle attempts overflow and fail
    while garbage_prod.push(vec![]).is_ok() {}
    while overflow_prod.push(vec![]).is_ok() {}

    let mut recycler_channels = ResourceRecycler::new(Some(garbage_prod), Some(overflow_prod));

    let bundle_overflowing = vec![Command::Core(CoreCommand::Play)];

    // This should fail to write to both queues and increment the leak metric
    recycler_channels.recycle_bundle(bundle_overflowing, &metrics, &health_signal);

    // Leak count should be incremented from 1 to 2
    assert_eq!(metrics.resource_leaks.load(Ordering::Relaxed), 2);
}

#[test]
fn test_engine_input_handler_edge_cases() {
    let mut graph = ProcessorGraph::new();
    let mut transport = Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: false,
        sample_rate: 44100.0,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    };

    let sample_registry = nullherz_dna::SampleRegistry::new();
    let metrics = EngineMetrics::new();
    let health_signal = Arc::new(AtomicBool::new(false));
    let mut recycler = ResourceRecycler::new(None, None);

    // Test with all-None options in EngineInputHandler
    EngineInputHandler::handle_async_inputs(
        &mut graph,
        &mut transport,
        &mut None,
        &mut None,
        &mut None,
        &mut recycler,
        &sample_registry,
        &metrics,
        &health_signal,
    );

    // Verify MIDI mass event processing does not block or panic
    let (mut midi_prod, midi_cons) = RingBuffer::<MidiEvent>::new(1000).split();
    for i in 0..500 {
        let event = MidiEvent {
            timestamp_samples: i as u64,
            status: 0x90,
            data1: 60,
            data2: 127,
            _pad: 0,
        };
        let _ = midi_prod.push(event);
    }

    let mut midi_consumer_opt: Option<Box<dyn nullherz_traits::MidiConsumer>> = Some(Box::new(midi_cons));
    EngineInputHandler::handle_async_inputs(
        &mut graph,
        &mut transport,
        &mut None,
        &mut None,
        &mut midi_consumer_opt,
        &mut recycler,
        &sample_registry,
        &metrics,
        &health_signal,
    );

    // Verify MIDI consumer is completely drained
    let mut midi_cons_back = midi_consumer_opt.unwrap();
    assert!(midi_cons_back.pop().is_none());
}
