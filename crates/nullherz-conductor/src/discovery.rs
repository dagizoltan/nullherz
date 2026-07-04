use std::net::UdpSocket;
use std::time::Duration;

pub struct DiscoveryBeacon {
    pub port: u16,
    pub service_name: String,
}

impl DiscoveryBeacon {
    pub fn new(port: u16, name: &str) -> Self {
        Self { port, service_name: name.to_string() }
    }

    pub fn start_broadcast(self) {
        tokio::spawn(async move {
            let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP socket");
            socket.set_broadcast(true).expect("Failed to set UDP broadcast");

            let msg = format!("nullherz_conductor:{}", self.port);
            let addr = "255.255.255.255:9001";

            loop {
                let _ = socket.send_to(msg.as_bytes(), addr);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }
}

pub struct SidecarDiscoveryService {
    pub plugins_dir: String,
    pub known_plugins: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl SidecarDiscoveryService {
    pub fn new(plugins_dir: &str) -> Self {
        Self {
            plugins_dir: plugins_dir.to_string(),
            known_plugins: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        }
    }

    pub fn start_watcher(&self) {
        let dir = self.plugins_dir.clone();
        let known = self.known_plugins.clone();

        tokio::spawn(async move {
            let path = std::path::Path::new(&dir);
            if !path.exists() {
                let _ = tokio::fs::create_dir_all(path).await;
            }

            loop {
                if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
                    let mut current_plugins = std::collections::HashSet::new();
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        if let Ok(file_type) = entry.file_type().await {
                            if file_type.is_file() {
                                if let Some(name) = entry.file_name().to_str() {
                                    current_plugins.insert(name.to_string());
                                }
                            }
                        }
                    }

                    let mut known_lock = known.lock().unwrap();
                    for plugin in &current_plugins {
                        if !known_lock.contains(plugin) {
                            println!("Discovery: Found new sidecar plugin: {}", plugin);
                            known_lock.insert(plugin.clone());
                        }
                    }
                    // Optional: remove plugins that disappeared
                    known_lock.retain(|p| current_plugins.contains(p));
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }
}
