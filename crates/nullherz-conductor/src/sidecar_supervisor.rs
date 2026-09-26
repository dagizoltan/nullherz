use fx_runtime::SidecarManager;
use crate::topology_manager::TopologyManager;
use crate::ipc_audio_bridge::IpcAudioBridge;
use nullherz_traits::{TopologyMutation, TimestampedCommand, SampleRegistry};
use std::sync::Arc;
use tokio::sync::Mutex;
use ipc_layer::tcp::{TcpIpcConsumer, TcpIpcProducer};
use tokio::io::AsyncReadExt;
use std::time::{Instant, Duration};
use tokio::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};

/// How often a listener wakes to notice `shutdown` when no packet arrives.
///
/// The listeners `select!` between their socket and this tick, so a quiet
/// socket still observes the flag within one interval. Polling an atomic is
/// what makes shutdown *bounded*; the socket side is fully event-driven, so
/// this timer costs one wakeup per interval per listener and nothing else.
const SHUTDOWN_POLL: Duration = Duration::from_millis(250);

pub struct RemoteSidecar {
    pub addr: String,
    pub writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    pub last_heartbeat: Instant,
    pub is_active: bool,
    pub mirrored_samples: std::collections::HashSet<u64>,
    pub pending_mirrored_samples: std::collections::HashSet<u64>,
    pub cpu_usage: f32,
    pub cpu_usage_trend: f32,
    pub latency_ms: f32,
    pub assigned_nodes: std::collections::HashSet<u32>,
}

pub struct RemoteSidecarManager {
    pub remote_nodes: Vec<RemoteSidecar>,
    pub pending_commands: Vec<TimestampedCommand>,
    pub last_broadcast_time: Instant,
    pub node_to_sidecar: std::collections::HashMap<u32, usize>,
}

