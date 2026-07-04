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
    pub known_plugins: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, nullherz_traits::SidecarManifest>>>,
}

impl SidecarDiscoveryService {
    pub fn new(plugins_dir: &str) -> Self {
        Self {
            plugins_dir: plugins_dir.to_string(),
            known_plugins: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
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
                    let mut current_manifests = std::collections::HashMap::new();
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("json") {
                            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                                if let Ok(manifest) = serde_json::from_str::<nullherz_traits::SidecarManifest>(&content) {
                                    current_manifests.insert(manifest.name.clone(), manifest);
                                }
                            }
                        }
                    }

                    let mut known_lock = known.lock().unwrap();
                    for (name, manifest) in current_manifests {
                        if !known_lock.contains_key(&name) {
                            println!("Discovery: Found new sidecar plugin: {} (v{})", name, manifest.version);
                            known_lock.insert(name, manifest);
                        }
                    }
                    // Clean up disappeared plugins
                    known_lock.retain(|name, manifest| {
                        let binary_path = std::path::Path::new(&dir).join(&manifest.binary_name);
                        let manifest_path = std::path::Path::new(&dir).join(format!("{}.json", name));
                        if !binary_path.exists() || !manifest_path.exists() {
                            println!("Discovery: Sidecar plugin removed: {}", name);
                            false
                        } else {
                            true
                        }
                    });
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    }
}
