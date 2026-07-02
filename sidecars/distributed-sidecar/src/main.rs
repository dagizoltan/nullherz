use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use nullherz_traits::TimestampedCommand;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await?;
    println!("Distributed Sidecar Prototype listening on {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = [0; 1024];
            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n == 0 => return,
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("Socket error: {}", e);
                        return;
                    }
                };

                // Prototype: Deserialize a TimestampedCommand and "process" it.
                // In a real scenario, this would bridge to the local IPC layer.
                if let Ok(cmd) = serde_json::from_slice::<TimestampedCommand>(&buf[0..n]) {
                    println!("Received Remote Command: {:?}", cmd);

                    // Respond with ACK
                    let _ = socket.write_all(b"ACK").await;
                }
            }
        });
    }
}
