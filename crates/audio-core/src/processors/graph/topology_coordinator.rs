use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicUsize};
use crate::processors::graph::topology_types::GraphTopology;
use nullherz_topology::GraphCompiler;

pub struct TopologyCoordinator {
    /// True while the mutation ring is still mid-stream (the per-block drain
    /// hit its cap). Committing mid-stream made the double-buffered sides
    /// diverge nondeterministically (decks randomly missing); holding the
    /// commit until the final partial chunk applies the whole streamed batch
    /// atomically. Set each block by the engine's input handler.
    pub(crate) stream_pending: bool,
    pub(crate) topologies: Box<[GraphTopology; 2]>,
    pub(crate) active_idx: Arc<AtomicUsize>,
    pub(crate) needs_commit: bool,
}

/// Dispose of a processor from the audio thread without calling the allocator.
///
/// `Box<dyn AudioProcessor>` must never be dropped here: `Box::drop` runs
/// `free()`, and a processor's own `Drop` may free much more than the box. The
/// garbage ring exists so the non-RT side does it; when there is no ring the
/// correct answer is to LEAK, because a leak is a bug and a `free()` on the
/// audio thread is a dropout.
///
/// This is a function rather than a pattern because the pattern was already
/// written correctly in three places and omitted in two — the out-of-range
/// branches of `AddNode` and `SwapProcessor`, where the processor simply fell
/// out of scope. See `out_of_range_mutation_does_not_free_on_the_audio_thread`.
#[inline]
fn retire(
    processor: Box<dyn nullherz_traits::AudioProcessor>,
    garbage_producer: &mut Option<Box<dyn nullherz_traits::GarbageProducer>>,
) {
    match garbage_producer.as_deref_mut() {
        Some(prod) => {
            if let Err(leaked) = prod.push_processor(processor) {
                std::mem::forget(leaked);
            }
        }
        None => std::mem::forget(processor),
    }
}

impl TopologyCoordinator {
    pub fn new(initial_topo: GraphTopology) -> Self {
        Self {
            topologies: Box::new([initial_topo.clone(), initial_topo]),
            active_idx: Arc::new(AtomicUsize::new(0)),
            needs_commit: false,
            stream_pending: false,
        }
    }

    pub fn active_idx(&self) -> usize {
        self.active_idx.load(Ordering::Acquire)
    }

    pub fn active_topology(&self) -> &GraphTopology {
        &self.topologies[self.active_idx()]
    }

    pub fn inactive_topology_mut(&mut self) -> &mut GraphTopology {
        let active = self.active_idx();
        let inactive = (active + 1) % 2;
        if !self.needs_commit {
            self.topologies[inactive] = self.topologies[active].clone();
            self.needs_commit = true;
        }
        // Structural mutation invalidates the (cloned, now stale) plan so
        // commit() recompiles from current routing. Reusing a stale plan is
        // how the engine ends up executing only the first bootstrap batch's
        // nodes forever ("plan ping-pong" silence bug).
        self.topologies[inactive].plan.num_stages = 0;
        &mut self.topologies[inactive]
    }

    pub fn prepare_commit(&mut self) {
        let active = self.active_idx();
        let inactive = (active + 1) % 2;
        if let Ok(plan) = GraphCompiler::compile(&self.topologies[inactive]) {
            self.topologies[inactive].plan = plan;
        }
    }

    /// Swap the staged topology onto the active side. Runs on the AUDIO THREAD.
    ///
    /// # Why this no longer compiles
    ///
    /// It used to call `GraphCompiler::compile()` here when the staged plan was
    /// empty, with the comment "in a production system, we'd have pre-compiled
    /// this off-thread". That compile boxes a `[[usize; MAX_NODES]; MAX_NODES]`
    /// (128 KB) and a `[[usize; MAX_NODES]; MAX_BUFFERS]` (245 KB) — ~373 KB of
    /// fresh allocation on a SCHED_FIFO thread, per commit. The 245 KB box
    /// clears glibc's 128 KB mmap threshold, so it is a new mapping plus the
    /// first-touch page faults to zero it, not just arithmetic; measured
    /// compute alone was 17 µs at the 46-node console and 75 µs at 128 nodes.
    /// Every individually-pushed `AddNode`/`UpdateEdge` invalidates the plan
    /// (`inactive_topology_mut` zeroes `num_stages`), so this fired throughout
    /// console bootstrap.
    ///
    /// It is also redundant: `TopologyManager::handle_topology_command` already
    /// compiles off-thread and ships the finished plan inside `SetTopology`.
    /// This branch was the fallback for a plan that arrived uncompiled — and
    /// the right answer for that is to keep playing the topology we have, not
    /// to malloc on the audio thread.
    ///
    /// [`Self::prepare_commit`] is the off-thread entry point that does compile.
    pub fn commit(&mut self) -> Result<(), &'static str> {
        // Mid-stream: the input handler saw a full drain chunk, more
        // mutations are queued — apply them all before swapping.
        if self.stream_pending {
            return Ok(());
        }
        let active = self.active_idx();
        let inactive = (active + 1) % 2;

