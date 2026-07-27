//! Capacity constants define what the engine CAN address; runtime config
//! defines what it actually uses. These tests pin the invariants that keep the
//! two from being conflated, and the one place where a capacity increase would
//! silently cost something.
//!
//! Every assertion here compares compile-time constants — that is the point, not
//! an oversight. They exist so that changing a constant produces a named, failing
//! test that explains the consequence, rather than a silently different binary.
//! Several of these invariants have no other guard: `MAX_MUTATIONS` overflow is a
//! silent drop, and `MAX_BUFFERS` overflow aliases crossfade slots onto pool ids.
#![allow(clippy::assertions_on_constants)]

use nullherz_traits::{
    AudioBlock, IPC_BLOCK_SIZE, MAX_BLOCK_SIZE, MAX_BUFFERS, MAX_CROSSFADE_BUFFERS, MAX_NODES,
};

/// `AudioBlock` is a shared-memory PROTOCOL type, allocated in fixed-depth rings
/// for every input, output and sidechain channel of every sidecar. Its size must
/// not track the engine's render capacity: doing so multiplies every ring by the
/// same factor while `len` still reports the (much smaller) realtime block, so
/// the bandwidth and residency are paid for frames that are never used.
#[test]
fn test_ipc_block_size_is_decoupled_from_render_capacity() {
    assert_eq!(
        std::mem::size_of::<AudioBlock>(),
        1088,
        "AudioBlock is a protocol ABI size; changing it changes every sidecar's \
         shared-memory layout"
    );
    assert_eq!(std::mem::align_of::<AudioBlock>(), 64);
    assert_eq!(
        IPC_BLOCK_SIZE, 256,
        "IPC_BLOCK_SIZE is the protocol frame count and should change only \
         deliberately, with the sidecar protocol version"
    );

    // The point of the split: raising render capacity must not move the ABI.
    assert!(
        MAX_BLOCK_SIZE >= IPC_BLOCK_SIZE,
        "render capacity ({MAX_BLOCK_SIZE}) below the IPC frame count \
         ({IPC_BLOCK_SIZE}) makes every sidecar push a partially-filled block"
    );
}

/// A render block larger than one `AudioBlock` has to be SPLIT before crossing
/// the IPC boundary. The sidecar bridge currently clamps, so this documents the
/// gap that its `debug_assert!` guards, and will start failing the day the
/// realtime block legitimately exceeds the protocol frame count.
#[test]
fn test_realtime_blocks_still_fit_one_ipc_block() {
    // The realtime block comes from system_config.json (128-256 today). The
    // capacity is larger for offline render and safe-mode, and those paths do
    // not cross the sidecar boundary.
    for realtime_block in [64usize, 128, 256] {
        assert!(
            realtime_block <= IPC_BLOCK_SIZE,
            "realtime block {realtime_block} exceeds IPC_BLOCK_SIZE; the sidecar \
             bridge must chunk before this is a supported configuration"
        );
    }
}

/// `block_x_map` packs crossfade overrides as `MAX_BUFFERS + k` into a `u8`,
/// with 0 reserved for "no override". Exceeding 255 aliases crossfade slots onto
/// pool ids — silent audio corruption, no error.
#[test]
fn test_buffer_space_fits_the_u8_sentinel_encoding() {
    assert!(
        MAX_BUFFERS + MAX_CROSSFADE_BUFFERS <= 255,
        "MAX_BUFFERS ({MAX_BUFFERS}) + MAX_CROSSFADE_BUFFERS \
         ({MAX_CROSSFADE_BUFFERS}) exceeds the u8 encoding space"
    );
    // Headroom, so adding a crossfade slot is not a cliff.
    assert!(
        MAX_BUFFERS + MAX_CROSSFADE_BUFFERS <= 248,
        "no headroom left in the u8 encoding space"
    );
}

/// Taps are subscriptions on existing buffer edges, so the buffer space is the
/// tap budget. The 4-deck console already reaches edge ~87; decomposing deck EQs
/// to expose per-band taps adds roughly eight edges per deck.
#[test]
fn test_buffer_space_has_room_for_tap_decomposition() {
    const CONSOLE_EDGES_TODAY: usize = 87;
    const ESTIMATED_TAP_DECOMPOSITION: usize = 4 * 8;
    assert!(
        MAX_BUFFERS >= CONSOLE_EDGES_TODAY + ESTIMATED_TAP_DECOMPOSITION,
        "MAX_BUFFERS ({MAX_BUFFERS}) leaves no room to decompose processors for \
         tap points"
    );
}

/// The 4-deck bootstrap already allocates ~46 nodes. Node space must leave room
/// for decomposition too, and `MAX_MUTATIONS` must cover a full graph rebuild —
/// its overflow is a silent drop, not an error.
#[test]
fn test_node_space_and_mutation_budget_cover_a_full_rebuild() {
    const CONSOLE_NODES_TODAY: usize = 46;
    assert!(
        MAX_NODES >= CONSOLE_NODES_TODAY * 2,
        "MAX_NODES ({MAX_NODES}) leaves less than 2x headroom over the current \
         console ({CONSOLE_NODES_TODAY} nodes)"
    );
    assert!(
        nullherz_traits::MAX_MUTATIONS >= MAX_NODES,
        "MAX_MUTATIONS ({}) is below MAX_NODES ({MAX_NODES}); building a full \
         graph would silently drop mutations",
        nullherz_traits::MAX_MUTATIONS
    );
}
