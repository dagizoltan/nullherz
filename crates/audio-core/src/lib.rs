/// Base trait for any audio processing unit.
///
/// Implementations must ensure that `process` is real-time safe:
/// - No allocations
/// - No locks
/// - No I/O
pub trait AudioProcessor {
    /// Process a block of audio.
    /// `buffer` contains interleaved or de-interleaved samples depending on the implementation.
    /// For this system, we aim for de-interleaved (planar) buffers.
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);
}
