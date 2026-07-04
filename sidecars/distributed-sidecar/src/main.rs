use tokio::io::{AsyncReadExt, AsyncWriteExt};
use nullherz_traits::{TimestampedCommand, Command, CoreCommand, SampleMetadata};
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

        let addr_clone = addr.to_string();
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
                            SampleMetadata::new_empty()
                        );
                        println!("Sidecar: Registered mirrored sample ID={}", sample_id);
                    }
                    continue;
                }

                if let Ok(cmd) = TimestampedCommand::from_binary(&payload) {
                    println!("Received command: {:?}", cmd);

                    // --- STAGE 3: AUDIO RETURN PATH ---
                    // If we receive a command that indicates we should start streaming back audio,
                    // we'd spawn a sender task. For now, we simulate by sending a dummy block on RequestSnapshots.
                    if let Command::Core(CoreCommand::RequestSnapshots) = cmd.command {
                        let dummy_block = ipc_layer::AudioBlock { data: [0.5; 256], len: 256 };
                        let block_bytes = bytemuck::bytes_of(&dummy_block);
                        let mut header = Vec::with_capacity(5);
                        header.push(3u8); // Type 3: Audio Return Block
                        header.extend_from_slice(&(block_bytes.len() as u32).to_be_bytes());
                        let _ = socket.write_all(&header).await;
                        let _ = socket.write_all(block_bytes).await;
                    }

                    // Simple Ping-Pong Heartbeat / ACK (Binary)
                    let ack_cmd = TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::Core(CoreCommand::RequestSnapshots),
                    };
                    if let Ok(ack) = ack_cmd.to_binary() {
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
