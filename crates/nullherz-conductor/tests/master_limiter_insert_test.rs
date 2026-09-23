//! The master limiter is 2.0 ms of output latency and should be a CHOICE.
//!
//! It is 96 samples of look-ahead, correctly declared. Look-ahead is measured in
//! samples, so its cost is flat at every block size — noise against the shipped
//! 256-frame period's 23 ms, 27% of the total at 64 frames, 43% at 32.
//!
//! Whether it belongs in the path is a question about the session rather than
//! about latency. Measured: `bench_console_block` puts the master at 1.0000 with
//! four decks summing at full scale, so on a live console it is actively
//! clamping and there are no retakes. `probe_chain_taps` puts it at 0.3509 with
//! one deck at -6 dBFS — a delay line doing nothing, on a monitoring path where
//! limiting you did not ask for flatters the mix.
//!
//! Removing beats bypassing, and the second test says why.

use nullherz_conductor::orchestrator::Conductor;
use std::sync::Arc;

const BLOCK: usize = 256;

fn pump(c: &mut Conductor, l: &mut [f32], r: &mut [f32]) {
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![l, r];
    let mut lock = c.engine_coordinator.backend_manager.engine_handle.lock();
    let e = lock.as_mut().expect("engine");
    if let Some(engine) = Arc::get_mut(e) {
        engine.process_block(&inputs, &mut outputs, BLOCK);
    } else {
        // Single-threaded harness, as in the other conductor tests.
        let p = Arc::as_ptr(e) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*p).process_block(&inputs, &mut outputs, BLOCK); }
    }
}

fn booted() -> (Conductor, Vec<f32>, Vec<f32>) {
    let mut c = Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();
    let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    for _ in 0..512 { pump(&mut c, &mut l, &mut r); }
    (c, l, r)
}

/// Which node writes the master buffers.
fn master_producer(c: &Conductor) -> Option<usize> {
    let master_l = c.mixer_manager.config.master_l as u32;
    let state = c.desired_graph_state();
    state.nodes.iter().position(|slot| {
        slot.as_ref().is_some_and(|n| {
            n.output_buffers.iter().take(n.output_count as usize).any(|b| *b == master_l)
        })
    })
}

#[test]
fn test_master_limiter_can_be_taken_out_and_put_back() {
    let (mut c, mut l, mut r) = booted();

    let lim = c.mixer_manager.node_names["master_limiter"] as usize;
    let eq = c.mixer_manager.node_names["master_eq"] as usize;
    assert_eq!(
        master_producer(&c),
        Some(lim),
        "precondition: the limiter feeds the master buffers"
    );
    let nodes_with = c.desired_graph_state().occupied();

    // OUT — the EQ must take over the master buffers, or the console goes silent.
    c.set_master_limiter(false).expect("remove");
    for _ in 0..64 { pump(&mut c, &mut l, &mut r); }
    assert_eq!(
        master_producer(&c),
        Some(eq),
        "with the limiter gone the mastering EQ must write the master buffers \
         directly — otherwise nothing does and the output is silence"
    );
    assert_eq!(
        c.desired_graph_state().occupied(),
        nodes_with - 1,
        "the limiter's node slot was not released"
    );

    // Idempotent.
    c.set_master_limiter(false).expect("remove again");
    assert_eq!(c.desired_graph_state().occupied(), nodes_with - 1);

    // BACK IN — between the EQ and the master again.
    c.set_master_limiter(true).expect("reinsert");
    for _ in 0..64 { pump(&mut c, &mut l, &mut r); }
    let back = master_producer(&c).expect("something feeds the master");
    assert_ne!(back, eq, "the EQ should no longer write the master directly");
    let state = c.desired_graph_state();
    let lim_node = state.nodes[back].expect("live");
    assert!(
        lim_node.input_buffers[..2]
            .iter()
            .all(|b| state.nodes[eq].unwrap().output_buffers[..2].contains(b)),
        "the reinserted limiter does not read what the EQ writes — the chain is \
         broken even though both nodes exist"
    );
    assert_eq!(c.desired_graph_state().occupied(), nodes_with);
}

/// A BYPASSED node must declare no latency, because it adds none.
///
/// Bypass is a passthrough — `run_job` copies `inputs[0]` to the outputs without
/// calling `process()` — so a bypassed processor's delay line never runs. PDC
/// read `active_node_types` alone and kept compensating anyway, delaying every
/// PARALLEL path by a latency the bypassed path no longer had: a misalignment in
/// the opposite direction to the one PDC exists to fix.
///
/// It is why `set_master_limiter` removes rather than bypasses.
#[test]
fn test_a_bypassed_node_declares_no_latency() {
    let (mut c, mut l, mut r) = booted();
    let lim = c.mixer_manager.node_names["master_limiter"];

    let declared = |c: &Conductor| {
        c.topology_manager.current_topology.plan.node_latencies[lim as usize]
    };
    assert!(
        declared(&c) > 0,
        "precondition: an active limiter declares its look-ahead (got {})",
        declared(&c)
    );

    c.apply_mixer_commands(vec![
        nullherz_traits::Command::Topology(nullherz_traits::TopologyCommand::SetBypass {
            node_idx: lim,
            enabled: true,
        }),
        nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology),
    ]);
    for _ in 0..64 { pump(&mut c, &mut l, &mut r); }

    assert_eq!(
        declared(&c),
        0,
        "a bypassed node still declares {} samples of latency — PDC is delaying \
         parallel paths to match a delay line that is not running",
        declared(&c)
    );
}
