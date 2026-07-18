use std::sync::Arc;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use parking_lot::Mutex;
use crate::*;


pub trait PeerSync {
    fn announce_dna(&self, dna: &nullherz_traits::SoundDNA);
    fn request_dna(&self, id: u64) -> Option<nullherz_traits::SoundDNA>;
    fn list_peer_dna(&self) -> Vec<(u64, String)>;
    /// Gossip-based metadata exchange: share known DNA identifiers with peers.
    fn gossip_metadata(&self, known_ids: &[(u64, String)]);
    /// Returns the public key of this node for signature verification.
    fn get_public_key(&self) -> [u8; 32];
}

/// Functional implementation of PeerSync using simple TCP exchange with Gossipsub control signals.
pub struct CloudPeerSync {
    pub peers: Vec<String>,
    pub trusted_peers: std::collections::HashSet<String>,
    /// Verified peer identity keys (peer_addr -> ed25519 public key), pinned
    /// trust-on-first-use via the HANDSHAKE/IDENTITY exchange. A peer whose key
    /// changes after pinning is rejected.
    pub peer_keys: Mutex<HashMap<String, [u8; 32]>>,
    /// Node's own signing key
    pub signing_key: Option<[u8; 32]>,
    /// Active Gossipsub mesh links
    pub mesh_links: Mutex<std::collections::HashSet<String>>,
}

impl CloudPeerSync {
    /// HANDSHAKE with a peer and pin its identity key (trust-on-first-use).
    /// Returns the peer's pinned public key, or None if the peer is unreachable,
    /// presents no identity, or presents a key that conflicts with the pin.
    pub fn handshake(&self, peer: &str) -> Option<[u8; 32]> {
        let addr: std::net::SocketAddr = peer.parse().ok()?;
        let mut stream = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(500)).ok()?;
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
        let our_pk = hex::encode(self.get_public_key());
        stream.write_all(format!("HANDSHAKE {}\n", our_pk).as_bytes()).ok()?;

        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.len() < 2 || parts[0] != "IDENTITY" || parts[1] == "none" {
            return None;
        }
        let key_bytes: [u8; 32] = hex::decode(parts[1]).ok()?.try_into().ok()?;
        // Reject keys that are not valid ed25519 points.
        ed25519_dalek::VerifyingKey::from_bytes(&key_bytes).ok()?;

        let mut keys = self.peer_keys.lock();
        match keys.get(peer) {
            Some(pinned) if *pinned != key_bytes => {
                eprintln!("Cloud: peer {} presented a DIFFERENT identity key than pinned — rejecting.", peer);
                None
            }
            _ => {
                keys.insert(peer.to_string(), key_bytes);
                Some(key_bytes)
            }
        }
    }
}

