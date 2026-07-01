use fx_runtime::SidecarManager;
use crate::topology_manager::TopologyManager;
use nullherz_traits::TopologyMutation;

pub struct SidecarSupervisor {
    pub manager: SidecarManager,
}

impl Default for SidecarSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl SidecarSupervisor {
    pub fn new() -> Self {
        Self {
            manager: SidecarManager::new(),
        }
    }

    pub fn supervise(&mut self, topology_manager: &mut TopologyManager) {
        // 1. Identify stalled heartbeats and trigger SOFT FALLBACK
        let stalled_nodes = self.manager.list_stalled_nodes();
        for node_idx in stalled_nodes {
            eprintln!("WARNING: Heartbeat stall detected for node {}. Triggering Soft Fallback...", node_idx);
            let fallback = Box::new(nullherz_processors::FallbackProcessor::new(node_idx as u64));
            if let Some(ref mut prod) = topology_manager.topo_producer {
                let _ = prod.push(TopologyMutation::SwapProcessor { node_idx, processor: fallback });
            }
        }

        // 2. Reap zombies and restore recovered processors
        let new_processors = self.manager.reap_zombies();
        for (node_idx, processor) in new_processors {
            eprintln!("Recovered sidecar process for node {}. Re-inserting into audio graph...", node_idx);
            if let Some(ref mut prod) = topology_manager.topo_producer {
                let _ = prod.push(TopologyMutation::SwapProcessor { node_idx, processor });
            }
        }
    }
}
