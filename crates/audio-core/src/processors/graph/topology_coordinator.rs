use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicUsize};
use crate::processors::graph::topology_types::GraphTopology;
use nullherz_topology::GraphCompiler;

pub struct TopologyCoordinator {
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

    pub fn apply_mutation(&mut self, mutation: crate::processors::TopologyMutation, nodes: &mut [super::node::ProcessorNode; crate::MAX_NODES], node_count: &mut usize, garbage_producer: &Option<Box<dyn nullherz_traits::GarbageProducer>>) {
        use crate::processors::TopologyMutation;
        match mutation {
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
                    topo.routing[n_idx].input_indices[i_idx] = new_buffer_idx.min(crate::MAX_NODES as u32 - 1);
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
                    topo.routing[n_idx].output_indices[o_idx] = new_buffer_idx.min(crate::MAX_NODES as u32 - 1);
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