impl RemoteSidecarManager {
    pub fn new() -> Self {
        Self {
            remote_nodes: Vec::new(),
            pending_commands: Vec::new(),
            last_broadcast_time: Instant::now(),
            node_to_sidecar: std::collections::HashMap::new(),
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

    pub fn predictive_migration_check(&mut self) -> Vec<(u32, String)> {
        let mut migrations = Vec::new();
        let num_nodes = self.remote_nodes.len();

        for i in 0..num_nodes {
            let (projected_cpu, node_addr, first_node_idx) = {
                let node = &self.remote_nodes[i];
                let proj = node.cpu_usage + node.cpu_usage_trend;
                let first_idx = node.assigned_nodes.iter().next().copied();
                (proj, node.addr.clone(), first_idx)
            };

            if projected_cpu > 0.85 {
                if let Some(node_idx) = first_node_idx {
                    // Find a candidate remote peer with lowest load
                    let candidate = self.remote_nodes.iter()
                        .filter(|n| n.addr != node_addr && n.is_active)
                        .min_by(|a, b| a.cpu_usage.partial_cmp(&b.cpu_usage).unwrap_or(std::cmp::Ordering::Equal));

                    if let Some(target) = candidate {
                        if target.cpu_usage < 0.5 {
                            migrations.push((node_idx, target.addr.clone()));
                        }
                    }
                }
            }
        }
        migrations
    }

    pub async fn send_audio_block(&mut self, node_idx: u32, block: nullherz_traits::AudioBlock) {
        let sidecar_idx = match self.node_to_sidecar.get(&node_idx) {
            Some(&idx) => idx,
            None => return,
        };

        let node = match self.remote_nodes.get_mut(sidecar_idx) {
            Some(n) => n,
            None => return,
        };

        // Payload: [u8 type:5][u32 node_idx][AudioBlock data]
        let mut payload = Vec::with_capacity(5 + std::mem::size_of::<nullherz_traits::AudioBlock>());
        payload.push(5u8);
        payload.extend_from_slice(&node_idx.to_be_bytes());
        payload.extend_from_slice(bytemuck::bytes_of(&block));

        let len = payload.len() as u32;

        if let Ok(mut writer) = node.writer.try_lock() {
            use tokio::io::AsyncWriteExt;
            let mut full_payload = Vec::with_capacity(4 + payload.len());
            full_payload.extend_from_slice(&len.to_be_bytes());
            full_payload.extend_from_slice(&payload);
            let _ = writer.write_all(&full_payload).await;
        }
    }

    pub async fn ensure_sample_mirrored(&mut self, sample_id: u64, registry: &dyn SampleRegistry) {
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
            if !node.mirrored_samples.contains(&sample_id) && !node.pending_mirrored_samples.contains(&sample_id) {
                node.pending_mirrored_samples.insert(sample_id);
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

pub struct HotStandbyProcess {
    pub sidecar_id: String,
    pub node_idx: u32,
    pub standby_processor: Option<Box<dyn nullherz_traits::AudioProcessor>>,
    pub last_ready: Instant,
}

pub struct SidecarSupervisor {
    pub manager: SidecarManager,
    pub remote_manager: Arc<Mutex<RemoteSidecarManager>>,
    pub hot_standbys: std::collections::HashMap<u32, HotStandbyProcess>,
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
            hot_standbys: std::collections::HashMap::new(),
        }
    }

    /// Register or refresh a pre-initialized hot-standby shadow process for a node
    pub fn register_hot_standby(
        &mut self,
        node_idx: u32,
        sidecar_id: impl Into<String>,
        standby_processor: Option<Box<dyn nullherz_traits::AudioProcessor>>,
    ) {
        self.hot_standbys.insert(node_idx, HotStandbyProcess {
            sidecar_id: sidecar_id.into(),
            node_idx,
            standby_processor,
            last_ready: Instant::now(),
        });
    }

    /// Pre-spawns a hot-standby shadow process instance for instant failover (<1.3 ms)
    pub fn spawn_hot_standby(
        &mut self,
        name: &str,
        binary_path: &str,
        node_idx: u32,
        num_channels: usize,
    ) -> Result<(), String> {
        let standby_id = format!("{}_standby", name);
        let processor = self.manager.spawn_sidecar(
            &standby_id,
            binary_path,
            node_idx,
            num_channels,
            fx_runtime::FailurePolicy::AutoRestart,
        )?;
        self.register_hot_standby(node_idx, name, Some(processor));
        Ok(())
    }

    /// UDP beacon listener: discovers sidecars announcing themselves.
    ///
    /// The socket is a `tokio::net::UdpSocket` and the receive is awaited. It
    /// used to be a blocking `std::net::UdpSocket` set non-blocking, polled
    /// once per 500 ms sleep — so a quiet network was fine but a busy one
    /// discovered at most two sidecars a second, and the poll interval was
    /// load-bearing for correctness rather than for pacing.
    pub async fn start_discovery_listener(
        remote_manager: Arc<Mutex<RemoteSidecarManager>>,
        audio_bridge: Arc<IpcAudioBridge>,
        port: u16,
        shutdown: Arc<AtomicBool>,
    ) -> std::io::Result<u16> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await?;
        let bound = socket.local_addr()?.port();
        println!("Conductor: UDP Discovery listening on port {}", bound);

        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            while !shutdown.load(Ordering::Relaxed) {
                let recv = tokio::select! {
                    r = socket.recv_from(&mut buf) => r,
                    _ = tokio::time::sleep(SHUTDOWN_POLL) => continue,
                };
                if let Ok((len, addr)) = recv {
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
                                                let node_idx = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]);
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
                                                    let now = Instant::now();
                                                    node.latency_ms = now.duration_since(node.last_heartbeat).as_secs_f32() * 1000.0;
                                                    node.last_heartbeat = now;
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
                                        pending_mirrored_samples: std::collections::HashSet::new(),
                                        cpu_usage: 0.0,
                                        cpu_usage_trend: 0.0,
                                        latency_ms: 0.0,
                                        assigned_nodes: std::collections::HashSet::new(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        });
        Ok(bound)
    }

    /// Audio-return listener (protocol type 6).
    ///
    /// This is the one that used to deadlock the process. It bound a BLOCKING
    /// `std::net::UdpSocket` and called `recv_from` inside `tokio::spawn`, which
    /// parks a runtime worker in the kernel until a packet arrives — forever, on
    /// a machine with no remote sidecars. A parked worker cannot be reclaimed, so
    /// `Runtime::drop` never completes and the process cannot exit: every test
    /// that built a `Conductor` hung at teardown, which is why
    /// `cargo test --workspace` never finished.
    pub async fn start_udp_return_listener(
        audio_bridge: Arc<IpcAudioBridge>,
        port: u16,
        shutdown: Arc<AtomicBool>,
    ) -> std::io::Result<u16> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await?;
        let bound = socket.local_addr()?.port();
        println!("Conductor: UDP Return listening on port {}", bound);

        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            while !shutdown.load(Ordering::Relaxed) {
                let recv = tokio::select! {
                    r = socket.recv_from(&mut buf) => r,
                    _ = tokio::time::sleep(SHUTDOWN_POLL) => continue,
                };
                if let Ok((len, _addr)) = recv
                    && len >= 5 && buf[0] == 6 {
                        let node_idx = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
                        let block_data = &buf[5..len];
                        if block_data.len() == std::mem::size_of::<nullherz_traits::AudioBlock>() {
                            let block: nullherz_traits::AudioBlock = bytemuck::pod_read_unaligned(block_data);
                            let _ = audio_bridge.push_block(node_idx, block);
                        }
                    }
            }
        });
        Ok(bound)
    }

    pub async fn listen_for_remote_sidecars(
        remote_manager: Arc<Mutex<RemoteSidecarManager>>,
        audio_bridge: Arc<IpcAudioBridge>,
        addr: &str,
        shutdown: Arc<AtomicBool>,
    ) -> std::io::Result<()> {
        let consumer = TcpIpcConsumer::bind(addr).await?;
        println!("Conductor: Listening for remote sidecars on {}", addr);

        tokio::spawn(async move {
            while !shutdown.load(Ordering::Relaxed) {
                let accepted = tokio::select! {
                    r = consumer.accept() => r,
                    _ = tokio::time::sleep(SHUTDOWN_POLL) => continue,
                };
                match accepted {
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
                                    let node_idx = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]);
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
                            pending_mirrored_samples: std::collections::HashSet::new(),
                            cpu_usage: 0.0,
                            cpu_usage_trend: 0.0,
                            latency_ms: 0.0,
                            assigned_nodes: std::collections::HashSet::new(),
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
        // 1. Identify stalled heartbeats and trigger SOFT FALLBACK or Hot-Standby Failover
        let stalled_nodes = self.manager.list_stalled_nodes();
        for node_idx in stalled_nodes {
            if let Some(mut standby) = self.hot_standbys.remove(&node_idx) {
                if let Some(proc) = standby.standby_processor.take() {
                    eprintln!("WARNING: Heartbeat stall detected for node {}. Instant failover to Hot-Standby Shadow ({})", node_idx, standby.sidecar_id);
                    if let Some(ref mut prod) = topology_manager.topo_producer {
                        let _ = prod.push(TopologyMutation::SwapProcessor { node_idx, processor: proc });
                        self.manager.mark_as_bypassed(node_idx);
                    }
                    continue;
                }
            }
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