        if self.topologies[inactive].plan.num_stages == 0 && self.topologies[inactive].node_count > 0 {
            // Uncompiled. Refuse the swap and keep the active topology: audio
            // keeps flowing through the last good graph, and the caller logs.
            // Swapping anyway would install a plan with zero stages, which
            // renders silence.
            self.needs_commit = false;
            // `&'static str`, not `format!`. This runs on the audio thread, and
            // building an error message is a heap allocation like any other —
            // the RT zero-allocation test catches it, which is how this line
            // came to be written this way the second time.
            return Err(
                "staged topology has nodes but no compiled plan; refusing to swap \
                 (compiling here would allocate on the audio thread). The conductor \
                 must send a compiled plan via SetTopology."
            );
        }

        self.active_idx.store(inactive, Ordering::Release);
        self.needs_commit = false;
        Ok(())
    }

    pub fn has_active_crossfades(&self) -> bool {
        self.active_topology().crossfades.iter().any(|x| x.is_some())
    }

    pub fn apply_mutation(&mut self, mutation: crate::processors::TopologyMutation, nodes: &mut [super::node::ProcessorNode; crate::MAX_NODES], node_count: &mut usize, garbage_producer: &mut Option<Box<dyn nullherz_traits::GarbageProducer>>, faulted_states: &[std::sync::atomic::AtomicBool; crate::MAX_NODES]) {
        use crate::processors::TopologyMutation;
        match mutation {
            TopologyMutation::RemoveNode { node_idx } => {
                let idx = node_idx as usize;
                if idx < crate::MAX_NODES {
                    // 1. Swap with DummyProcessor and send the old one to garbage_producer
                    let dummy = Box::new(super::DummyProcessor) as Box<dyn nullherz_traits::AudioProcessor>;
                    let old_proc = unsafe { std::ptr::replace(nodes[idx].processor.get(), dummy) };
                    retire(old_proc, garbage_producer);

                    // 2. Clear faulted state for this node_idx
                    faulted_states[idx].store(false, Ordering::Relaxed);

                    // 3. Call inactive_topology_mut first to ensure inactive topology is initialized & cloned.
                    self.inactive_topology_mut();

                    let active = self.active_idx();
                    let inactive = (active + 1) % 2;

                    // Disconnect any edges (input or output) that reference this node_idx's buffers
                    let mut buffers_to_clear = std::collections::HashSet::new();

                    {
                        let topo = &mut self.topologies[inactive];
                        let r = &topo.routing[idx];
                        for &buf_idx in r.output_indices.iter().take(r.output_count) {
                            if buf_idx != nullherz_traits::BufferId(0) {
                                buffers_to_clear.insert(buf_idx);
                            }
                        }
                        for &buf_idx in r.input_indices.iter().take(r.input_count) {
                            if buf_idx != nullherz_traits::BufferId(0) {
                                buffers_to_clear.insert(buf_idx);
                            }
                        }

                        // Clear node's own routing
                        topo.routing[idx].input_indices.fill(nullherz_traits::BufferId(0));
                        topo.routing[idx].output_indices.fill(nullherz_traits::BufferId(0));
                        topo.routing[idx].sidechain_indices.fill(nullherz_traits::BufferId(0));
                        topo.routing[idx].input_count = 0;
                        topo.routing[idx].output_count = 0;
                        topo.routing[idx].sidechain_count = 0;
                        topo.routing[idx].input_delays.fill(0.0);

                        // Clear from other nodes
                        for other_idx in 0..crate::MAX_NODES {
                            if other_idx == idx { continue; }
                            let other_routing = &mut topo.routing[other_idx];
                            for i in 0..other_routing.input_count {
                                if buffers_to_clear.contains(&other_routing.input_indices[i]) {
                                    other_routing.input_indices[i] = nullherz_traits::BufferId(0);
                                }
                            }
                            for i in 0..other_routing.output_count {
                                if buffers_to_clear.contains(&other_routing.output_indices[i]) {
                                    other_routing.output_indices[i] = nullherz_traits::BufferId(0);
                                }
                            }
                        }
                    }

                    // Clear position, bypass states on active AND inactive topologies
                    self.topologies[active].node_positions[idx] = None;
                    self.topologies[active].bypass_states[idx] = false;
                    self.topologies[inactive].node_positions[idx] = None;
                    self.topologies[inactive].bypass_states[idx] = false;
                    self.topologies[active].node_assignments[idx] = nullherz_traits::NodeAssignment([0; 32]);
                    self.topologies[inactive].node_assignments[idx] = nullherz_traits::NodeAssignment([0; 32]);

                    // Update node_count for nodes and topologies
                    let mut max_idx = 0;
                    for i in (0..*node_count).rev() {
                        let proc_ptr = nodes[i].processor.get();
                        let is_dummy = unsafe { (*proc_ptr).as_any().is::<super::DummyProcessor>() };
                        if !is_dummy {
                            max_idx = i + 1;
                            break;
                        }
                    }
                    *node_count = max_idx;

                    let mut max_topo_idx = 0;
                    for i in (0..self.topologies[inactive].node_count).rev() {
                        let proc_ptr = nodes[i].processor.get();
                        let is_dummy = unsafe { (*proc_ptr).as_any().is::<super::DummyProcessor>() };
                        if !is_dummy {
                            max_topo_idx = i + 1;
                            break;
                        }
                    }
                    self.topologies[inactive].node_count = max_topo_idx;
                }
            }
            TopologyMutation::LoadProcessorState { node_idx, state_data } => {
                if let Some(node) = nodes.get_mut(node_idx as usize) {
                    let proc = unsafe { &mut *node.processor.get() };
                    proc.load_state(&state_data);
                }
            }
            TopologyMutation::Disconnect { node_idx, input_idx } => {
                // Shift later inputs down and shorten the list. A hole would be
                // read as a live input pointing at buffer 0 — silence mixed into
                // the signal rather than nothing — because `input_count` is what
                // the executor iterates.
                let n_idx = node_idx as usize;
                let i_idx = input_idx as usize;
                if n_idx < crate::MAX_NODES && i_idx < crate::MAX_CHANNELS {
                    let topo = self.inactive_topology_mut();
                    let r = &mut topo.routing[n_idx];
                    if i_idx < r.input_count {
                        for j in i_idx..r.input_count.saturating_sub(1) {
                            r.input_indices[j] = r.input_indices[j + 1];
                        }
                        r.input_count -= 1;
                        r.input_indices[r.input_count] = nullherz_traits::BufferId(0);
                    }
                }
            }
            TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let i_idx = input_idx as usize;
                // The conductor rejects out-of-range buffers before they get
                // here; a survivor is a bug. Drop the mutation rather than
                // clamp — clamping aliases a buffer some other edge owns.
                debug_assert!(new_buffer_idx < crate::MAX_BUFFERS as u32,
                    "UpdateEdge buffer {} escaped conductor validation", new_buffer_idx);
                if n_idx < crate::MAX_NODES && i_idx < crate::MAX_CHANNELS && new_buffer_idx < crate::MAX_BUFFERS as u32 {
                    let topo = self.inactive_topology_mut();
                    topo.routing[n_idx].input_indices[i_idx] = nullherz_traits::BufferId(new_buffer_idx);
                    if i_idx >= topo.routing[n_idx].input_count {
                        topo.routing[n_idx].input_count = i_idx + 1;
                    }
                }
            }
            TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let o_idx = output_idx as usize;
                debug_assert!(new_buffer_idx < crate::MAX_BUFFERS as u32,
                    "UpdateOutputEdge buffer {} escaped conductor validation", new_buffer_idx);
                if n_idx < crate::MAX_NODES && o_idx < crate::MAX_CHANNELS && new_buffer_idx < crate::MAX_BUFFERS as u32 {
                    let topo = self.inactive_topology_mut();
                    topo.routing[n_idx].output_indices[o_idx] = nullherz_traits::BufferId(new_buffer_idx);
                    if o_idx >= topo.routing[n_idx].output_count {
                        topo.routing[n_idx].output_count = o_idx + 1;
                    }
                }
            }
            TopologyMutation::SwapProcessor { node_idx, mut processor } => {
                let n_idx = node_idx as usize;
                if n_idx >= crate::MAX_NODES {
                    // Same as AddNode: a sentinel aimed at the engine lands here.
                    retire(processor, garbage_producer);
                } else {
                    if let Some(prod) = garbage_producer.as_deref() { processor.set_garbage_producer(prod); }
                    // Straight into the ring. This used to `clone_box` the
                    // producer first — a second heap allocation per mutation,
                    // on the audio thread, purely to satisfy
                    // `push_processor(&mut self)` through a shared ref.
                    let old_proc = unsafe { std::ptr::replace(nodes[n_idx].processor.get(), processor) };
                    retire(old_proc, garbage_producer);
                }
            }
            TopologyMutation::AddNode { node_idx, mut processor } => {
                let idx = node_idx as usize;
                if idx >= crate::MAX_NODES {
                    // Reachable by design: NodeConventions sentinels live above
                    // MAX_NODES precisely so this guard discards them. Discard
                    // must not mean `free()` on the audio thread.
                    retire(processor, garbage_producer);
                } else {
                    if let Some(prod) = garbage_producer.as_deref() { processor.set_garbage_producer(prod); }
                    // Straight into the ring. This used to `clone_box` the
                    // producer first — a second heap allocation per mutation,
                    // on the audio thread, purely to satisfy
                    // `push_processor(&mut self)` through a shared ref.
                    let old_proc = unsafe { std::ptr::replace(nodes[idx].processor.get(), processor) };
                    retire(old_proc, garbage_producer);

                    if idx >= *node_count { *node_count = idx + 1; }
                    let topo = self.inactive_topology_mut();
                    topo.routing[idx].input_count = 0;
                    topo.routing[idx].output_count = 0;
                    if idx >= topo.node_count { topo.node_count = idx + 1; }
                }
            }
            TopologyMutation::SetTopology(topo) => {
                let inactive = (self.active_idx() + 1) % 2;
                self.topologies[inactive] = topo.as_ref().clone();
                self.needs_commit = true;
            }
            TopologyMutation::AddSource { node_idx, buffer, sample_id, metadata } => {
                let idx = node_idx as usize;
                if idx < *node_count {
                    unsafe { (*nodes[idx].processor.get()).apply_topology_mutation(TopologyMutation::AddSource { node_idx, buffer, sample_id, metadata }); }
                }
            }
            TopologyMutation::UpdateMetadata { node_idx, metadata } => {
                let idx = node_idx as usize;
                if idx < *node_count {
                    unsafe { (*nodes[idx].processor.get()).apply_topology_mutation(TopologyMutation::UpdateMetadata { node_idx, metadata }); }
                }
            }
            TopologyMutation::SetNodePosition { node_idx, x, y } => {
                let n_idx = node_idx as usize;
                if n_idx < crate::MAX_NODES {
                    let inactive = (self.active_idx() + 1) % 2;
                    self.topologies[inactive].node_positions[n_idx] = Some((x, y));
                    self.topologies[self.active_idx()].node_positions[n_idx] = Some((x, y));
                }
            }
            TopologyMutation::SetBypass { node_idx, enabled } => {
                let n_idx = node_idx as usize;
                if n_idx < crate::MAX_NODES {
                    let inactive = (self.active_idx() + 1) % 2;
                    self.topologies[inactive].bypass_states[n_idx] = enabled;
                    self.topologies[self.active_idx()].bypass_states[n_idx] = enabled;
                }
            }
        }
    }
}