impl PeerSync for CloudPeerSync {
    fn announce_dna(&self, _dna: &nullherz_traits::SoundDNA) {
        // In Gossipsub, announcing is broadcasting to our grafted mesh links
        let mesh = self.mesh_links.lock();
        for peer in mesh.iter() {
            if let Ok(addr) = peer.parse()
                && let Ok(mut stream) = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(100)) {
                    let _ = stream.write_all(b"GOSSIP_PUB\n");
                }
        }
    }

    fn get_public_key(&self) -> [u8; 32] {
        if let Some(sk_bytes) = self.signing_key {
            let sk = ed25519_dalek::SigningKey::from_bytes(&sk_bytes);
            sk.verifying_key().to_bytes()
        } else {
            [0u8; 32]
        }
    }

    fn gossip_metadata(&self, known_ids: &[(u64, String)]) {
        let sk_bytes = match self.signing_key {
            Some(key) => key,
            None => {
                let mut csprng = rand::rngs::OsRng;
                let sk = ed25519_dalek::SigningKey::generate(&mut csprng);
                sk.to_bytes()
            }
        };
        for peer in &self.peers {
            let addr = match peer.parse() {
                Ok(a) => a,
                Err(_) => continue,
            };
            if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
                &addr,
                std::time::Duration::from_millis(100)
            )
                && let Ok(payload) = serde_json::to_string(known_ids) {
                    use ed25519_dalek::Signer;
                    let sk = ed25519_dalek::SigningKey::from_bytes(&sk_bytes);
                    let sig = sk.sign(payload.as_bytes());
                    // Format: GOSSIP_SIGNED <sig_hex> <pk_hex> <payload_json>
                    let msg = format!("GOSSIP_SIGNED {} {} {}\n", hex::encode(sig.to_bytes()), hex::encode(sk.verifying_key().to_bytes()), payload);
                    let _ = stream.write_all(msg.as_bytes());
                }
        }
    }

    fn request_dna(&self, id: u64) -> Option<nullherz_traits::SoundDNA> {
        for peer in &self.peers {
            if !self.trusted_peers.contains(peer) {
                continue;
            }
            let addr: std::net::SocketAddr = match peer.parse() {
                Ok(a) => a,
                Err(_) => continue,
            };
            if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
                &addr,
                std::time::Duration::from_millis(500)
            ) {
                let req = format!("GET DNA {}\n", id);
                let _ = stream.write_all(req.as_bytes());
                let mut buffer = Vec::new();
                let _ = stream.read_to_end(&mut buffer);

                if let Ok(signed_dna) = serde_json::from_slice::<SignedSoundDna>(&buffer) {
                    // Lineage and Signature verification before ingestion
                    if !GeneticLineageConsensus::verify_lineage(&signed_dna) {
                        println!("Cloud: DNA lineage consensus validation FAILED from peer {}", peer);
                        continue;
                    }
                    // Identity check: the payload must be signed by the key
                    // pinned for this peer (pinned on first contact).
                    let pinned = { self.peer_keys.lock().get(peer).copied() }
                        .or_else(|| self.handshake(peer));
                    match pinned {
                        Some(key) if key == signed_dna.signer_public_key => return Some(signed_dna.dna),
                        Some(_) => {
                            println!("Cloud: DNA from peer {} signed by a key that does not match its pinned identity — rejected.", peer);
                        }
                        None => {
                            println!("Cloud: peer {} has no verifiable identity — rejected.", peer);
                        }
                    }
                }
            }
        }
        None
    }

    fn list_peer_dna(&self) -> Vec<(u64, String)> {
        let mut all_dna = Vec::new();
        for peer in &self.peers {
            let addr = match peer.parse() {
                Ok(a) => a,
                Err(_) => continue,
            };

            if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
                &addr,
                std::time::Duration::from_millis(500)
            ) {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
                let _ = stream.write_all(b"LIST\n");
                let mut buffer = Vec::new();
                let _ = stream.read_to_end(&mut buffer);
                if let Ok(list) = serde_json::from_slice::<Vec<(u64, String)>>(&buffer) {
                    all_dna.extend(list);
                }
            }
        }
        all_dna
    }
}

pub struct DiscoveryService {
    pub known_peers: Vec<String>,
    pub trusted_peers: std::collections::HashSet<String>,
    mdns: Option<mdns_sd::ServiceDaemon>,
    service_type: &'static str,
    pub signing_key: Option<[u8; 32]>,
}

impl Default for DiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoveryService {
    pub fn new() -> Self {
        let mut trusted_peers = std::collections::HashSet::new();
        // Default trusted peers for local development and testing
        trusted_peers.insert("127.0.0.1:9003".to_string());
        trusted_peers.insert("localhost:9003".to_string());

        let mut csprng = rand::rngs::OsRng;
        let sk = ed25519_dalek::SigningKey::generate(&mut csprng);

        Self {
            known_peers: Vec::new(),
            trusted_peers,
            mdns: mdns_sd::ServiceDaemon::new().ok(),
            service_type: "_nullherz-dna._udp.local.",
            signing_key: Some(sk.to_bytes()),
        }
    }

