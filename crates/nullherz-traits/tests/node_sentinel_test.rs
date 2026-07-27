//! Logical node sentinels must never be addressable as graph indices.
//!
//! `NodeConventions` ids (PREVIEW, the per-deck sequencers) are LOGICAL: the UI
//! targets them without knowing the graph, and the conductor resolves them to
//! allocated node indices. The engine's guards are plain bounds checks
//! (`idx < MAX_NODES` in `topology_coordinator::apply_mutation`), so a sentinel
//! that happens to fall inside the graph index space stops being caught and
//! starts addressing a real node.
//!
//! They used to sit at 70–73 and 111, which was out of range only because
//! MAX_NODES was 64. Raising MAX_NODES to 128 would have made all five legal —
//! silently aiming every sentinel at a live deck. `_SENTINELS_ABOVE_MAX_NODES`
//! in `execution.rs` makes that a compile error; these tests pin the runtime
//! contract that goes with it.

use nullherz_traits::{NodeConventions as NC, MAX_NODES};

#[test]
fn test_every_sentinel_is_outside_the_graph_index_space() {
    for (name, id) in [
        ("PREVIEW", NC::PREVIEW),
        ("DECK_A_SEQUENCER", NC::DECK_A_SEQUENCER),
        ("DECK_B_SEQUENCER", NC::DECK_B_SEQUENCER),
        ("DECK_C_SEQUENCER", NC::DECK_C_SEQUENCER),
        ("DECK_D_SEQUENCER", NC::DECK_D_SEQUENCER),
    ] {
        assert!(
            id as usize >= MAX_NODES,
            "{name} = {id} is a legal graph index (MAX_NODES = {MAX_NODES}); \
             the engine's bounds guards will no longer drop it"
        );
        assert!(
            NC::is_logical(id),
            "{name} = {id} is not recognised by is_logical()"
        );
    }
}

#[test]
fn test_is_logical_rejects_real_graph_indices() {
    // Every addressable node index, plus the first value past the end, must read
    // as a graph index and not as a sentinel.
    for idx in 0..=MAX_NODES {
        assert!(
            !NC::is_logical(idx as u32),
            "graph index {idx} was misclassified as a logical sentinel"
        );
    }
}

#[test]
fn test_sentinel_range_has_headroom_over_max_nodes() {
    // Not merely disjoint — far enough apart that a plausible MAX_NODES increase
    // cannot close the gap without tripping the compile-time assertion.
    assert!(
        NC::LOGICAL_BASE as u64 > MAX_NODES as u64 * 1000,
        "sentinel base {:#x} is uncomfortably close to MAX_NODES {MAX_NODES}",
        NC::LOGICAL_BASE
    );
}

#[test]
fn test_sequencer_for_deck_maps_a_through_d() {
    assert_eq!(NC::sequencer_for_deck('A'), NC::DECK_A_SEQUENCER);
    assert_eq!(NC::sequencer_for_deck('B'), NC::DECK_B_SEQUENCER);
    assert_eq!(NC::sequencer_for_deck('C'), NC::DECK_C_SEQUENCER);
    assert_eq!(NC::sequencer_for_deck('D'), NC::DECK_D_SEQUENCER);
    // Lower case must resolve identically — the helper upper-cases its input.
    assert_eq!(NC::sequencer_for_deck('c'), NC::DECK_C_SEQUENCER);
}
