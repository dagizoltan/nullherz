//! `set_config` must not require `&mut` to the engine, and the transport must
//! still follow the configured rate.
//!
//! The engine lives in an `Arc<dyn RenderingEngine>`, so a `&mut self` on
//! `set_config` forced every caller to obtain one by casting a shared `Arc` —
//! `Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine`. That is UB however
//! many threads are involved, and it was not avoidable at the call site:
//! `EngineBuilder::build` keeps a second clone of the engine as
//! `EngineHandle::controller` for the life of the session, so the
//! `Arc::get_mut` those sites tried first could never succeed. See
//! `TECHNICAL_DEBT_AND_STUBS.md` §1.1.
//!
//! `set_config` now takes `&self`. The compiler enforces the no-`&mut` half of
//! that (a `&self` call through `&Arc<dyn _>` simply compiles), so what needs a
//! test is the BEHAVIOUR that moved to make it possible: `transport.sample_rate`
//! used to be written inside `set_config` and is now picked up by `process` from
//! an atomic, one block later.

use audio_core::engine::AudioEngine;
use audio_core::processors::graph::ProcessorGraph;
use audio_core::engine::processing_kernel::StandardKernel;
use ipc_layer::{MpscRingBuffer, RingBuffer};
use nullherz_traits::{AudioConfig, RenderingEngine, TimestampedCommand, TopologyMutation};
use std::sync::Arc;

fn engine() -> AudioEngine<StandardKernel> {
    let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(256));
    let (_topo_prod, topo_cons) = RingBuffer::<TopologyMutation>::new(256).split();
    let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn nullherz_traits::AudioProcessor>>::new(64).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(64).split();

    let resources = audio_core::engine::EngineResources {
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
        worker_count: None,
    };
    AudioEngine::new(
        resources,
        Box::new(ProcessorGraph::new()),
        Arc::new(nullherz_dna::SampleRegistry::new()),
        Arc::new(audio_core::rt_logging::RtLogger::new(64)),
        StandardKernel,
    )
}

fn render_one_block(engine: &mut AudioEngine<StandardKernel>) {
    let mut l = [0.0f32; 128];
    let mut r = [0.0f32; 128];
    let (a, b) = (&mut l[..], &mut r[..]);
    let mut outs: [&mut [f32]; 2] = [a, b];
    engine.process(&[], &mut outs);
}

/// The whole point: reconfiguring needs only a shared reference, so no caller
/// has to cast an `Arc` to do it. If `set_config` ever regains `&mut self` this
/// stops compiling, which is the assertion.
#[test]
fn test_set_config_needs_only_a_shared_reference() {
    let shared: Arc<dyn RenderingEngine> = Arc::new(engine());
    // A second holder, exactly as `EngineHandle::controller` is in production —
    // so `Arc::get_mut` would fail here, as it does there.
    let _second = shared.clone();
    assert_eq!(Arc::strong_count(&shared), 2);

    shared.set_config(AudioConfig { sample_rate: 96_000.0, block_size: 128 });

    assert_eq!(
        shared.target_sample_rate(), 96_000.0,
        "set_config through a shared reference did not take effect"
    );
}

/// `transport.sample_rate` moved out of `set_config` (which can no longer write
/// RT-owned state) into `process`. It must still end up correct.
#[test]
fn test_the_transport_adopts_the_configured_rate_on_the_next_block() {
    let mut e = engine();
    let before = e.transport.sample_rate;
    assert_ne!(before, 96_000.0, "precondition: pick a rate the engine is not already at");

    e.set_config(AudioConfig { sample_rate: 96_000.0, block_size: 128 });
    assert_eq!(e.target_sample_rate(), 96_000.0, "the target rate is immediate");

    render_one_block(&mut e);
    assert_eq!(
        e.transport.sample_rate, 96_000.0,
        "the transport did not adopt the configured rate; every beat-derived \
         position in the graph would be computed against the old rate"
    );
}

/// And it must not drift back, or cost anything per block once settled.
#[test]
fn test_the_rate_is_stable_across_further_blocks() {
    let mut e = engine();
    e.set_config(AudioConfig { sample_rate: 88_200.0, block_size: 128 });
    for _ in 0..8 {
        render_one_block(&mut e);
    }
    assert_eq!(e.transport.sample_rate, 88_200.0);
    assert_eq!(e.target_sample_rate(), 88_200.0);
}
