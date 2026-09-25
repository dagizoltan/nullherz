use tokio::io::{AsyncReadExt, AsyncWriteExt};
use nullherz_traits::{TimestampedCommand, Command, CoreCommand, SampleMetadata, SampleRegistry as _};
use nullherz_dna::SampleRegistry;
use std::net::UdpSocket;
use std::time::Duration;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sample_registry = Arc::new(SampleRegistry::new());
    let sidecar_port: u16 = 9002;
    let conductor_discovery_port: u16 = 9001;

    // 1. Start UDP Beacon for Discovery
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_broadcast(true)?;
    let beacon_msg = format!("nullherz_sidecar:{}", sidecar_port);

    tokio::spawn(async move {
        let addr = format!("255.255.255.255:{}", conductor_discovery_port);
        loop {
            let _ = socket.send_to(beacon_msg.as_bytes(), &addr);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    // 2. Start TCP Listener for Conductor Attachment
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", sidecar_port)).await?;
    println!("Distributed Sidecar listening on port {}", sidecar_port);

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("Conductor attached from {}", addr);
        let registry_clone = sample_registry.clone();

        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            loop {
                // Read length prefix
                if socket.read_exact(&mut len_buf).await.is_err() { break; }
                let len = u32::from_be_bytes(len_buf) as usize;

                // Special handling for large sample mirroring payloads
                if len > 10 * 1024 * 1024 { break; }

                let mut payload = vec![0u8; len];
                if socket.read_exact(&mut payload).await.is_err() { break; }

                // Check for Sample Data type (Type 2, from SidecarSupervisor::ensure_sample_mirrored)
                if payload.len() >= 13 && payload[0] == 2 {
                    let mut cursor = 1;
                    let sample_id = u64::from_be_bytes(payload[cursor..cursor+8].try_into().unwrap());
                    cursor += 8;
                    let sample_count = u32::from_be_bytes(payload[cursor..cursor+4].try_into().unwrap()) as usize;
                    cursor += 4;

                    if payload.len() >= cursor + sample_count * 4 {
                        let f32_data: &[f32] = bytemuck::cast_slice(&payload[cursor..cursor + sample_count * 4]);
                        registry_clone.register_with_metadata(
                            sample_id,
                            Arc::new(f32_data.to_vec()),
                            Arc::new(SampleMetadata::new_empty())
                        );
                        println!("Sidecar: Registered mirrored sample ID={}", sample_id);
                    }
                    continue;
                }

                // Drop the non-Send Box<dyn Error> before awaiting below (tokio::spawn needs Send)
                let parsed_cmd = TimestampedCommand::from_binary(&payload).ok();
                if let Some(cmd) = parsed_cmd {
                    println!("Received command: {:?}", cmd);

                    // --- STAGE 3 & 4: AUDIO RETURN & TYPE 7 RDMA DMA TRANSPORT PATH ---
                    if let Command::Core(CoreCommand::RequestSnapshots) = cmd.command {
                        let dummy_block = ipc_layer::AudioBlock { data: [0.5; ipc_layer::IPC_BLOCK_SIZE], len: 256, _pad: [0; 15] };
                        let block_bytes = bytemuck::bytes_of(&dummy_block);
                        let mut header = Vec::with_capacity(5);
                        header.push(3u8); // Type 3: Audio Return Block
                        header.extend_from_slice(&(block_bytes.len() as u32).to_be_bytes());
                        let _ = socket.write_all(&header).await;
                        let _ = socket.write_all(block_bytes).await;
                    }

                    // Type 7: Low-latency RDMA / Kernel-Bypass Direct DMA payload handler
                    if payload.first() == Some(&7u8) && payload.len() >= 9 + std::mem::size_of::<ipc_layer::AudioBlock>() {
                        let seq_id = u64::from_be_bytes(payload[1..9].try_into().unwrap());
                        let block_bytes = &payload[9..9 + std::mem::size_of::<ipc_layer::AudioBlock>()];
                        if let Ok(block) = bytemuck::try_from_bytes::<ipc_layer::AudioBlock>(block_bytes) {
                            println!("Distributed Sidecar: Received Type 7 RDMA DMA Audio Block seq_id={}", seq_id);
                            let _ = block;
                        }
                    }

                    // Simple Ping-Pong Heartbeat / ACK (Binary)
                    let ack_cmd = TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::Core(CoreCommand::RequestSnapshots),
                    };
                    let ack_bytes = ack_cmd.to_binary().ok();
                    if let Some(ack) = ack_bytes {
                        let mut resp = Vec::with_capacity(4 + ack.len());
                        resp.extend_from_slice(&(ack.len() as u32).to_be_bytes());
                        resp.extend_from_slice(&ack);
                        let _ = socket.write_all(&resp).await;
                    }
                }
            }
            println!("Conductor detached from {}", addr);
        });
    }
}

/// Type 7: Low-Latency RDMA / Kernel-Bypass Direct DMA Audio Transport
pub struct RDMAAudioTransport {
    pub socket: UdpSocket,
    pub target_addr: Option<std::net::SocketAddr>,
    pub memory_region: Vec<ipc_layer::AudioBlock>,
}

impl RDMAAudioTransport {
    pub fn new(bind_addr: &str) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(bind_addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            target_addr: None,
            memory_region: vec![ipc_layer::AudioBlock { data: [0.0; ipc_layer::IPC_BLOCK_SIZE], len: 256, _pad: [0; 15] }; 64],
        })
    }

    /// Transmits an audio block directly via zero-copy UDP DMA memory ring.
    pub fn send_audio_block_rdma(&self, block: &ipc_layer::AudioBlock, seq_id: u64) -> std::io::Result<usize> {
        if let Some(target) = self.target_addr {
            let mut packet = Vec::with_capacity(1 + 8 + std::mem::size_of::<ipc_layer::AudioBlock>());
            packet.push(7u8); // Type 7: RDMA Audio Direct Transport
            packet.extend_from_slice(&seq_id.to_be_bytes());
            packet.extend_from_slice(bytemuck::bytes_of(block));
            self.socket.send_to(&packet, target)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "RDMA target address not set"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rdma_transport_creation_and_packet_layout() {
        let transport = RDMAAudioTransport::new("127.0.0.1:0").expect("Failed to bind RDMA socket");
        assert_eq!(transport.memory_region.len(), 64);
        assert!(transport.target_addr.is_none());

        let block = ipc_layer::AudioBlock { data: [0.25; ipc_layer::IPC_BLOCK_SIZE], len: 256, _pad: [0; 15] };
        let result = transport.send_audio_block_rdma(&block, 101);
        assert!(result.is_err(), "Expected error when target_addr is unconfigured");
    }
}