    /// Announces presence to the genetic cloud via mDNS
    pub fn discover(&mut self) {
        if let Some(mdns) = &self.mdns {
            let hostname = gethostname::gethostname().to_string_lossy().to_string();
            let service_info = mdns_sd::ServiceInfo::new(
                self.service_type,
                &hostname,
                &format!("{}.local.", hostname),
                "0.0.0.0",
                9001,
                None,
            ).expect("Failed to create mDNS service info");

            mdns.register(service_info).expect("Failed to register mDNS service");
            println!("P2P Discovery: Announced '{}' to the genetic cloud via mDNS.", hostname);
        }
    }

    /// Listens for new peers in the local genetic cloud
    pub fn listen(&mut self) {
        if let Some(mdns) = &self.mdns
            && let Ok(browser) = mdns.browse(self.service_type) {
                while let Ok(event) = browser.recv_timeout(std::time::Duration::from_millis(10)) {
                    if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                        let addr = info.get_addresses().iter().next()
                            .map(|a| format!("{}:{}", a, info.get_port()))
                            .unwrap_or_else(|| "unknown".to_string());

                        if !self.known_peers.contains(&addr) {
                            println!("P2P Discovery: Found peer '{}' at {}", info.get_fullname(), addr);
                            self.known_peers.push(addr);
                        }
                    }
                }
            }
    }

    /// Proactively announces a new SoundDNA availability to known peers.
    pub fn announce_push(&self, dna_id: u64) {
        for peer in &self.known_peers {
            let addr = match peer.parse::<std::net::SocketAddr>() {
                Ok(a) => a,
                Err(_) => continue,
            };
            if let Ok(mut stream) = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(100)) {
                // Gossip protocol control signal: IHAVE message indicating new template ID
                let msg = format!("IHAVE_ID {}\n", dna_id);
                let _ = stream.write_all(msg.as_bytes());
            }
        }
    }

    /// Performs a Gossip cycle with a random subset of known peers.
    pub fn gossip_cycle(&self, lib: &LibraryDatabase) {
        let sk_bytes = match self.signing_key {
            Some(key) => key,
            None => {
                let mut csprng = rand::rngs::OsRng;
                let sk = ed25519_dalek::SigningKey::generate(&mut csprng);
                sk.to_bytes()
            }
        };
        if let Ok(tracks) = lib.list_tracks() {
            let metadata: Vec<(u64, String)> = tracks.into_iter().map(|t| (t.id, t.title)).collect();
            // Select a random peer (simplified)
            if let Some(peer) = self.known_peers.first() {
                let addr = match peer.parse() {
                    Ok(a) => a,
                    Err(_) => return,
                };
                if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
                    &addr,
                    std::time::Duration::from_millis(200)
                )
                    && let Ok(payload) = serde_json::to_string(&metadata) {
                        use ed25519_dalek::Signer;
                        let sk = ed25519_dalek::SigningKey::from_bytes(&sk_bytes);
                        let sig = sk.sign(payload.as_bytes());
                        // Format: GOSSIP_SIGNED <sig_hex> <pk_hex> <payload_json>
                        let msg = format!("GOSSIP_SIGNED {} {} {}\n", hex::encode(sig.to_bytes()), hex::encode(sk.verifying_key().to_bytes()), payload);
                        let _ = stream.write_all(msg.as_bytes());
                    }
            }
        }
    }
}

