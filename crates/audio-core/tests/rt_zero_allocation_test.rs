//! The ENGINE's block cycle must not allocate — not just the processors in it.
//!
//! The per-processor conformance gauntlet
//! (`nullherz-processors/tests/conformance_gauntlet.rs`) covers `process()`.
//! It could never have caught the violation that was actually in the tree,
//! because that one was in the engine's own plumbing:
//! `TopologyCoordinator::commit()` called `GraphCompiler::compile()` on the
//! audio thread, boxing a 128 KB adjacency matrix and a 245 KB producer map —
//! ~373 KB per commit, the larger box past glibc's mmap threshold, on a
//! SCHED_FIFO thread.
//!
//! So this measures the whole cycle: command drain, topology commit, graph
//! execution, telemetry finalisation. Anything on that path that reaches for
//! the heap shows up here.

use std::sync::Arc;

use audio_core::engine::builder::EngineBuilder;
use audio_core::processors::graph::ProcessorGraph;

use nullherz_traits::test_kit::rt_alloc;
use nullherz_traits::{
    AudioConfig, BufferId, Command, CompiledGraphPlan, CoreCommand, GraphTopology,
    NodeAssignment, NodeRouting, TimestampedCommand, TopologyMutation, MAX_BUFFERS, MAX_CHANNELS,
    MAX_NODES,
};

#[global_allocator]
static ALLOC: rt_alloc::CountingAllocator<std::alloc::System> =
    rt_alloc::CountingAllocator::new(std::alloc::System);

const BLOCK: usize = 256;
const SR: f32 = 44_100.0;

fn gain_topology(node_count: usize) -> GraphTopology {
    let mut v2p = [BufferId(0); MAX_BUFFERS];
    for (i, v) in v2p.iter_mut().enumerate() {
        *v = BufferId(i as u32);
    }
    let mut topo = GraphTopology {
        routing: [NodeRouting {
            input_indices: [BufferId(0); MAX_CHANNELS],
            output_indices: [BufferId(0); MAX_CHANNELS],
            sidechain_indices: [BufferId(0); MAX_CHANNELS],
            input_count: 0,
            output_count: 0,
            sidechain_count: 0,
            input_delays: [0.0; MAX_CHANNELS],
        }; MAX_NODES],
        virtual_to_physical: v2p,
        plan: CompiledGraphPlan::default(),
        crossfades: [None; 8],
        node_count,
        node_assignments: [NodeAssignment([0; 32]); MAX_NODES],
        node_positions: [None; MAX_NODES],
        bypass_states: [false; MAX_NODES],
    };
    // A chain: node i reads buffer i, writes buffer i+1.
    for i in 0..node_count {
        topo.routing[i].input_indices[0] = BufferId(i as u32);
        topo.routing[i].input_count = 1;
        topo.routing[i].output_indices[0] = BufferId((i + 1) as u32);
        topo.routing[i].output_count = 1;
    }
    topo.plan = nullherz_topology::GraphCompiler::compile(&topo).expect("compile off-thread");
    topo
}

#[test]
fn guard_is_installed() {
    assert!(rt_alloc::is_installed(), "allocation guard not in force in this binary");
}

/// Steady-state rendering: no commands, no topology churn, just audio.
#[test]
fn steady_state_block_does_not_allocate() {
    let registry = Arc::new(nullherz_dna::SampleRegistry::new());
    let (engine, _handle) = EngineBuilder::new()
        .with_sample_registry(registry)
        .build();
    // SAFETY: single-threaded test; nothing else holds the engine.
    let engine = unsafe { &mut *(Arc::as_ptr(&engine) as *mut audio_core::engine::AudioEngine) };
    engine.set_config(AudioConfig { sample_rate: SR, block_size: BLOCK });

    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];

    // Warm-up: the first blocks build lazily-initialised shared state (FFT
    // plans, the resampler's sinc table). That is a one-time cost, not a
    // per-block one.
    for _ in 0..8 {
        let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
        engine.process(&[], &mut outs);
    }

    let report = rt_alloc::measure(|| {
        for _ in 0..64 {
            let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
            engine.process(&[], &mut outs);
        }
    });

    assert!(
        report.is_clean(),
        "64 steady-state blocks allocated: {report}. The audio callback runs under \
         SCHED_FIFO; malloc there can take the mmap path and fault pages in."
    );
}