#[cfg(test)]
mod out_of_range_tests {
    use super::*;
    use crate::processors::TopologyMutation;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    /// Records its own destruction, so a test can ask WHERE it was freed.
    struct DropSpy(Arc<AtomicUsize>);
    impl Drop for DropSpy {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    impl std::fmt::Debug for DropSpy {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "DropSpy") }
    }
    impl nullherz_traits::SignalProcessor for DropSpy {
        fn process(&mut self, _i: &[&[f32]], _o: &mut [&mut [f32]], _c: &mut nullherz_traits::ProcessContext) {}
    }
    impl nullherz_traits::MidiResponder for DropSpy {}
    impl nullherz_traits::SnapshotProvider for DropSpy {}
    impl nullherz_traits::AudioProcessor for DropSpy {
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    /// Collects what the coordinator hands it, and keeps it alive — exactly
    /// what the real garbage ring does until the non-RT side drains it.
    #[derive(Clone)]
    #[allow(clippy::disallowed_types)]
    struct CollectingGarbage(Arc<std::sync::Mutex<Vec<Box<dyn nullherz_traits::AudioProcessor>>>>);
    impl nullherz_traits::GarbageProducer for CollectingGarbage {
        #[allow(clippy::disallowed_methods)]
        fn push_processor(&mut self, processor: Box<dyn nullherz_traits::AudioProcessor>)
            -> Result<(), Box<dyn nullherz_traits::AudioProcessor>>
        {
            self.0.lock().expect("test mutex").push(processor);
            Ok(())
        }
    }

    fn empty_topology() -> GraphTopology {
        let mut v2p = [nullherz_traits::BufferId(0); crate::MAX_BUFFERS];
        for (i, v) in v2p.iter_mut().enumerate() { *v = nullherz_traits::BufferId(i as u32); }
        GraphTopology {
            routing: [nullherz_traits::NodeRouting {
                input_indices: [nullherz_traits::BufferId(0); crate::MAX_CHANNELS],
                output_indices: [nullherz_traits::BufferId(0); crate::MAX_CHANNELS],
                sidechain_indices: [nullherz_traits::BufferId(0); crate::MAX_CHANNELS],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; crate::MAX_CHANNELS],
            }; crate::MAX_NODES],
            virtual_to_physical: v2p,
            plan: Default::default(),
            crossfades: [None; crate::MAX_CROSSFADE_BUFFERS],
            node_count: 0,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); crate::MAX_NODES],
            node_positions: [None; crate::MAX_NODES],
            bypass_states: [false; crate::MAX_NODES],
        }
    }

    fn fresh_nodes() -> Box<[super::super::node::ProcessorNode; crate::MAX_NODES]> {
        Box::new(std::array::from_fn(|_| super::super::node::ProcessorNode {
            processor: std::cell::UnsafeCell::new(
                Box::new(super::super::DummyProcessor) as Box<dyn nullherz_traits::AudioProcessor>
            ),
        }))
    }

    /// An out-of-range `AddNode`/`SwapProcessor` must not FREE its processor
    /// here — this runs on the audio thread.
    ///
    /// Every in-range path in this file is careful about it: the displaced
    /// processor goes to the garbage ring, and `std::mem::forget` is used when
    /// there is no ring, deliberately leaking rather than calling the allocator
    /// on the RT thread. The out-of-range branch did neither. It was a bare
    /// `if idx < MAX_NODES { ... }` with no `else`, so the `processor` binding
    /// fell out of scope and `Box::drop` ran `free()` inline.
    ///
    /// It is reachable BY DESIGN, not by accident: `NodeConventions` places its
    /// logical sentinels at `0xFFFF_FF00+` specifically so these guards discard
    /// them, and its doc comment says so. Every sentinel that reaches the engine
    /// as one of these two mutations was a free on the audio thread.
    ///
    /// The allocation guard in `test_kit::rt_alloc` cannot catch this — it
    /// counts `alloc` and deliberately not `dealloc` — so the property is
    /// tested directly instead: the processor must still be alive when
    /// `apply_mutation` returns.
    #[test]
    fn out_of_range_mutation_does_not_free_on_the_audio_thread() {
        for (label, make) in [
            ("AddNode", 0u8),
            ("SwapProcessor", 1u8),
        ] {
            let drops = Arc::new(AtomicUsize::new(0));
            #[allow(clippy::disallowed_types)]
            let collected = Arc::new(std::sync::Mutex::new(Vec::new()));
            let mut garbage: Option<Box<dyn nullherz_traits::GarbageProducer>> =
                Some(Box::new(CollectingGarbage(collected.clone())));

            let mut coord = TopologyCoordinator::new(empty_topology());
            let mut nodes = fresh_nodes();
            let mut node_count = 0usize;
            let faulted: [std::sync::atomic::AtomicBool; crate::MAX_NODES] =
                std::array::from_fn(|_| std::sync::atomic::AtomicBool::new(false));

            let spy = Box::new(DropSpy(drops.clone())) as Box<dyn nullherz_traits::AudioProcessor>;
            // A logical sentinel — the exact value NodeConventions documents as
            // being discarded by this guard.
            let node_idx = nullherz_traits::NodeConventions::PREVIEW;
            let mutation = if make == 0 {
                TopologyMutation::AddNode { node_idx, processor: spy }
            } else {
                TopologyMutation::SwapProcessor { node_idx, processor: spy }
            };

            coord.apply_mutation(mutation, nodes.as_mut(), &mut node_count, &mut garbage, &faulted);

            assert_eq!(
                drops.load(Ordering::Relaxed), 0,
                "{label} with an out-of-range index freed its processor on the audio thread"
            );
            #[allow(clippy::disallowed_methods)]
            {
                assert_eq!(
                    collected.lock().expect("test mutex").len(), 1,
                    "{label} must hand the rejected processor to the garbage ring for the non-RT side to free"
                );
            }
            // And it must not have touched the graph.
            assert_eq!(node_count, 0, "{label} out of range must not grow node_count");
        }
    }
}