fn handle_gossip(payload: &str, lib_clone: &Arc<Mutex<LibraryDatabase>>, stream: &std::net::TcpStream) {
    if let Ok(remote_metadata) = serde_json::from_str::<Vec<(u64, String)>>(payload) {
        println!("DNA Server: Received GOSSIP payload with {} entries.", remote_metadata.len());

        if let Ok(peer_addr) = stream.peer_addr() {
            let peer_addr_str = peer_addr.to_string();

            // 1. Identify missing IDs without holding the lock for networking
            let mut missing_ids = Vec::new();
            {
                let lib = lib_clone.lock();
                for (id, name) in remote_metadata {
                    if lib.get_track(id).map(|t| t.is_none()).unwrap_or(false) {
                        missing_ids.push((id, name));
                    }
                }
            }

            // 2. Perform networking outside the lock
            if !missing_ids.is_empty() {
                let lib_c = lib_clone.clone();
                let addr_clone = peer_addr_str.clone();
                std::thread::spawn(move || {
                    let sync_client = CloudPeerSync {
                        peers: vec![addr_clone.clone()],
                        trusted_peers: std::collections::HashSet::from([addr_clone.clone()]),
                        peer_keys: Mutex::new(HashMap::new()),
                        signing_key: None,
                        mesh_links: Mutex::new(std::collections::HashSet::new()),
                    };

                    for (id, name) in missing_ids {
                        println!("Gossip: Discovered unknown DNA '{}' ({}) at {}. Initiating pull...", id, name, addr_clone);
                        if let Some(dna) = sync_client.request_dna(id) {
                            let track = LibraryTrack {
                                id,
                                path: format!("cloud://{}", id),
                                title: name,
                                artist: "Cloud Peer".to_string(),
                                album: "Gossip Discovery".to_string(),
                                genre: "Unknown".to_string(),
                                energy_level: 0.5,
                                metadata: Arc::new(nullherz_traits::SampleMetadata {
                                    dna,
                                    ..nullherz_traits::SampleMetadata::new_empty()
                                }),
                            };
                            {
                                let lib = lib_c.lock();
                                let _ = lib.save_track(&track);
                                println!("Gossip: Successfully synchronized DNA '{}' from cloud.", id);
                            }
                        }
                    }
                });
            }
        }
    }
}

pub struct DnaServer;

