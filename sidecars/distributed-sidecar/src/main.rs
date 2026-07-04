use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use nullherz_traits::{TimestampedCommand, Command, CoreCommand};
use std::net::UdpSocket;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

        tokio::spawn(async move {
            let mut buf = [0u8; 4];
            loop {
                // Read length prefix
                if socket.read_exact(&mut buf).await.is_err() { break; }
                let len = u32::from_be_bytes(buf) as usize;
                if len > 65536 { break; }

                let mut payload = vec![0u8; len];
                if socket.read_exact(&mut payload).await.is_err() { break; }

                if let Ok(cmd) = serde_json::from_slice::<TimestampedCommand>(&payload) {
                    println!("Received command: {:?}", cmd);

                    // Simple Ping-Pong Heartbeat / ACK
                    let ack = serde_json::to_vec(&TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::Core(CoreCommand::RequestSnapshots), // Use RequestSnapshots as a keep-alive/ping
                    }).unwrap();

                    let mut resp = Vec::with_capacity(4 + ack.len());
                    resp.extend_from_slice(&(ack.len() as u32).to_be_bytes());
                    resp.extend_from_slice(&ack);
                    let _ = socket.write_all(&resp).await;
                }
            }
            println!("Conductor detached from {}", addr);
        });
    }
}
