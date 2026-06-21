use fx_runtime::SidecarManager;
use crate::topology_manager::TopologyManager;
use nullherz_traits::TopologyMutation;

pub struct SidecarSupervisor {
    pub manager: SidecarManager,
}

impl SidecarSupervisor {
    pub fn new() -> Self {
        Self {
            manager: SidecarManager::new(),
        }
    }

    pub fn supervise(&mut self, topology_manager: &mut TopologyManager) {
        let new_processors = self.manager.reap_zombies();
        for (node_idx, processor) in new_processors {
            eprintln!("Recovered sidecar process for node {}. Re-inserting into audio graph...", node_idx);
            if let Some(ref mut prod) = topology_manager.topo_producer {
                let _ = prod.push(TopologyMutation::SwapProcessor { node_idx, processor });
            }
        }
    }
}
