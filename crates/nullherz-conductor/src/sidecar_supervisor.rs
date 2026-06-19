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
        for processor in new_processors {
            eprintln!("Recovered sidecar process. Re-inserting into audio graph...");
            if let Some(ref mut prod) = topology_manager.topo_producer {
                // Automated recovery into node 0 (standard for current Conductor pattern)
                let _ = prod.push(TopologyMutation::SwapProcessor { node_idx: 0, processor });
            }
        }
    }
}
