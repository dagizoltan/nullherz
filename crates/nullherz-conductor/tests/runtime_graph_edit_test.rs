//! A session must be able to gain and lose a channel strip while it is running.
//!
//! This was the gap behind the "~10 channel strips" ceiling, and the ceiling was
//! the less interesting half: it was never reachable, because nothing could
//! create a strip after bootstrap. `GraphReconciler` over `DesiredGraphState`
//! and `MixerManager::create_studio_strip` were both written, complete-looking,
//! and had zero callers between them.
//!
//! Two properties matter and they are different:
//!
//!   * a strip added at runtime REACHES THE MASTER — an unconnected strip is
//!     silent, and silence is the failure mode that looks like success;
//!   * removing one gives its node slots and buffers BACK. `IdAllocator` is
//!     monotonic and says ids are "never reused for safety and simplicity",
//!     which is correct for a bootstrap that runs once and fatal for an editor:
//!     a strip costs 11 nodes and 19 buffers against ceilings of 128 and 240, so
//!     leaking them exhausts the graph in about a dozen edits with nothing live.

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
        // Single-threaded harness; same pattern as the other conductor tests.
        let p = Arc::as_ptr(e) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*p).process_block(&inputs, &mut outputs, BLOCK); }
    }
}

fn booted() -> (Conductor, Vec<f32>, Vec<f32>) {
    let mut c = Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();
    let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    // The mutation ring drains a bounded number per block; let bootstrap land.
    for _ in 0..512 { pump(&mut c, &mut l, &mut r); }
    (c, l, r)
}

/// The DJ A bus, which `create_4channel_mixer` pins at fixed ids.
const BUS_L: u32 = 2;
const BUS_R: u32 = 3;

#[test]
fn test_a_strip_can_be_added_to_a_running_session() {
    let (mut c, mut l, mut r) = booted();

    let before = c.desired_graph_state().occupied();
    let placed = c
        .add_channel_strip(BUS_L, BUS_R)
        .expect("a booted console has room for one more strip");
    for _ in 0..64 { pump(&mut c, &mut l, &mut r); }

    assert_eq!(placed.len(), 2, "expected sampler + gain, got {placed:?}");
    let after = c.desired_graph_state().occupied();
    assert_eq!(
        after,
        before + 2,
        "the strip did not land in the live graph: {before} -> {after} nodes"
    );

    // It must be CONNECTED, not merely present. An unconnected strip renders
    // silence, which is exactly what a passing-looking failure looks like here.
    let state = c.desired_graph_state();
    let gain = state.nodes[placed[1] as usize].expect("gain node is live");
    let sum_sees_it = state.nodes.iter().flatten().any(|n| {
        n.input_buffers
            .iter()
            .take(n.input_count as usize)
            .any(|b| *b == gain.output_buffers[0])
    });
    assert!(
        sum_sees_it,
        "nothing consumes the new strip's output buffer {} — it is in the graph \
         and inaudible",
        gain.output_buffers[0]
    );
}

#[test]
fn test_removing_a_strip_returns_its_slots_and_buffers() {
    let (mut c, mut l, mut r) = booted();
    let baseline = c.desired_graph_state();
    let nodes_before = baseline.occupied();
    let bufs_before = baseline.buffers_in_use().len();

    let placed = c.add_channel_strip(BUS_L, BUS_R).expect("room for a strip");
    for _ in 0..64 { pump(&mut c, &mut l, &mut r); }
    assert_eq!(c.desired_graph_state().occupied(), nodes_before + 2);

    c.remove_nodes(&placed).expect("remove");
    for _ in 0..64 { pump(&mut c, &mut l, &mut r); }

    let after = c.desired_graph_state();
    assert_eq!(
        after.occupied(),
        nodes_before,
        "node slots leaked: {} live after add+remove, started at {nodes_before}",
        after.occupied()
    );
    assert_eq!(
        after.buffers_in_use().len(),
        bufs_before,
        "buffers leaked: {} in use after add+remove, started at {bufs_before} — a \
         strip costs 19 of 240, so this exhausts the graph in a dozen edits",
        after.buffers_in_use().len()
    );
}

/// The property that makes an editor an editor: repeated edits must not consume
/// the graph. Monotonic ids would fail this on the third or fourth cycle.
#[test]
fn test_repeated_add_remove_does_not_consume_the_graph() {
    let (mut c, mut l, mut r) = booted();
    let start = c.desired_graph_state();
    let (n0, b0) = (start.occupied(), start.buffers_in_use().len());

    for cycle in 0..20 {
        let placed = c
            .add_channel_strip(BUS_L, BUS_R)
            .unwrap_or_else(|e| panic!("cycle {cycle}: {e}"));
        for _ in 0..16 { pump(&mut c, &mut l, &mut r); }
        c.remove_nodes(&placed).expect("remove");
        for _ in 0..16 { pump(&mut c, &mut l, &mut r); }
    }

    let end = c.desired_graph_state();
    assert_eq!(
        (end.occupied(), end.buffers_in_use().len()),
        (n0, b0),
        "20 add/remove cycles left the graph at {} nodes / {} buffers, started at \
         {n0} / {b0} — ids are leaking and the session dies by attrition",
        end.occupied(),
        end.buffers_in_use().len()
    );
}
