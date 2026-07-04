use fx_runtime::SidecarManager;
use crate::topology_manager::TopologyManager;
use crate::ipc_audio_bridge::IpcAudioBridge;
use nullherz_traits::{TopologyMutation, TimestampedCommand};
use std::sync::Arc;
use tokio::sync::Mutex;
use ipc_layer::tcp::{TcpIpcConsumer, TcpIpcProducer};
use tokio::io::AsyncReadExt;
use std::time::{Instant, Duration};
use std::net::UdpSocket;

pub struct RemoteSidecar {
    pub addr: String,
    pub writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    pub last_heartbeat: Instant,
    pub is_active: bool,
    pub mirrored_samples: std::collections::HashSet<u64>,
    pub cpu_usage: f32,
    pub latency_ms: f32,
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
        let serialized = match cmd.to_binary() {
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

    pub async fn ensure_sample_mirrored(&mut self, sample_id: u64, registry: &nullherz_dna::SampleRegistry) {
        let sample = match registry.get(sample_id) {
            Some(s) => s,
            None => return,
        };

        // Binary payload: [u32 len][u8 type:2][u64 id][u32 sample_count][f32 data...]
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u8.to_be_bytes()); // Type: Sample Data
        payload.extend_from_slice(&sample_id.to_be_bytes());
        payload.extend_from_slice(&(sample.buffer.len() as u32).to_be_bytes());
        let data_bytes = bytemuck::cast_slice(&sample.buffer);
        payload.extend_from_slice(data_bytes);

        let len = payload.len() as u32;

        for node in &mut self.remote_nodes {
            if !node.mirrored_samples.contains(&sample_id) {
                if let Ok(mut writer) = node.writer.try_lock() {
                    use tokio::io::AsyncWriteExt;
                    let mut full_payload = Vec::with_capacity(4 + payload.len());
                    full_payload.extend_from_slice(&len.to_be_bytes());
                    full_payload.extend_from_slice(&payload);
                    if writer.write_all(&full_payload).await.is_ok() {
                        node.mirrored_samples.insert(sample_id);
                        println!("Conductor: Mirrored sample {} to {}", sample_id, node.addr);
                    }
                }
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

    pub async fn start_discovery_listener(remote_manager: Arc<Mutex<RemoteSidecarManager>>, audio_bridge: Arc<IpcAudioBridge>, port: u16) -> std::io::Result<()> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
        socket.set_nonblocking(true)?;
        println!("Conductor: UDP Discovery listening on port {}", port);

        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                if let Ok((len, addr)) = socket.recv_from(&mut buf) {
                    let msg = String::from_utf8_lossy(&buf[..len]);
                    if msg.starts_with("nullherz_sidecar:") {
                        let sidecar_port = msg.split(':').nth(1).and_then(|p| p.parse::<u16>().ok()).unwrap_or(9001);
                        let sidecar_addr = format!("{}:{}", addr.ip(), sidecar_port);

                        let mut manager = remote_manager.lock().await;
                        if !manager.remote_nodes.iter().any(|n| n.addr == sidecar_addr) {
                            println!("Conductor: Discovered remote sidecar at {}. Attempting to attach...", sidecar_addr);
                            if let Ok(stream_prod) = TcpIpcProducer::connect(&sidecar_addr).await {
                                if let Ok(stream) = stream_prod.into_inner() {
                                    let (mut reader, writer) = stream.into_split();
                                    let writer_arc = Arc::new(Mutex::new(writer));
                                    let remote_manager_clone = remote_manager.clone();
                                    let audio_bridge_clone = audio_bridge.clone();
                                    let addr_clone = sidecar_addr.clone();

                                    tokio::spawn(async move {
                                        loop {
                                            let mut len_buf = [0u8; 4];
                                            if reader.read_exact(&mut len_buf).await.is_err() { break; }
                                            let len = u32::from_be_bytes(len_buf) as usize;
                                            if len > 65536 { break; }
                                            let mut buffer = vec![0u8; len];
                                            if reader.read_exact(&mut buffer).await.is_err() { break; }

                                            // Handle Audio Return Blocks (Type 3)
                                            if buffer.len() >= 5 && buffer[0] == 3 {
                                                let node_idx = u32::from_be_bytes(buffer[1..5].try_into().unwrap());
                                                let block_data = &buffer[5..];
                                                if block_data.len() == std::mem::size_of::<nullherz_traits::AudioBlock>() {
                                                     let block: nullherz_traits::AudioBlock = bytemuck::pod_read_unaligned(block_data);
                                                     let _ = audio_bridge_clone.push_block(node_idx, block);
                                                }
                                                continue;
                                            }

                                            let decoded = TimestampedCommand::from_binary(&buffer).ok();
                                            if let Some(cmd) = decoded {
                                                let mut manager = remote_manager_clone.lock().await;
                                                if let Some(node) = manager.remote_nodes.iter_mut().find(|n| n.addr == addr_clone) {
                                                    node.last_heartbeat = Instant::now();
                                                }
                                                manager.pending_commands.push(cmd);
                                            }
                                            tokio::task::yield_now().await;
                                        }
                                    });

                                    manager.remote_nodes.push(RemoteSidecar {
                                        addr: sidecar_addr,
                                        writer: writer_arc,
                                        last_heartbeat: Instant::now(),
                                        is_active: true,
                                        mirrored_samples: std::collections::HashSet::new(),
                                        cpu_usage: 0.0,
                                        latency_ms: 0.0,
                                    });
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
        Ok(())
    }

    pub async fn listen_for_remote_sidecars(remote_manager: Arc<Mutex<RemoteSidecarManager>>, audio_bridge: Arc<IpcAudioBridge>, addr: &str) -> std::io::Result<()> {
        let consumer = TcpIpcConsumer::bind(addr).await?;
        println!("Conductor: Listening for remote sidecars on {}", addr);

        tokio::spawn(async move {
            loop {
                match consumer.accept().await {
                    Ok(stream) => {
                        let peer_addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string());
                        let remote_manager_clone = remote_manager.clone();
                        let audio_bridge_clone = audio_bridge.clone();
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

                                // 2. Read Binary payload
                                let mut buffer = vec![0u8; len];
                                if reader.read_exact(&mut buffer).await.is_err() { break; }

                                // Handle Audio Return Blocks (Type 3)
                                if buffer.len() >= 5 && buffer[0] == 3 {
                                    let node_idx = u32::from_be_bytes(buffer[1..5].try_into().unwrap());
                                    let block_data = &buffer[5..];
                                    if block_data.len() == std::mem::size_of::<nullherz_traits::AudioBlock>() {
                                         let block: nullherz_traits::AudioBlock = bytemuck::pod_read_unaligned(block_data);
                                         let _ = audio_bridge_clone.push_block(node_idx, block);
                                    }
                                    continue;
                                }

                                let decoded = TimestampedCommand::from_binary(&buffer).ok();
                                if let Some(cmd) = decoded {
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
                            mirrored_samples: std::collections::HashSet::new(),
                            cpu_usage: 0.0,
                            latency_ms: 0.0,
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
                self.manager.mark_as_bypassed(node_idx);
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
