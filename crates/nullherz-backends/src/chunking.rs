//! Splitting a driver period into engine-sized render blocks.
//!
//! A driver hands us whatever it negotiated — ALSA a period, PipeWire a quantum
//! (commonly 1024, up to `clock.max-quantum`) — and that is independent of
//! `MAX_BLOCK_SIZE`, which is the engine's render CAPACITY. When the period is
//! larger, the period must be SPLIT across several `process_block` calls.
//!
//! Clamping instead of splitting is a silent defect: the PipeWire callback used
//! to do `num_samples = frames.min(MAX_BLOCK_SIZE)` and then report that clamped
//! count back in `chunk.size`, so the stream received fewer frames than its
//! quantum every cycle. This helper exists so the arithmetic is written once,
//! shared, and directly testable — the callbacks themselves are C FFI entry
//! points that cannot be exercised without a live server.

/// Successive `(offset, len)` render blocks covering `total_frames`, each at
/// most `max_block`.
///
/// Guarantees, all pinned by tests below: the blocks are contiguous from 0, they
/// sum to exactly `total_frames`, none is empty, and none exceeds `max_block`.
#[inline]
pub fn render_blocks(total_frames: usize, max_block: usize) -> RenderBlocks {
    RenderBlocks { offset: 0, total: total_frames, max_block: max_block.max(1) }
}

pub struct RenderBlocks {
    offset: usize,
    total: usize,
    max_block: usize,
}

impl Iterator for RenderBlocks {
    type Item = (usize, usize);

    #[inline]
    fn next(&mut self) -> Option<(usize, usize)> {
        if self.offset >= self.total {
            return None;
        }
        let len = (self.total - self.offset).min(self.max_block);
        let at = self.offset;
        self.offset += len;
        Some((at, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::MAX_BLOCK_SIZE;

    /// The property that matters: whatever the driver asks for, every frame is
    /// rendered exactly once. A gap is silence; an overlap is corruption.
    fn assert_exact_cover(total: usize, max_block: usize) {
        let blocks: Vec<(usize, usize)> = render_blocks(total, max_block).collect();

        let mut expected_offset = 0;
        for &(offset, len) in &blocks {
            assert_eq!(offset, expected_offset, "gap or overlap at {offset} (total {total})");
            assert!(len > 0, "empty block at {offset}");
            assert!(len <= max_block, "block of {len} exceeds max_block {max_block}");
            expected_offset += len;
        }
        assert_eq!(
            expected_offset, total,
            "blocks cover {expected_offset} of {total} frames"
        );
    }

    #[test]
    fn test_periods_larger_than_the_render_capacity_are_split_not_truncated() {
        // PipeWire's common quantum and its default max, against the engine
        // capacity. These are the sizes that used to be silently clamped.
        for total in [512usize, 1024, 2048, 4096] {
            assert_exact_cover(total, MAX_BLOCK_SIZE);
        }
    }

    #[test]
    fn test_periods_at_or_below_capacity_render_in_one_block() {
        for total in [1usize, 64, 128, 256, MAX_BLOCK_SIZE] {
            let blocks: Vec<_> = render_blocks(total, MAX_BLOCK_SIZE).collect();
            assert_eq!(blocks, vec![(0, total)], "{total} frames should be a single block");
        }
    }

    #[test]
    fn test_ragged_periods_are_covered_exactly() {
        // Nothing guarantees a driver period is a multiple of the block size.
        for total in [1usize, 257, 999, 1023, 1025, 3000] {
            assert_exact_cover(total, MAX_BLOCK_SIZE);
        }
        // And across a range of block sizes, including 1 and non-powers of two.
        for max_block in [1usize, 3, 64, 100, 256, 1024] {
            for total in [0usize, 1, 7, 100, 257, 1024, 2049] {
                assert_exact_cover(total, max_block);
            }
        }
    }

    #[test]
    fn test_zero_frames_yields_no_blocks() {
        assert_eq!(render_blocks(0, MAX_BLOCK_SIZE).count(), 0);
    }

    #[test]
    fn test_zero_max_block_does_not_hang() {
        // A misconfigured capacity must not produce an infinite loop of empty
        // blocks on the audio thread.
        let blocks: Vec<_> = render_blocks(4, 0).collect();
        assert_eq!(blocks, vec![(0, 1), (1, 1), (2, 1), (3, 1)]);
    }
}
