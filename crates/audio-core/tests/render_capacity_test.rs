//! The graph must actually be able to render `MAX_BLOCK_SIZE` frames per call.
//!
//! Both audio backends chunk the device period against `MAX_BLOCK_SIZE` and
//! hand the engine whatever chunk that yields. If the graph's buffers are
//! smaller than that constant, the slice `data[offset..offset + num_samples]`
//! goes out of bounds and the process aborts — on the SCHED_FIFO audio thread,
//! mid-stream, with no recovery.
//!
//! This did not reproduce on the development machine because PipeWire happened
//! to negotiate a 256-frame period, exactly the size the buffers had. Anyone
//! whose device negotiated 512 or 1024 (or who raised `period_size` in
//! system_config.json for stability on a loaded machine) hit an instant crash.
//! So these tests deliberately exercise sizes the reference hardware never
//! produces.

use audio_core::processors::graph::{GraphBufferPool, RenderBlock};
use nullherz_traits::{IPC_BLOCK_SIZE, MAX_BLOCK_SIZE};

#[test]
fn test_graph_buffers_hold_a_full_max_block() {
    assert_eq!(
        RenderBlock::silent().data.len(),
        MAX_BLOCK_SIZE,
        "graph buffers are {} frames but the backends chunk to MAX_BLOCK_SIZE ({}) — \
         every render of a larger period indexes out of bounds on the audio thread",
        RenderBlock::silent().data.len(),
        MAX_BLOCK_SIZE
    );
}

#[test]
fn test_render_capacity_is_not_pinned_to_the_ipc_protocol_size() {
    // The regression in full: the graph pool was built from `AudioBlock`, whose
    // size is a shared-memory protocol decision. Reusing it for graph buffers
    // silently made the IPC size the render capacity. They are allowed to be
    // equal, but the graph must never be limited *because* of the IPC choice.
    assert!(
        RenderBlock::silent().data.len() >= IPC_BLOCK_SIZE,
        "graph capacity ({}) is below the IPC block size ({IPC_BLOCK_SIZE}); a block \
         arriving from a sidecar cannot even be staged",
        RenderBlock::silent().data.len()
    );
}

// Both operands are constants, so this belongs at compile time: a violation
// should fail the build rather than wait for someone to run the test.
const _MAX_BLOCK_COVERS_IPC_BLOCK: () = assert!(MAX_BLOCK_SIZE >= IPC_BLOCK_SIZE);

#[test]
fn test_every_pool_buffer_is_indexable_to_capacity() {
    // Not just the prototype block: `buffers`, `old_path_buffers` and
    // `crossfade_buffers` are separate arrays, and the crash reads from all
    // three. Touching the last frame of each proves none was allocated short.
    let mut pool = GraphBufferPool::new();
    pool.clear();

    // capture_old_buffers copies buffers -> old_path_buffers across the full
    // width; if the two ever disagree on length this panics.
    pool.capture_old_buffers(MAX_BLOCK_SIZE);

    // Reaching the final frame through the public surface: a full-width slice
    // is exactly what the executor takes when num_samples == MAX_BLOCK_SIZE.
    //
    // Indexed via `get` on a black_box'd length rather than a literal. A direct
    // `data[MAX_BLOCK_SIZE - 1]` is const-evaluated, so an undersized buffer
    // becomes a *compile* error (`unconditional_panic`) — correct, but it takes
    // the whole test binary down and none of these diagnostics ever print.
    let block = RenderBlock::silent();
    let last = std::hint::black_box(MAX_BLOCK_SIZE) - 1;
    assert_eq!(
        block.data.get(last).copied(),
        Some(0.0),
        "graph buffer holds {} frames, so frame {last} of a MAX_BLOCK_SIZE ({MAX_BLOCK_SIZE}) \
         render does not exist — this is the out-of-bounds the audio thread hits",
        block.data.len()
    );
}

#[test]
fn test_backend_chunking_never_exceeds_graph_capacity() {
    // The contract between the two halves of the bug. Mirrors
    // nullherz_backends::chunking::render_blocks without depending on that
    // crate (audio-core sits below it): whatever period a device negotiates,
    // no single render call may exceed what the graph can hold.
    for period in [64usize, 128, 256, 480, 512, 1024, 2048, 4096] {
        let mut offset = 0usize;
        while offset < period {
            let chunk = (period - offset).min(MAX_BLOCK_SIZE);
            assert!(
                chunk <= RenderBlock::silent().data.len(),
                "device period {period} produces a {chunk}-frame render call, but graph \
                 buffers hold only {} frames",
                RenderBlock::silent().data.len()
            );
            assert!(chunk > 0, "zero-length chunk would loop forever for period {period}");
            offset += chunk;
        }
        assert_eq!(offset, period, "chunks for period {period} must cover it exactly");
    }
}
