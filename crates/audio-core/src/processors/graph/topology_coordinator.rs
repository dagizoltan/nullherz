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

    pub fn commit(&mut self) -> Result<(), String> {
        // Mid-stream: the input handler saw a full drain chunk, more
        // mutations are queued — apply them all before swapping.
        if self.stream_pending {
            return Ok(());
        }
        let active = self.active_idx();
        let inactive = (active + 1) % 2;

        if self.topologies[inactive].plan.num_stages == 0 && self.topologies[inactive].node_count > 0 {
            // Re-run compilation to be sure.
            // In a production system, we'd have pre-compiled this off-thread.
            match GraphCompiler::compile(&self.topologies[inactive]) {
                Ok(plan) => self.topologies[inactive].plan = plan,
                Err(e) => return Err(format!("Compilation failed: {}", e)),
            }
        }

        self.active_idx.store(inactive, Ordering::Release);
        self.needs_commit = false;
        Ok(())
    }

    pub fn has_active_crossfades(&self) -> bool {
        self.active_topology().crossfades.iter().any(|x| x.is_some())
    }

    pub fn apply_mutation(&mut self, mutation: crate::processors::TopologyMutation, nodes: &mut [super::node::ProcessorNode; crate::MAX_NODES], node_count: &mut usize, garbage_producer: &Option<Box<dyn nullherz_traits::GarbageProducer>>, faulted_states: &[std::sync::atomic::AtomicBool; crate::MAX_NODES]) {
        use crate::processors::TopologyMutation;
        match mutation {
            TopologyMutation::RemoveNode { node_idx } => {
                let idx = node_idx as usize;
                if idx < crate::MAX_NODES {
                    // 1. Swap with DummyProcessor and send the old one to garbage_producer
                    let dummy = Box::new(super::DummyProcessor) as Box<dyn nullherz_traits::AudioProcessor>;
                    let old_proc = unsafe { std::ptr::replace(nodes[idx].processor.get(), dummy) };
                    if let Some(prod) = garbage_producer {
                        let mut cloned = dyn_clone::clone_box(&**prod);
                        if let Err(leaked) = cloned.push_processor(old_proc) { std::mem::forget(leaked); }
                    } else { std::mem::forget(old_proc); }

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
                            if buf_idx != 0 {
                                buffers_to_clear.insert(buf_idx);
                            }
                        }
                        for &buf_idx in r.input_indices.iter().take(r.input_count) {
                            if buf_idx != 0 {
                                buffers_to_clear.insert(buf_idx);
                            }
                        }

                        // Clear node's own routing
                        topo.routing[idx].input_indices.fill(0);
                        topo.routing[idx].output_indices.fill(0);
                        topo.routing[idx].sidechain_indices.fill(0);
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
                                    other_routing.input_indices[i] = 0;
                                }
                            }
                            for i in 0..other_routing.output_count {
                                if buffers_to_clear.contains(&other_routing.output_indices[i]) {
                                    other_routing.output_indices[i] = 0;
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
            TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let i_idx = input_idx as usize;
                if n_idx < crate::MAX_NODES && i_idx < crate::MAX_CHANNELS {
                    let topo = self.inactive_topology_mut();
                    topo.routing[n_idx].input_indices[i_idx] = new_buffer_idx.min(crate::MAX_BUFFERS as u32 - 1);
                    if i_idx >= topo.routing[n_idx].input_count {
                        topo.routing[n_idx].input_count = i_idx + 1;
                    }
                }
            }
            TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let o_idx = output_idx as usize;
                if n_idx < crate::MAX_NODES && o_idx < crate::MAX_CHANNELS {
                    let topo = self.inactive_topology_mut();
                    topo.routing[n_idx].output_indices[o_idx] = new_buffer_idx.min(crate::MAX_BUFFERS as u32 - 1);
                    if o_idx >= topo.routing[n_idx].output_count {
                        topo.routing[n_idx].output_count = o_idx + 1;
                    }
                }
            }
            TopologyMutation::SwapProcessor { node_idx, mut processor } => {
                let n_idx = node_idx as usize;
                if n_idx < crate::MAX_NODES {
                    if let Some(prod) = garbage_producer { processor.set_garbage_producer(dyn_clone::clone_box(&**prod)); }
                    let old_proc = unsafe { std::ptr::replace(nodes[n_idx].processor.get(), processor) };
                    if let Some(prod) = garbage_producer {
                        let mut cloned = dyn_clone::clone_box(&**prod);
                        if let Err(leaked) = cloned.push_processor(old_proc) { std::mem::forget(leaked); }
                    } else { std::mem::forget(old_proc); }
                }
            }
            TopologyMutation::AddNode { node_idx, mut processor } => {
                let idx = node_idx as usize;
                if idx < crate::MAX_NODES {
                    if let Some(prod) = garbage_producer { processor.set_garbage_producer(dyn_clone::clone_box(&**prod)); }
                    let old_proc = unsafe { std::ptr::replace(nodes[idx].processor.get(), processor) };
                    if let Some(prod) = garbage_producer {
                        let mut cloned = dyn_clone::clone_box(&**prod);
                        if let Err(leaked) = cloned.push_processor(old_proc) { std::mem::forget(leaked); }
                    } else { std::mem::forget(old_proc); }

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
