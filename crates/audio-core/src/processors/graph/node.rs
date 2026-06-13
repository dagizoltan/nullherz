use crate::processors::AudioProcessor;

pub struct ProcessorNode {
    pub processor: std::cell::UnsafeCell<Box<dyn AudioProcessor>>,
}

// SAFETY: ProcessorNode is Send/Sync despite containing UnsafeCell because:
// 1. STAGE INDEPENDENCE: The execution graph is partitioned into topological stages using Kahn's algorithm.
//    By definition, all nodes within a single stage are mutually independent (no direct or indirect data
//    dependencies). Thus, no node index is ever repeated within a stage.
// 2. THREAD EXCLUSION: During parallel execution, the TaskPool worker threads are assigned distinct,
//    disjoint node indices. No two worker threads are ever dispatched a Job pointing to the same ProcessorNode
//    at the same time. This is enforced by the topological scheduler which ensures no RAW, WAR, or WAW hazards
//    exist within a parallel stage.
// 3. NO STRUCTURAL MUTATION: The `nodes` array in `ProcessorGraph` is not modified structurally (no reallocations,
//    insertions, or deletions) while the audio thread is executing `process_block`. Topology mutations
//    are queued and processed synchronously between block processing cycles.
// 4. ALIASING GUARANTEE: Since each thread receives a pointer to a unique `ProcessorNode` within the stage,
//    each thread accesses a disjoint `UnsafeCell`. This guarantees that no two threads can ever construct
//    overlapping or concurrent mutable references (`&mut dyn AudioProcessor`) to the same underlying processor.
//    The `UnsafeCell::get()` is only called within the TaskPool worker after the stage fencing has guaranteed
//    no other thread is accessing the same processor.
unsafe impl Send for ProcessorNode {}
unsafe impl Sync for ProcessorNode {}

#[derive(Debug)]
pub struct DummyProcessor;
impl AudioProcessor for DummyProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {}
}
