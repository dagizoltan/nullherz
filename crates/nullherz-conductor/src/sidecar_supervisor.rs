use fx_runtime::SidecarManager;
use crate::topology_manager::TopologyManager;
use nullherz_traits::TopologyMutation;
use std::sync::Arc;
use tokio::sync::Mutex;
use ipc_layer::tcp::TcpIpcConsumer;

pub struct RemoteSidecar {
    pub addr: String,
    pub stream: Arc<Mutex<tokio::net::TcpStream>>,
}

pub struct RemoteSidecarManager {
    pub remote_nodes: Vec<RemoteSidecar>,
}

pub struct SidecarSupervisor {
    pub manager: SidecarManager,
    pub remote_manager: Arc<Mutex<RemoteSidecarManager>>,
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
            remote_manager: Arc::new(Mutex::new(RemoteSidecarManager { remote_nodes: Vec::new() })),
        }
    }

    pub async fn listen_for_remote_sidecars(remote_manager: Arc<Mutex<RemoteSidecarManager>>, addr: &str) -> std::io::Result<()> {
        let consumer = TcpIpcConsumer::bind(addr).await?;
        let addr_string = addr.to_string();
        println!("Conductor: Listening for remote sidecars on {}", addr_string);

        tokio::spawn(async move {
            loop {
                if let Ok(stream) = consumer.accept().await {
                    let mut manager = remote_manager.lock().await;
                    manager.remote_nodes.push(RemoteSidecar {
                        addr: addr_string.clone(),
                        stream: Arc::new(Mutex::new(stream)),
                    });
                    println!("Conductor: Attached remote sidecar from source {}", addr_string);
                }
            }
        });

        Ok(())
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
