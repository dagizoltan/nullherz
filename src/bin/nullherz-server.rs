use std::net::SocketAddr;
use std::path::Path;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    println!("🚀 Nullherz Local Store & Gateway Server starting on http://{}", addr);

    let store_catalog_dir = Path::new("assets/store_catalog");
    if !store_catalog_dir.exists() {
        let _ = std::fs::create_dir_all(store_catalog_dir);
    }

    let listener = TcpListener::bind(addr).await?;
    println!("Serving catalog bundles from assets/store_catalog/...");

    loop {
        let (mut socket, _peer_addr) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            if let Ok(n) = socket.read(&mut buf).await {
                if n == 0 {
                    return;
                }
                let request = String::from_utf8_lossy(&buf[..n]);
                let mut lines = request.lines();
                if let Some(first_line) = lines.next() {
                    let parts: Vec<&str> = first_line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let path_str = parts[1];
                        if path_str == "/health" || path_str == "/" {
                            let body = r#"{"status":"ok","server":"nullherz-store-local"}"#;
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else if path_str == "/catalog" || path_str == "/catalog.json" {
                            let cat_path = Path::new("assets/store_catalog/catalog.json");
                            let body = if cat_path.exists() {
                                std::fs::read_to_string(cat_path).unwrap_or_else(|_| "[]".to_string())
                            } else {
                                "[]".to_string()
                            };
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else {
                            let response = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                            let _ = socket.write_all(response.as_bytes()).await;
                        }
                    }
                }
            }
        });
    }
}