/// A topology commit is the path that was actually broken.
///
/// `SetTopology` carries a plan compiled off-thread, so installing it should
/// cost the audio thread nothing. Before the fix, `commit()` re-compiled
/// whenever the staged plan looked empty — ~373 KB of boxes, on this thread.
#[test]
fn topology_commit_does_not_allocate() {
    let registry = Arc::new(nullherz_dna::SampleRegistry::new());
    let (engine, handle) = EngineBuilder::new()
        .with_sample_registry(registry)
        .with_initial_graph(Box::new(ProcessorGraph::new()))
        .build();
    let engine = unsafe { &mut *(Arc::as_ptr(&engine) as *mut audio_core::engine::AudioEngine) };
    engine.set_config(AudioConfig { sample_rate: SR, block_size: BLOCK });

    let mut topo_producer = handle.topology_producer;
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];

    let pump = |engine: &mut audio_core::engine::AudioEngine, l: &mut Vec<f32>, r: &mut Vec<f32>| {
        let mut outs: [&mut [f32]; 2] = [l, r];
        engine.process(&[], &mut outs);
    };

    // Warm up, then pre-build the topologies OUTSIDE the measured region —
    // building an Arc<GraphTopology> is orchestration-plane work.
    for _ in 0..8 {
        pump(engine, &mut l, &mut r);
    }
    let topologies: Vec<Arc<GraphTopology>> =
        (1..=8).map(|n| Arc::new(gain_topology(n))).collect();

    let report = rt_alloc::measure(|| {
        for topo in &topologies {
            let _ = topo_producer.push(TopologyMutation::SetTopology(topo.clone()));
            // Two blocks: one drains the mutation and stages it, the next
            // performs the swap.
            pump(engine, &mut l, &mut r);
            pump(engine, &mut l, &mut r);
        }
    });

    assert!(
        report.is_clean(),
        "installing 8 pre-compiled topologies allocated on the audio thread: {report}. \
         A commit must be a pointer swap; compiling here boxes ~373 KB per call."
    );
}

/// INDIVIDUAL structural mutations — the path that actually allocated.
///
/// `TopologyManager` pushes `AddNode`/`UpdateEdge` one at a time and then a
/// `CommitTopology` that ships the compiled plan. Each individual mutation
/// goes through `inactive_topology_mut()`, which zeroes `plan.num_stages` to
/// invalidate the stale plan — and THAT is what used to send `commit()` into
/// `GraphCompiler::compile` on this thread. `SetTopology` never triggered it,
/// because the plan it carries is already compiled, which is why a test built
/// only on `SetTopology` would pass either way.
#[test]
fn individual_mutations_do_not_allocate() {
    let registry = Arc::new(nullherz_dna::SampleRegistry::new());
    let (engine, handle) = EngineBuilder::new()
        .with_sample_registry(registry)
        .with_initial_graph(Box::new(ProcessorGraph::new()))
        .build();
    let engine = unsafe { &mut *(Arc::as_ptr(&engine) as *mut audio_core::engine::AudioEngine) };
    engine.set_config(AudioConfig { sample_rate: SR, block_size: BLOCK });

    let mut topo_producer = handle.topology_producer;
    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..8 {
        let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
        engine.process(&[], &mut outs);
    }

    // Build the processors up front — constructing a Box<dyn AudioProcessor>
    // is orchestration-plane work and belongs outside the measured region,
    // exactly as the conductor does it.
    let mut pending: Vec<TopologyMutation> = Vec::new();
    for i in 0..8u32 {
        pending.push(TopologyMutation::AddNode {
            node_idx: i,
            processor: Box::new(nullherz_processors::gain::GainProcessor::new(i as u64, 1.0)),
        });
        pending.push(TopologyMutation::UpdateEdge {
            node_idx: i,
            input_idx: 0,
            new_buffer_idx: i,
        });
        pending.push(TopologyMutation::UpdateOutputEdge {
            node_idx: i,
            output_idx: 0,
            new_buffer_idx: i + 1,
        });
    }

    let report = rt_alloc::measure(|| {
        for m in pending.drain(..) {
            let _ = topo_producer.push(m);
        }
        // Enough blocks to drain the ring (16 mutations per block) and let the
        // coordinator attempt its commits.
        for _ in 0..8 {
            let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
            engine.process(&[], &mut outs);
        }
    });

    assert!(
        report.is_clean(),
        "draining 24 structural mutations allocated on the audio thread: {report}. \
         `commit()` must not compile here — that boxes a 128 KB adjacency matrix and \
         a 245 KB producer map, the latter past glibc's mmap threshold."
    );
}

/// Commands are the other thing that crosses into the block cycle.
///
/// Sample-accurate dispatch splits the block at each command timestamp, so a
/// block carrying commands walks a different path through the kernel than a
/// quiet one. Both have to be allocation-free.
#[test]
fn command_dispatch_does_not_allocate() {
    let registry = Arc::new(nullherz_dna::SampleRegistry::new());
    let (engine, handle) = EngineBuilder::new()
        .with_sample_registry(registry)
        .build();
    let engine = unsafe { &mut *(Arc::as_ptr(&engine) as *mut audio_core::engine::AudioEngine) };
    engine.set_config(AudioConfig { sample_rate: SR, block_size: BLOCK });

    let mut l = vec![0.0f32; BLOCK];
    let mut r = vec![0.0f32; BLOCK];
    for _ in 0..8 {
        let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
        engine.process(&[], &mut outs);
    }

    let report = rt_alloc::measure(|| {
        for block in 0..8u64 {
            // Several commands per block, at distinct sub-block offsets.
            for k in 0..4u64 {
                let _ = handle.command_producer.push_command(TimestampedCommand {
                    timestamp_samples: block * BLOCK as u64 + k * 37,
                    command: Command::Core(CoreCommand::SetBpm(120.0 + k as f32)),
                });
            }
            let mut outs: [&mut [f32]; 2] = [&mut l, &mut r];
            engine.process(&[], &mut outs);
        }
    });

    assert!(
        report.is_clean(),
        "command dispatch allocated on the audio thread: {report}"
    );
}
