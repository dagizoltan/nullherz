use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicUsize};
use crate::processors::graph::topology_types::GraphTopology;
use crate::processors::graph::compiler::GraphCompiler;

pub struct TopologyCoordinator {
    pub(crate) topologies: Box<[GraphTopology; 2]>,
    pub(crate) active_idx: Arc<AtomicUsize>,
    pub(crate) needs_commit: bool,
}

impl TopologyCoordinator {
    pub fn new(initial_topo: GraphTopology) -> Self {
        Self {
            topologies: Box::new([initial_topo; 2]),
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
            self.topologies[inactive] = self.topologies[active];
            self.needs_commit = true;
        }
        &mut self.topologies[inactive]
    }

    pub fn prepare_commit(&mut self) {
        let active = self.active_idx();
        let inactive = (active + 1) % 2;
        GraphCompiler::calculate_stages(&mut self.topologies[inactive]);
    }

    pub fn commit(&mut self) -> Result<(), String> {
        let active = self.active_idx();
        let inactive = (active + 1) % 2;

        if self.topologies[inactive].num_stages == 0 && self.topologies[inactive].node_count > 0 {
            return Err("Cannot commit empty topology with nodes".into());
        }

        if let Err(msg) = GraphCompiler::verify_no_hazards(&self.topologies[inactive]) {
            return Err(format!("Hazard detected: {}", msg));
        }

        self.active_idx.store(inactive, Ordering::Release);
        self.needs_commit = false;
        Ok(())
    }

    pub fn has_active_crossfades(&self) -> bool {
        self.active_topology().crossfades.iter().any(|x| x.is_some())
    }
}
