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
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Discovery beacon disabled: could not bind UDP socket ({})", e);
                    return;
                }
            };
            if let Err(e) = socket.set_broadcast(true) {
                eprintln!("Discovery beacon disabled: could not enable broadcast ({})", e);
                return;
            }

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
    pub known_plugins: std::sync::Arc<parking_lot::Mutex<std::collections::HashMap<String, nullherz_traits::SidecarManifest>>>,
    pub library_db: Option<std::sync::Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>>,
    pub dna_discovery: std::sync::Arc<parking_lot::Mutex<nullherz_dna::DiscoveryService>>,
}

impl SidecarDiscoveryService {
    pub fn new(plugins_dir: &str) -> Self {
        Self {
            plugins_dir: plugins_dir.to_string(),
            known_plugins: std::sync::Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            library_db: None,
            dna_discovery: std::sync::Arc::new(parking_lot::Mutex::new(nullherz_dna::DiscoveryService::new())),
        }
    }

    pub fn with_library(mut self, db: std::sync::Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>) -> Self {
        self.library_db = Some(db);
        self
    }

    fn manifest_exists_in_dir(dir: &str, name: &str, manifest: &nullherz_traits::SidecarManifest) -> bool {
        let manifest_path = std::path::Path::new(dir).join(format!("{}.json", name));
        let manifest_alt_path = std::path::Path::new(dir).join(format!("{}.json", manifest.binary_name));
        if manifest_path.exists() || manifest_alt_path.exists() {
            return true;
        }

        let expected_name = format!("{}.json", name).to_lowercase();
        std::fs::read_dir(dir).ok().map_or(false, |entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry.path().extension().and_then(|s| s.to_str()).map(|ext| ext.eq_ignore_ascii_case("json")).unwrap_or(false)
                    && entry.path().file_name().and_then(|n| n.to_str()).map(|fname| fname.to_lowercase() == expected_name).unwrap_or(false)
            })
        })
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
                        let mut discovery = discovery_mutex.lock();
                        discovery.discover();
                        discovery.listen();
                        discovery.known_peers.clone()
                    };
                    tokio::time::sleep(Duration::from_secs(5)).await;

                    if peers.is_empty() { continue; }
                    if peers.is_empty() { continue; }

                    let lib_lock = lib_db.lock();

                    let (trusted_peers, signing_key) = {
                        let d = discovery_mutex.lock();
                        (d.trusted_peers.clone(), d.signing_key)
                    };

                    let sync = nullherz_dna::CloudPeerSync {
                        peers,
                        trusted_peers,
                        peer_keys: parking_lot::Mutex::new(std::collections::HashMap::new()),
                        signing_key,
                        mesh_links: parking_lot::Mutex::new(std::collections::HashSet::new()),
                    };
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
                                ui_controls: Vec::new(),
                            });
                        }
                    }

                    let mut known_lock = known.lock();
                    for (name, manifest) in current_manifests {
                        if !known_lock.contains_key(&name) {
                            println!("Discovery: Found new sidecar plugin: {} (v{})", name, manifest.version);
                            known_lock.insert(name, manifest);
                        }
                    }
                    // Clean up disappeared plugins
                    known_lock.retain(|name, manifest| {
                        let binary_path = std::path::Path::new(&dir).join(&manifest.binary_name);
                        let is_wasm = manifest.version.contains("wasm");
                        let manifest_exists = is_wasm || Self::manifest_exists_in_dir(&dir, name, manifest);

                        let exists = binary_path.exists() && manifest_exists;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_manifest_exists_in_dir_case_insensitive_json_filename() {
        let tmp_dir = std::env::temp_dir().join(format!("nullherz-plugin-test-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&tmp_dir).expect("failed to create temp dir");

        let manifest = nullherz_traits::SidecarManifest {
            name: "Bitcrusher".to_string(),
            version: "0.1.0".to_string(),
            author: "Nullherz Reference".to_string(),
            processor_type_id: 210,
            binary_name: "bitcrusher".to_string(),
            ui_controls: Vec::new(),
        };

        let manifest_path = tmp_dir.join("bitcrusher.json");
        let mut file = File::create(&manifest_path).expect("failed to create manifest file");
        file.write_all(serde_json::to_string(&manifest).unwrap().as_bytes()).expect("failed to write manifest");
        file.flush().expect("failed to flush manifest file");

        let binary_path = tmp_dir.join("bitcrusher");
        File::create(&binary_path).expect("failed to create binary file");

        assert!(SidecarDiscoveryService::manifest_exists_in_dir(tmp_dir.to_str().unwrap(), &manifest.name, &manifest));

        fs::remove_dir_all(&tmp_dir).expect("failed to clean up temp dir");
    }
}