impl DnaServer {
    pub fn start(lib: Arc<Mutex<LibraryDatabase>>, port: u16, signing_key: Option<[u8; 32]>) -> std::io::Result<()> {
        let listener = std::net::TcpListener::bind(format!("0.0.0.0:{}", port))?;
        println!("DNA Server listening on port {}", port);

        std::thread::spawn(move || {
            let mesh_peers: Arc<Mutex<std::collections::HashSet<String>>> = Arc::new(Mutex::new(std::collections::HashSet::new()));

            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    let lib_clone = lib.clone();
                    let mesh_peers_clone = mesh_peers.clone();
                    std::thread::spawn(move || {
                        let mut reader = BufReader::new(&stream);
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_ok() {
                            let line_trimmed = line.trim();
                            if line_trimmed == "LIST" {
                                let lib = lib_clone.lock();
                                if let Ok(tracks) = lib.list_tracks() {
                                    let list: Vec<(u64, String)> = tracks.into_iter().map(|t| (t.id, t.title)).collect();
                                    let _ = serde_json::to_writer(&mut stream, &list);
                                }
                                return;
                            }

                            let parts: Vec<&str> = line_trimmed.split_whitespace().collect();
                            if parts.is_empty() { return; }

                            match parts[0] {
                                // GOSSIPSUB CONTROL MESSAGES OVER TCP
                                "GRAFT" => {
                                    if let Ok(peer_addr) = stream.peer_addr() {
                                        mesh_peers_clone.lock().insert(peer_addr.to_string());
                                        let _ = stream.write_all(b"GRAFT_ACK\n");
                                    }
                                }
                                "PRUNE" => {
                                    if let Ok(peer_addr) = stream.peer_addr() {
                                        mesh_peers_clone.lock().remove(&peer_addr.to_string());
                                        let _ = stream.write_all(b"PRUNE_ACK\n");
                                    }
                                }
                                "IHAVE_ID"
                                    if parts.len() >= 2 => {
                                        let _ = stream.write_all(format!("IWANT_ID {}\n", parts[1]).as_bytes());
                                    }
                                "GOSSIP" => {
                                    eprintln!("DNA Server: GOSSIP payload rejected because it was unsigned.");
                                }
                                "GOSSIP_SIGNED"
                                    if parts.len() >= 4 => {
                                        let sig_hex = parts[1];
                                        let pk_hex = parts[2];
                                        // Find start of payload. It's after "GOSSIP_SIGNED <sig> <pk> "
                                        let prefix_len = "GOSSIP_SIGNED ".len() + sig_hex.len() + 1 + pk_hex.len() + 1;
                                        if line_trimmed.len() > prefix_len {
                                            let payload = &line_trimmed[prefix_len..];

                                            use ed25519_dalek::{Verifier, Signature, VerifyingKey};
                                            let sig_res = hex::decode(sig_hex).map_err(|e| e.to_string()).and_then(|b| Signature::from_slice(&b).map_err(|e| e.to_string()));
                                            let pk_res = hex::decode(pk_hex).map_err(|e| e.to_string()).and_then(|b| {
                                                let arr: [u8; 32] = b.try_into().map_err(|_| "Invalid public key length")?;
                                                VerifyingKey::from_bytes(&arr).map_err(|e| e.to_string())
                                            });

                                            match (sig_res, pk_res) {
                                                (Ok(sig), Ok(pk)) => {
                                                    if pk.verify(payload.as_bytes(), &sig).is_ok() {
                                                        handle_gossip(payload, &lib_clone, &stream);
                                                    } else {
                                                        eprintln!("DNA Server: GOSSIP signature verification FAILED.");
                                                    }
                                                }
                                                _ => {
                                                    eprintln!("DNA Server: GOSSIP contains invalid cryptographic material.");
                                                }
                                            }
                                        }
                                    }
                                "HANDSHAKE"
                                    if parts.len() >= 2 => {
                                        let pk_hex = parts[1];
                                        println!("DNA Server: Peer handshake with public key {}", pk_hex);
                                        // Respond with the PUBLIC verifying key only — the
                                        // signing key must never leave this node.
                                        let resp = signing_key
                                            .map(|k| hex::encode(ed25519_dalek::SigningKey::from_bytes(&k).verifying_key().to_bytes()))
                                            .unwrap_or_else(|| "none".to_string());
                                        let _ = stream.write_all(format!("IDENTITY {}\n", resp).as_bytes());
                                    }
                                "GET"
                                    if parts.len() >= 3 && parts[1] == "DNA" => {
                                        let id_str = parts[2];
                                        if let Ok(id) = id_str.parse::<u64>() {
                                            let lib = lib_clone.lock();
                                            if let Ok(Some(track)) = lib.get_track(id) {
                                                    // Production Beta: Sign DNA payload and calculate CAS-ID
                                                    use ed25519_dalek::{Signer, SigningKey};
                                                    use sha2::{Sha256, Digest};

                                                    let mut signature = [0u8; 64];
                                                    let mut pub_key = [0u8; 32];
                                                    let dna_bytes = serde_json::to_vec(&track.metadata.dna).unwrap_or_default();

                                                    let mut hasher = Sha256::new();
                                                    hasher.update(&dna_bytes);
                                                    let cas_id: [u8; 32] = hasher.finalize().into();

                                                    if let Some(sk_bytes) = signing_key {
                                                        let sk = SigningKey::from_bytes(&sk_bytes);
                                                        let sig = sk.sign(&dna_bytes);
                                                        signature = sig.to_bytes();
                                                        pub_key = sk.verifying_key().to_bytes();
                                                    }

                                                    let signed = SignedSoundDna {
                                                        dna: track.metadata.dna.clone(),
                                                        signature,
                                                        signer_public_key: pub_key,
                                                        cas_id: Some(cas_id),
                                                        parent_hashes: Vec::new(),
                                                        authorship_chain: vec!["origin".to_string()],
                                                        generation: 1,
                                                    };
                                                    let _ = serde_json::to_writer(&mut stream, &signed);
                                            }
                                        }
                                    }
                                _ => {}
                            }
                        }
                    });
                }
            }
        });
        Ok(())
    }
}
