use fx_runtime::SidecarManager;
use crate::topology_manager::TopologyManager;
use nullherz_traits::{TopologyMutation, TimestampedCommand};
use std::sync::Arc;
use tokio::sync::Mutex;
use ipc_layer::tcp::TcpIpcConsumer;
use tokio::io::AsyncReadExt;
use std::time::{Instant, Duration};

pub struct RemoteSidecar {
    pub addr: String,
    pub writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    pub last_heartbeat: Instant,
    pub is_active: bool,
}

pub struct RemoteSidecarManager {
    pub remote_nodes: Vec<RemoteSidecar>,
    pub pending_commands: Vec<TimestampedCommand>,
    pub last_broadcast_time: Instant,
}

impl RemoteSidecarManager {
    pub fn new() -> Self {
        Self {
            remote_nodes: Vec::new(),
            pending_commands: Vec::new(),
            last_broadcast_time: Instant::now(),
        }
    }

    pub async fn broadcast_command(&mut self, cmd: TimestampedCommand) {
        let serialized = match serde_json::to_vec(&cmd) {
            Ok(s) => s,
            Err(_) => return,
        };
        let len = serialized.len() as u32;

        for node in &mut self.remote_nodes {
            if let Ok(mut writer) = node.writer.try_lock() {
                use tokio::io::AsyncWriteExt;
                let mut full_payload = Vec::with_capacity(4 + serialized.len());
                full_payload.extend_from_slice(&len.to_be_bytes());
                full_payload.extend_from_slice(&serialized);
                let _ = writer.write_all(&full_payload).await;
            }
        }
    }
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
            remote_manager: Arc::new(Mutex::new(RemoteSidecarManager::new())),
        }
    }

    pub async fn listen_for_remote_sidecars(remote_manager: Arc<Mutex<RemoteSidecarManager>>, addr: &str) -> std::io::Result<()> {
        let consumer = TcpIpcConsumer::bind(addr).await?;
        println!("Conductor: Listening for remote sidecars on {}", addr);

        tokio::spawn(async move {
            loop {
                match consumer.accept().await {
                    Ok(stream) => {
                        let peer_addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string());
                        let remote_manager_clone = remote_manager.clone();
                        let addr_clone = peer_addr.clone();

                        let (mut reader, writer) = stream.into_split();
                        let writer_arc = Arc::new(Mutex::new(writer));

                        tokio::spawn(async move {
                            loop {
                                // 1. Read length prefix (u32)
                                let mut len_buf = [0u8; 4];
                                if reader.read_exact(&mut len_buf).await.is_err() { break; }
                                let len = u32::from_be_bytes(len_buf) as usize;

                                if len > 65536 { break; } // Safety limit

                                // 2. Read JSON payload
                                let mut buffer = vec![0u8; len];
                                if reader.read_exact(&mut buffer).await.is_err() { break; }

                                if let Ok(cmd) = serde_json::from_slice::<TimestampedCommand>(&buffer) {
                                    let mut manager = remote_manager_clone.lock().await;
                                    // Update heartbeat if this was a Ping or any command
                                    if let Some(node) = manager.remote_nodes.iter_mut().find(|n| n.addr == addr_clone) {
                                        node.last_heartbeat = Instant::now();
                                    }
                                    manager.pending_commands.push(cmd);
                                }
                                tokio::task::yield_now().await;
                            }
                            println!("Conductor: Remote sidecar disconnected from {}", addr_clone);
                        });

                        let mut manager = remote_manager.lock().await;
                        manager.remote_nodes.push(RemoteSidecar {
                            addr: peer_addr.clone(),
                            writer: writer_arc,
                            last_heartbeat: Instant::now(),
                            is_active: true,
                        });
                        println!("Conductor: Attached remote sidecar from {}", peer_addr);
                    }
                    Err(e) => {
                        eprintln!("Conductor: TCP accept error: {}. Backing off...", e);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn broadcast_to_remote(&mut self, cmd: TimestampedCommand) {
        let mut manager = self.remote_manager.lock().await;
        manager.broadcast_command(cmd).await;
    }

    pub fn supervise(&mut self, topology_manager: &mut TopologyManager) -> Vec<TimestampedCommand> {
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

        // 3. Drain pending commands from remote sidecars
        let mut remote_cmds = Vec::new();
        if let Ok(mut manager) = self.remote_manager.try_lock() {
            remote_cmds = std::mem::take(&mut manager.pending_commands);

            // 4. Prune disconnected nodes based on heartbeat timeout (5 seconds)
            let now = Instant::now();
            manager.remote_nodes.retain(|node| {
                if now.duration_since(node.last_heartbeat) > Duration::from_secs(5) {
                    eprintln!("Conductor: Remote sidecar {} timed out. Dropping...", node.addr);
                    false
                } else {
                    node.is_active
                }
            });
        }
        remote_cmds
    }
}
