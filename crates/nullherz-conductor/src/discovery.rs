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
    pub library_db: Option<std::sync::Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>>,
    pub dna_discovery: std::sync::Arc<std::sync::Mutex<nullherz_dna::DiscoveryService>>,
}

impl SidecarDiscoveryService {
    pub fn new(plugins_dir: &str) -> Self {
        Self {
            plugins_dir: plugins_dir.to_string(),
            known_plugins: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            library_db: None,
            dna_discovery: std::sync::Arc::new(std::sync::Mutex::new(nullherz_dna::DiscoveryService::new())),
        }
    }

    pub fn with_library(mut self, db: std::sync::Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>) -> Self {
        self.library_db = Some(db);
        self
    }

    pub fn start_watcher(&self) {
        self.start_p2p_sync();
        self.start_local_watcher();
    }

    fn start_p2p_sync(&self) {
        let lib = self.library_db.clone();
        let discovery_mutex = self.dna_discovery.clone();
        if let Some(lib_db) = lib {
            tokio::spawn(async move {
                loop {
                    let peers = {
                        let mut discovery = discovery_mutex.lock().unwrap();
                        discovery.discover();
                        discovery.listen();
                        discovery.known_peers.clone()
                    };
                    tokio::time::sleep(Duration::from_secs(5)).await;

                    if peers.is_empty() { continue; }
                    if peers.is_empty() { continue; }

                    let lib_lock = lib_db.lock().unwrap();

                    let sync = nullherz_dna::CloudPeerSync { peers };
                    let _ = lib_lock.sync_with_cloud(&sync);
                }
            });
        }
    }

    fn start_local_watcher(&self) {
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
                        let ext = path.extension().and_then(|s| s.to_str());
                        if ext == Some("json") {
                            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                                if let Ok(manifest) = serde_json::from_str::<nullherz_traits::SidecarManifest>(&content) {
                                    current_manifests.insert(manifest.name.clone(), manifest);
                                }
                            }
                        } else if ext == Some("wasm") {
                            // Stage 6 Universal Extensibility: Implicit manifest for WASM blobs
                            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown_wasm").to_string();
                            current_manifests.insert(name.clone(), nullherz_traits::SidecarManifest {
                                name,
                                version: "1.0.0-wasm".to_string(),
                                author: "Universal Extension".to_string(),
                                processor_type_id: 200, // WASM Generic ID
                                binary_name: path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string(),
                            });
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
                        // WASM plugins might not have a .json manifest
                        let is_wasm = manifest.version.contains("wasm");
                        let manifest_path = std::path::Path::new(&dir).join(format!("{}.json", name));

                        let exists = binary_path.exists() && (is_wasm || manifest_path.exists());
                        if !exists {
                            println!("Discovery: Sidecar plugin removed: {} (binary: {:?})", name, binary_path);
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
