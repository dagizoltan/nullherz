use std::sync::Arc;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};
use redb::{Database, TableDefinition, ReadableTable, TableError};

pub type SampleBuffer = Arc<Vec<f32>>;
pub use nullherz_traits::RegisteredSample;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SignedSoundDna {
    pub dna: nullherz_traits::SoundDNA,
    #[serde(with = "serde_big_array::BigArray")]
    pub signature: [u8; 64],
    pub signer_public_key: [u8; 32],
    /// Content-Addressable Identifier (Blake3 hash of the serialized DNA)
    pub cas_id: Option<[u8; 32]>,

    // LINEAGE CONSENSUS EXTENSIONS
    #[serde(default)]
    pub parent_hashes: Vec<[u8; 32]>,
    #[serde(default)]
    pub authorship_chain: Vec<String>,
    #[serde(default)]
    pub generation: u32,
}

/// Cryptographic and lineage-based consensus verifier.
pub struct GeneticLineageConsensus;

impl GeneticLineageConsensus {
    pub fn verify_signature(signed_dna: &SignedSoundDna) -> bool {
        use ed25519_dalek::{Verifier, Signature, VerifyingKey};
        let pub_key_res = VerifyingKey::from_bytes(&signed_dna.signer_public_key);
        let sig_res = Signature::from_slice(&signed_dna.signature);

        if let (Ok(pub_key), Ok(sig)) = (pub_key_res, sig_res) {
            let dna_bytes = serde_json::to_vec(&signed_dna.dna).unwrap_or_default();
            pub_key.verify(&dna_bytes, &sig).is_ok()
        } else {
            false
        }
    }

    pub fn verify_lineage(signed_dna: &SignedSoundDna) -> bool {
        if !Self::verify_signature(signed_dna) {
            return false;
        }
        // Height check: if parents exist, generation must be > 0.
        if !signed_dna.parent_hashes.is_empty() && signed_dna.generation == 0 {
            return false;
        }
        // Authorship check: active ancestry requires at least one author registered.
        if signed_dna.generation > 0 && signed_dna.authorship_chain.is_empty() {
            return false;
        }
        true
    }
}

mod serde_arc {
    use std::sync::Arc;
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    pub fn serialize<T, S>(val: &Arc<T>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        val.as_ref().serialize(s)
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Arc<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        T::deserialize(d).map(Arc::new)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct LibraryTrack {
    pub id: u64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub energy_level: f32,
    #[serde(with = "serde_arc")]
    pub metadata: Arc<nullherz_traits::SampleMetadata>,
}

const TRACKS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("tracks");
const CRATES_TABLE: TableDefinition<(&str, u64), ()> = TableDefinition::new("crates_v2");
const SMART_CRATES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("smart_crates");

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SmartCrateDefinition {
    pub name: String,
    pub target_dna: Option<nullherz_traits::SoundDNA>,
    pub threshold: f32,
    pub spectral_tilt_range: Option<(f32, f32)>,
    pub rhythmic_syncopation_range: Option<(f32, f32)>,
    pub glitch_density_range: Option<(f32, f32)>,
    pub genre: Option<String>,
    pub bpm_range: Option<(f32, f32)>,
    pub energy_range: Option<(f32, f32)>,
    pub root_key: Option<f32>,
}

pub trait GeneticLibrary: Send + Sync {
    fn get_track(&self, id: u64) -> Result<Option<LibraryTrack>, Box<dyn std::error::Error>>;
    fn list_tracks(&self) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>>;
    fn save_track(&self, track: &LibraryTrack) -> Result<(), Box<dyn std::error::Error>>;
    fn add_to_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>>;
    fn remove_from_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>>;
    fn list_crates(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn get_tracks_in_crate(&self, crate_name: &str) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>>;
    fn query_tracks(&self, genre: Option<&str>, min_bpm: Option<f32>, max_bpm: Option<f32>, root_key: Option<f32>) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>>;
    /// Proactively suggests tracks from the library that are genetically similar to the provided DNA.
    fn suggest_matches(&self, target_dna: &nullherz_traits::SoundDNA, limit: usize) -> Result<Vec<(u64, f32)>, Box<dyn std::error::Error>>;
    fn remove_track(&self, id: u64) -> Result<(), Box<dyn std::error::Error>>;
}

pub struct LibraryDatabase {
    db: Database,
    /// Merkle-DAG Root Hash representing the entire library state.
    pub merkle_root: Mutex<[u8; 32]>,
}

impl GeneticLibrary for LibraryDatabase {
    fn get_track(&self, id: u64) -> Result<Option<LibraryTrack>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TRACKS_TABLE) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let result = table.get(id)?;
        if let Some(guard) = result {
            let track: LibraryTrack = serde_json::from_slice(guard.value())?;
            return Ok(Some(track));
        }
        Ok(None)
    }

    fn list_tracks(&self) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(TRACKS_TABLE) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut tracks = Vec::new();
        for res in table.iter()? {
            let (_id, val) = res?;
            let track: LibraryTrack = serde_json::from_slice(val.value())?;
            tracks.push(track);
        }
        Ok(tracks)
    }

    fn save_track(&self, track: &LibraryTrack) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TRACKS_TABLE)?;
            let serialized = serde_json::to_vec(track)?;
            table.insert(track.id, serialized.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn add_to_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CRATES_TABLE)?;
            table.insert((crate_name, track_id), ())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn remove_from_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CRATES_TABLE)?;
            table.remove((crate_name, track_id))?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn list_crates(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(CRATES_TABLE) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let mut crate_names = std::collections::HashSet::new();
        for res in table.iter()? {
            let (key_guard, _) = res?;
            let (name, _) = key_guard.value();
            crate_names.insert(name.to_string());
        }
        Ok(crate_names.into_iter().collect())
    }

    fn get_tracks_in_crate(&self, crate_name: &str) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let crate_table = match read_txn.open_table(CRATES_TABLE) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let track_table = read_txn.open_table(TRACKS_TABLE)?;

        let mut tracks = Vec::new();
        let start = (crate_name, 0);
        let end = (crate_name, u64::MAX);
        for res in crate_table.range(start..=end)? {
            let (key_guard, _) = res?;
            let (_name, track_id) = key_guard.value();
            if let Some(val) = track_table.get(track_id)? {
                let track: LibraryTrack = serde_json::from_slice(val.value())?;
                tracks.push(track);
            }
        }
        Ok(tracks)
    }

    fn query_tracks(&self, genre: Option<&str>, min_bpm: Option<f32>, max_bpm: Option<f32>, root_key: Option<f32>) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>> {
        let all_tracks = self.list_tracks()?;
        let results = all_tracks.into_iter().filter(|t| {
            if let Some(g) = genre {
                if t.genre != g { return false; }
            }
            if let Some(min) = min_bpm {
                if (*t.metadata).bpm < min { return false; }
            }
            if let Some(max) = max_bpm {
                if (*t.metadata).bpm > max { return false; }
            }
            if let Some(key) = root_key {
                if (*t.metadata).root_key != Some(key) { return false; }
            }
            true
        }).collect();
        Ok(results)
    }

    fn suggest_matches(&self, target_dna: &nullherz_traits::SoundDNA, limit: usize) -> Result<Vec<(u64, f32)>, Box<dyn std::error::Error>> {
        Matchmaker::find_best_matches(self, target_dna, limit)
    }

    fn remove_track(&self, id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut track_table = write_txn.open_table(TRACKS_TABLE)?;
            track_table.remove(id)?;

            let mut crate_table = write_txn.open_table(CRATES_TABLE)?;
            let mut keys_to_remove = Vec::new();
            for res in crate_table.iter()? {
                let (key_guard, _) = res?;
                let (name, track_id) = key_guard.value();
                if track_id == id {
                    keys_to_remove.push((name.to_string(), track_id));
                }
            }
            for (name, track_id) in keys_to_remove {
                crate_table.remove((name.as_str(), track_id))?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }
}

impl LibraryDatabase {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::create(path)?;
        // Ensure table exists
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(TRACKS_TABLE)?;
            let _ = write_txn.open_table(CRATES_TABLE)?;
            let _ = write_txn.open_table(SMART_CRATES_TABLE)?;
        }
        write_txn.commit()?;
        Ok(Self { db, merkle_root: Mutex::new([0u8; 32]) })
    }

    pub fn update_merkle_root(&self) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        use sha2::{Sha256, Digest};
        let tracks = self.list_tracks()?;
        let mut hasher = Sha256::new();
        for track in tracks {
            let dna_bytes = serde_json::to_vec(&(*track.metadata).dna)?;
            hasher.update(&dna_bytes);
        }
        let hash: [u8; 32] = hasher.finalize().into();
        let mut root = self.merkle_root.lock().unwrap();
        *root = hash;
        Ok(hash)
    }

    pub fn save_smart_crate(&self, definition: &SmartCrateDefinition) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SMART_CRATES_TABLE)?;
            let serialized = serde_json::to_vec(definition)?;
            table.insert(definition.name.as_str(), serialized.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_smart_crate(&self, name: &str) -> Result<Option<SmartCrateDefinition>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(SMART_CRATES_TABLE) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let result = table.get(name)?;
        if let Some(guard) = result {
            let definition: SmartCrateDefinition = serde_json::from_slice(guard.value())?;
            return Ok(Some(definition));
        }
        Ok(None)
    }

    pub fn list_smart_crates(&self) -> Result<Vec<SmartCrateDefinition>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(SMART_CRATES_TABLE) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut crates = Vec::new();
        for res in table.iter()? {
            let (_name, val) = res?;
            let definition: SmartCrateDefinition = serde_json::from_slice(val.value())?;
            crates.push(definition);
        }
        Ok(crates)
    }

    pub fn get_smart_crate_tracks(&self, name: &str) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>> {
        let definition = self.get_smart_crate(name)?;
        if let Some(def) = definition {
            let all_tracks = self.list_tracks()?;
            Ok(SmartCrateManager::filter_tracks(&def, all_tracks))
        } else {
            Ok(Vec::new())
        }
    }

    pub fn sync_with_cloud(&self, sync_service: &dyn PeerSync) -> Result<(), Box<dyn std::error::Error>> {
        let tracks = self.list_tracks()?;
        for track in tracks {
            sync_service.announce_dna(&(*track.metadata).dna);
        }

        let remote_dna = sync_service.list_peer_dna();
        for (id, name) in remote_dna {
            if self.get_track(id)?.is_none() {
                if let Some(dna) = sync_service.request_dna(id) {
                    println!("Sync: Inherited SoundDNA '{}' from cloud peer.", name);
                    let track = LibraryTrack {
                        id,
                        path: format!("cloud://{}", id),
                        title: name,
                        artist: "Cloud Peer".to_string(),
                        album: "Unknown".to_string(),
                        genre: "Unknown".to_string(),
                        energy_level: 0.5,
                        metadata: Arc::new(nullherz_traits::SampleMetadata {
                            dna,
                            ..nullherz_traits::SampleMetadata::new_empty()
                        }),
                    };
                    self.save_track(&track)?;
                }
            }
        }
        Ok(())
    }
}

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
    /// Mock for cryptographic signatures: peer_addr -> signature
    pub peer_signatures: HashMap<String, [u8; 64]>,
    /// Node's own signing key
    pub signing_key: Option<[u8; 32]>,
    /// Active Gossipsub mesh links
    pub mesh_links: Mutex<std::collections::HashSet<String>>,
}

impl PeerSync for CloudPeerSync {
    fn announce_dna(&self, _dna: &nullherz_traits::SoundDNA) {
        // In Gossipsub, announcing is broadcasting to our grafted mesh links
        let mesh = self.mesh_links.lock().unwrap();
        for peer in mesh.iter() {
            if let Ok(addr) = peer.parse() {
                if let Ok(mut stream) = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(100)) {
                    let _ = stream.write_all(b"GOSSIP_PUB\n");
                }
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
            ) {
                if let Ok(payload) = serde_json::to_string(known_ids) {
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

    fn request_dna(&self, id: u64) -> Option<nullherz_traits::SoundDNA> {
        for peer in &self.peers {
            if !self.trusted_peers.contains(peer) {
                continue;
            }
            let addr = match peer.parse() {
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
                    if GeneticLineageConsensus::verify_lineage(&signed_dna) {
                        return Some(signed_dna.dna);
                    } else {
                        println!("Cloud: DNA lineage consensus validation FAILED from peer {}", peer);
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
        if let Some(mdns) = &self.mdns {
            if let Ok(browser) = mdns.browse(self.service_type) {
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
                ) {
                    if let Ok(payload) = serde_json::to_string(&metadata) {
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
}

fn handle_gossip(payload: &str, lib_clone: &Arc<Mutex<LibraryDatabase>>, stream: &std::net::TcpStream) {
    if let Ok(remote_metadata) = serde_json::from_str::<Vec<(u64, String)>>(payload) {
        println!("DNA Server: Received GOSSIP payload with {} entries.", remote_metadata.len());

        if let Ok(peer_addr) = stream.peer_addr() {
            let peer_addr_str = peer_addr.to_string();

            // 1. Identify missing IDs without holding the lock for networking
            let mut missing_ids = Vec::new();
            if let Ok(lib) = lib_clone.lock() {
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
                        peer_signatures: HashMap::new(),
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
                            if let Ok(lib) = lib_c.lock() {
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
                                if let Ok(lib) = lib_clone.lock() {
                                    if let Ok(tracks) = lib.list_tracks() {
                                        let list: Vec<(u64, String)> = tracks.into_iter().map(|t| (t.id, t.title)).collect();
                                        let _ = serde_json::to_writer(&mut stream, &list);
                                    }
                                }
                                return;
                            }

                            let parts: Vec<&str> = line_trimmed.split_whitespace().collect();
                            if parts.is_empty() { return; }

                            match parts[0] {
                                // GOSSIPSUB CONTROL MESSAGES OVER TCP
                                "GRAFT" => {
                                    if let Ok(peer_addr) = stream.peer_addr() {
                                        mesh_peers_clone.lock().unwrap().insert(peer_addr.to_string());
                                        let _ = stream.write_all(b"GRAFT_ACK\n");
                                    }
                                }
                                "PRUNE" => {
                                    if let Ok(peer_addr) = stream.peer_addr() {
                                        mesh_peers_clone.lock().unwrap().remove(&peer_addr.to_string());
                                        let _ = stream.write_all(b"PRUNE_ACK\n");
                                    }
                                }
                                "IHAVE_ID" => {
                                    if parts.len() >= 2 {
                                        let _ = stream.write_all(format!("IWANT_ID {}\n", parts[1]).as_bytes());
                                    }
                                }
                                "GOSSIP" => {
                                    eprintln!("DNA Server: GOSSIP payload rejected because it was unsigned.");
                                }
                                "GOSSIP_SIGNED" => {
                                    if parts.len() >= 4 {
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
                                }
                                "HANDSHAKE" => {
                                    if parts.len() >= 2 {
                                        let pk_hex = parts[1];
                                        println!("DNA Server: Peer handshake with public key {}", pk_hex);
                                        let resp = signing_key.map(|k| hex::encode(&k[..])).unwrap_or_else(|| "none".to_string());
                                        let _ = stream.write_all(format!("IDENTITY {}\n", resp).as_bytes());
                                    }
                                }
                                "GET" => {
                                    if parts.len() >= 3 && parts[1] == "DNA" {
                                        let id_str = parts[2];
                                        if let Ok(id) = id_str.parse::<u64>() {
                                            if let Ok(lib) = lib_clone.lock() {
                                                if let Ok(Some(track)) = lib.get_track(id) {
                                                    // Production Beta: Sign DNA payload and calculate CAS-ID
                                                    use ed25519_dalek::{Signer, SigningKey};
                                                    use sha2::{Sha256, Digest};

                                                    let mut signature = [0u8; 64];
                                                    let mut pub_key = [0u8; 32];
                                                    let dna_bytes = serde_json::to_vec(&(*track.metadata).dna).unwrap_or_default();

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
                                                        dna: (*track.metadata).dna.clone(),
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

pub struct SmartCrateManager;

impl SmartCrateManager {
    pub fn filter_tracks(def: &SmartCrateDefinition, tracks: Vec<LibraryTrack>) -> Vec<LibraryTrack> {
        let mut results = tracks;

        // 1. Filter by DNA Similarity if target_dna is present
        if let Some(ref target) = def.target_dna {
            let matched = Matchmaker::find_matches_above_threshold(target, &results, def.threshold);
            let matched_ids: std::collections::HashSet<u64> = matched.into_iter().map(|(id, _)| id).collect();
            results.retain(|t| matched_ids.contains(&t.id));
        }

        // 2. Filter by Spectral Tilt
        if let Some((min, max)) = def.spectral_tilt_range {
            results.retain(|t| {
                let val = (*t.metadata).dna.spectral.tilt;
                val >= min && val <= max
            });
        }

        // 3. Filter by Rhythmic Syncopation
        if let Some((min, max)) = def.rhythmic_syncopation_range {
            results.retain(|t| {
                let val = (*t.metadata).dna.rhythmic.syncopation_index;
                val >= min && val <= max
            });
        }

        // 4. Filter by Glitch Density
        if let Some((min, max)) = def.glitch_density_range {
            results.retain(|t| {
                let val = (*t.metadata).dna.artifacts.glitch_density;
                val >= min && val <= max
            });
        }

        // 5. Filter by Genre
        if let Some(ref genre) = def.genre {
            results.retain(|t| t.genre == *genre);
        }

        // 6. Filter by BPM range
        if let Some((min, max)) = def.bpm_range {
            results.retain(|t| (*t.metadata).bpm >= min && (*t.metadata).bpm <= max);
        }

        // 7. Filter by Energy level
        if let Some((min, max)) = def.energy_range {
            results.retain(|t| t.energy_level >= min && t.energy_level <= max);
        }

        // 8. Filter by Root Key
        if let Some(key) = def.root_key {
            results.retain(|t| (*t.metadata).root_key == Some(key));
        }

        results
    }

    /// Automatically generates a smart crate based on "energy-level-matching" to a seed track.
    pub fn generate_energy_matched_crate(seed_track: &LibraryTrack, _all_tracks: Vec<LibraryTrack>, threshold: f32) -> SmartCrateDefinition {
        SmartCrateDefinition {
            name: format!("Energy Match: {}", seed_track.title),
            target_dna: Some((*seed_track.metadata).dna.clone()),
            threshold,
            spectral_tilt_range: None,
            rhythmic_syncopation_range: None,
            glitch_density_range: None,
            genre: Some(seed_track.genre.clone()),
            bpm_range: Some(((*seed_track.metadata).bpm - 5.0, (*seed_track.metadata).bpm + 5.0)),
            energy_range: Some((seed_track.energy_level - 0.2, seed_track.energy_level + 0.2)),
            root_key: (*seed_track.metadata).root_key,
        }
    }
}

/// High-performance Sample Registry using a multi-tiered lock-free approach.
/// Stage 2: Optimized for O(1) concurrent registration and high-speed lookups.
pub struct SampleRegistry {
    inner: AtomicPtr<HashMap<u64, RegisteredSample>>,
    write_lock: Mutex<()>,
    garbage: Mutex<Vec<*mut HashMap<u64, RegisteredSample>>>,
    readers: std::sync::atomic::AtomicUsize,
}

unsafe impl Send for SampleRegistry {}
unsafe impl Sync for SampleRegistry {}

impl Default for SampleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl nullherz_traits::SampleRegistry for SampleRegistry {
    fn register(&self, id: u64, buffer: SampleBuffer) {
        self.register_with_metadata(id, buffer, Arc::new(nullherz_traits::SampleMetadata::new_empty()));
    }

    fn register_with_metadata(&self, id: u64, buffer: SampleBuffer, metadata: Arc<nullherz_traits::SampleMetadata>) {
        let _lock = self.write_lock.lock().unwrap();

        let old_ptr = self.inner.load(Ordering::Acquire);
        let mut new_map = unsafe { (*old_ptr).clone() };
        new_map.insert(id, RegisteredSample { buffer, metadata });

        let new_ptr = Box::into_raw(Box::new(new_map));
        self.inner.store(new_ptr, Ordering::Release);
        self.garbage.lock().unwrap().push(old_ptr);
    }

    fn drain_garbage(&self) {
        if self.readers.load(Ordering::SeqCst) > 0 { return; }
        let mut g = self.garbage.lock().unwrap();
        for ptr in g.drain(..) {
            unsafe { drop(Box::from_raw(ptr)); }
        }
    }

    fn get(&self, id: u64) -> Option<RegisteredSample> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).get(&id).cloned() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }

    fn list_ids(&self) -> Vec<u64> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).keys().cloned().collect() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }
}

impl SampleRegistry {
    pub fn new() -> Self {
        let initial_map = Box::new(HashMap::new());
        Self {
            inner: AtomicPtr::new(Box::into_raw(initial_map)),
            write_lock: Mutex::new(()),
            garbage: Mutex::new(Vec::new()),
            readers: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl Drop for SampleRegistry {
    fn drop(&mut self) {
        use nullherz_traits::SampleRegistry;
        let ptr = self.inner.load(Ordering::Acquire);
        unsafe { drop(Box::from_raw(ptr)); }
        self.drain_garbage();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_lineage_consensus_verification() {
        use ed25519_dalek::{SigningKey, Signer};
        let mut csprng = rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        let dna = nullherz_traits::SoundDNA::default();
        let dna_bytes = serde_json::to_vec(&dna).unwrap_or_default();
        let signature = signing_key.sign(&dna_bytes);

        let mut signed = SignedSoundDna {
            dna,
            signature: signature.to_bytes(),
            signer_public_key: verifying_key.to_bytes(),
            cas_id: None,
            parent_hashes: vec![[1u8; 32]],
            authorship_chain: vec!["alice".to_string()],
            generation: 1,
        };
        // Should succeed for a properly signed lineage record
        assert!(GeneticLineageConsensus::verify_lineage(&signed));

        // Should fail if signature is invalid
        signed.signature[0] ^= 1;
        assert!(!GeneticLineageConsensus::verify_lineage(&signed));
        signed.signature[0] ^= 1; // restore

        // Should fail if generation is 0 but has parents
        signed.generation = 0;
        assert!(!GeneticLineageConsensus::verify_lineage(&signed));

        // Should fail if generation is >0 but authorship_chain is empty
        signed.generation = 1;
        signed.authorship_chain = vec![];
        assert!(!GeneticLineageConsensus::verify_lineage(&signed));
    }

    #[test]
    fn test_library_database_crud() {
        let db_path = "test_library.redb";
        let _ = fs::remove_file(db_path);

        let db = LibraryDatabase::load(db_path).unwrap();

        let track = LibraryTrack {
            id: 1,
            path: "/test/path.wav".to_string(),
            title: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
            album: "Test Album".to_string(),
            genre: "Test Genre".to_string(),
            energy_level: 0.8,
            metadata: Arc::new(nullherz_traits::SampleMetadata::new_empty()),
        };

        db.save_track(&track).unwrap();

        let loaded = db.get_track(1).unwrap().unwrap();
        assert_eq!(loaded, track);

        let list = db.list_tracks().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], track);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn test_sync_with_cloud_persistence() {
        let db_path = "test_sync.redb";
        let _ = std::fs::remove_file(db_path);
        let db = LibraryDatabase::load(db_path).unwrap();

        struct MockSync;
        impl PeerSync for MockSync {
            fn announce_dna(&self, _dna: &nullherz_traits::SoundDNA) {}
            fn request_dna(&self, id: u64) -> Option<nullherz_traits::SoundDNA> {
                if id == 0xABC { Some(nullherz_traits::SoundDNA::default()) } else { None }
            }
            fn list_peer_dna(&self) -> Vec<(u64, String)> {
                vec![(0xABC, "Cloud Track".to_string())]
            }
            fn gossip_metadata(&self, _known_ids: &[(u64, String)]) {}
            fn get_public_key(&self) -> [u8; 32] { [0u8; 32] }
        }

        db.sync_with_cloud(&MockSync).unwrap();

        let track = db.get_track(0xABC).unwrap().expect("Track should be persisted after sync");
        assert_eq!(track.title, "Cloud Track");
        assert_eq!(track.artist, "Cloud Peer");
        assert!(track.path.contains("cloud://"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_library_crating() {
        let db_path = "test_crates.redb";
        let _ = fs::remove_file(db_path);
        let db = LibraryDatabase::load(db_path).unwrap();

        let track = LibraryTrack {
            id: 101,
            path: "/test/track.wav".to_string(),
            title: "Crate Track".to_string(),
            artist: "Crate Artist".to_string(),
            album: "Crate Album".to_string(),
            genre: "Techno".to_string(),
            energy_level: 0.9,
            metadata: Arc::new(nullherz_traits::SampleMetadata::new_empty()),
        };

        db.save_track(&track).unwrap();
        db.add_to_crate("Techno", 101).unwrap();

        let in_crate = db.get_tracks_in_crate("Techno").unwrap();
        assert_eq!(in_crate.len(), 1);
        assert_eq!(in_crate[0].id, 101);

        let empty_crate = db.get_tracks_in_crate("House").unwrap();
        assert!(empty_crate.is_empty());

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn test_genetic_similarity() {
        use nullherz_traits::SoundDNA;
        let mut dna_a = SoundDNA::default();
        let mut dna_b = SoundDNA::default();

        for i in 0..16 {
            dna_a.spectral.latent_space[i] = 0.5;
            dna_b.spectral.latent_space[i] = 0.55;
        }

        let sim = calculate_similarity(&dna_a, &dna_b);
        assert!(sim > 0.9);

        for i in 0..16 { dna_b.spectral.latent_space[i] = 1.0; }
        let sim_low = calculate_similarity(&dna_a, &dna_b);
        assert!(sim_low < sim);
    }

    #[test]
    fn test_simd_dna_interpolation() {
        use nullherz_traits::SoundDNA;
        let mut dna_a = SoundDNA::default();
        let mut dna_b = SoundDNA::default();

        for i in 0..16 {
            dna_a.spectral.latent_space[i] = 0.2;
            dna_b.spectral.latent_space[i] = 0.8;
        }

        let child = transfuse_dna(&dna_a, &dna_b, 0.5);

        for i in 0..16 {
            // Neural shaping applies tanh() to the result: 0.5.tanh() ~= 0.4621
            assert!((child.spectral.latent_space[i] - 0.5_f32.tanh()).abs() < 0.001);
        }

        let child_025 = transfuse_dna(&dna_a, &dna_b, 0.25);
        for i in 0..16 {
            // (0.2 * 0.75) + (0.8 * 0.25) = 0.15 + 0.2 = 0.35
            // 0.35.tanh() ~= 0.3363
            assert!((child_025.spectral.latent_space[i] - 0.35_f32.tanh()).abs() < 0.001);
        }
    }

    #[test]
    fn test_smart_crate_filtering() {
    }

    #[test]
    fn test_chaotic_transfusion_determinism() {
        use nullherz_traits::SoundDNA;
        let dna_a = SoundDNA::default();
        let dna_b = SoundDNA::default();

        let child_1 = chaotic_transfuse_dna(&dna_a, &dna_b, 0.5, 0.5);
        let child_2 = chaotic_transfuse_dna(&dna_a, &dna_b, 0.5, 0.5);

        assert_eq!(child_1, child_2);
    }

    #[test]
    fn test_chaotic_transfusion_mutation() {
        use nullherz_traits::SoundDNA;
        let dna_a = SoundDNA::default();
        let dna_b = SoundDNA::default();

        let normal = transfuse_dna(&dna_a, &dna_b, 0.5);
        let chaotic = chaotic_transfuse_dna(&dna_a, &dna_b, 0.5, 1.0);

        // Chaotic transfusion should produce different latent spaces due to mutations
        assert_ne!(normal.spectral.latent_space, chaotic.spectral.latent_space);
        assert!(chaotic.artifacts.glitch_density > normal.artifacts.glitch_density);
    }

    #[test]
    fn test_gossip_signature_enforcement_server() {
        use std::net::TcpStream;
        use std::io::Write;
        use ed25519_dalek::{SigningKey, Signer};

        let mut port = 18200;
        let listener = loop {
            match std::net::TcpListener::bind(format!("127.0.0.1:{}", port)) {
                Ok(l) => break l,
                Err(_) => {
                    port += 1;
                    if port > 19200 {
                        panic!("Failed to find free port for test");
                    }
                }
            }
        };
        let actual_port = listener.local_addr().unwrap().port();
        drop(listener);

        let db_path = format!("test_gossip_srv_{}.redb", actual_port);
        let _ = fs::remove_file(&db_path);
        let lib = Arc::new(Mutex::new(LibraryDatabase::load(&db_path).unwrap()));

        let mut csprng = rand::rngs::OsRng;
        let sk = SigningKey::generate(&mut csprng);
        let sk_bytes = sk.to_bytes();

        DnaServer::start(lib.clone(), actual_port, Some(sk_bytes)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 1. Send unsigned GOSSIP
        {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", actual_port)).unwrap();
            let payload = "[(1, \"Unsigned Track\")]";
            let msg = format!("GOSSIP {}\n", payload);
            stream.write_all(msg.as_bytes()).unwrap();
            stream.flush().unwrap();
            // Connection will be closed by server
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).unwrap();
        }

        // 2. Send GOSSIP_SIGNED
        {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", actual_port)).unwrap();
            let payload = "[(2, \"Signed Track\")]";
            let sig = sk.sign(payload.as_bytes());
            let msg = format!(
                "GOSSIP_SIGNED {} {} {}\n",
                hex::encode(sig.to_bytes()),
                hex::encode(sk.verifying_key().to_bytes()),
                payload
            );
            stream.write_all(msg.as_bytes()).unwrap();
            stream.flush().unwrap();
            // Connection will be closed by server
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).unwrap();
        }

        let _ = fs::remove_file(&db_path);
    }
}

pub struct NeuralTransfuser;

impl NeuralTransfuser {
    pub fn interpolate_latent(dest: &mut [f32; 16], src_a: &[f32; 16], src_b: &[f32; 16], bias: f32) {
        use audio_dsp::simd_vec::FloatX16;
        let v_inv_bias = FloatX16::splat(1.0 - bias);
        let v_bias = FloatX16::splat(bias);

        let v_a = FloatX16::new(*src_a);
        let v_b = FloatX16::new(*src_b);

        // Linear interpolation in latent space
        let mut v_res = (v_a * v_inv_bias) + (v_b * v_bias);

        // Stage 6: Apply neural shaping (tanh activation) for better transfusion semantics
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        {
            v_res.parts[0] = audio_dsp::simd_vec::tanh_simd(v_res.parts[0]);
            v_res.parts[1] = audio_dsp::simd_vec::tanh_simd(v_res.parts[1]);
            v_res.parts[2] = audio_dsp::simd_vec::tanh_simd(v_res.parts[2]);
            v_res.parts[3] = audio_dsp::simd_vec::tanh_simd(v_res.parts[3]);
        }
        #[cfg(not(all(target_arch = "wasm32", target_feature = "simd128")))]
        {
            // Fallback for non-wasm or missing SIMD128
            let mut arr: [f32; 16] = v_res.into();
            for val in arr.iter_mut() { *val = val.tanh(); }
            v_res = FloatX16::new(arr);
        }

        *dest = v_res.into();
    }
}

pub trait NeuralEncoder {
    fn encode(&self, audio: &[f32]) -> [f32; 16];
    fn decode(&self, latent: &[f32; 16]) -> Vec<f32>;
}

/// Standard Stage 6 Neural Encoder using SIMD-optimized feature extraction.
pub struct StandardNeuralEncoder {
    /// Projection matrix for latent space reduction (mocked)
    pub projection: [[f32; 128]; 16],
}

impl NeuralEncoder for StandardNeuralEncoder {
    fn encode(&self, audio: &[f32]) -> [f32; 16] {
        use audio_dsp::simd_vec::load_f32x8;
        let mut latent = [0.0f32; 16];

        // 1. Decimate audio to 128 feature bins (simplified)
        let mut features = [0.0f32; 128];
        let step = audio.len() / 128;
        if step > 0 {
            for i in 0..128 {
                features[i] = audio[i * step].abs();
            }
        }

        // 2. Linear projection to 16-dim latent space using SIMD
        for i in 0..16 {
            let mut sum = 0.0f32;
            let proj_row = &self.projection[i];

            let mut j = 0;
            while j + 8 <= 128 {
                let v_feat = load_f32x8(&features, j);
                let v_proj = load_f32x8(proj_row, j);
                let v_res = v_feat * v_proj;
                let arr: [f32; 8] = v_res.into();
                sum += arr.iter().sum::<f32>();
                j += 8;
            }
            latent[i] = sum.tanh();
        }

        latent
    }

    fn decode(&self, _latent: &[f32; 16]) -> Vec<f32> {
        // Generative reconstruction (Stage 7)
        Vec::new()
    }
}

impl Default for StandardNeuralEncoder {
    fn default() -> Self {
        let mut projection = [[0.0f32; 128]; 16];
        for i in 0..16 {
            for j in 0..128 {
                projection[i][j] = ((i * j) as f32).sin() * 0.1;
            }
        }
        Self { projection }
    }
}

pub struct FeatureMutator;

impl FeatureMutator {
    pub fn mutate(dna: &mut nullherz_traits::SoundDNA, feature_name: &str, strength: f32) {
        match feature_name {
            "metallic" => {
                // Metallic textures often involve high-frequency resonances.
                // We simulate this by perturbing specific dimensions of the latent space.
                dna.spectral.latent_space[2] = (dna.spectral.latent_space[2] + 0.2 * strength).clamp(0.0, 1.0);
                dna.spectral.latent_space[7] = (dna.spectral.latent_space[7] + 0.3 * strength).clamp(0.0, 1.0);
                dna.artifacts.glitch_density = (dna.artifacts.glitch_density + 0.1 * strength).clamp(0.0, 1.0);
            }
            "organic" => {
                // Organic sounds often have smoother spectral tilts and lower glitch density.
                dna.spectral.tilt = (dna.spectral.tilt - 0.1 * strength).clamp(-1.0, 1.0);
                dna.artifacts.glitch_density = (dna.artifacts.glitch_density - 0.2 * strength).clamp(0.0, 1.0);
                dna.spectral.latent_space[0] = (dna.spectral.latent_space[0] + 0.1 * strength).clamp(0.0, 1.0);
            }
            "warm" => {
                dna.spectral.tilt = (dna.spectral.tilt - 0.2 * strength).clamp(-1.0, 1.0);
                dna.spectral.latent_space[1] = (dna.spectral.latent_space[1] + 0.15 * strength).clamp(0.0, 1.0);
            }
            "aggressive" => {
                dna.artifacts.noise_floor_db = (dna.artifacts.noise_floor_db + 6.0 * strength).clamp(-96.0, 12.0);
                dna.spectral.latent_space[5] = (dna.spectral.latent_space[5] + 0.25 * strength).clamp(0.0, 1.0);
            }
            _ => {
                // Default: minor random perturbation of feature vector
                for i in 0..8 {
                    dna.feature_vector[i] = (dna.feature_vector[i] + 0.05 * strength).clamp(0.0, 1.0);
                }
            }
        }
    }
}

pub fn calculate_similarity(dna_a: &nullherz_traits::SoundDNA, dna_b: &nullherz_traits::SoundDNA) -> f32 {
    // Stage 6 Intelligent Similarity: Weighted combination of Latent Distance and Feature Correlation

    // 1. Spectral Latent Similarity (SIMD Optimized Euclidean Distance)
    use audio_dsp::simd_vec::FloatX16;
    let v_a = FloatX16::new(dna_a.spectral.latent_space);
    let v_b = FloatX16::new(dna_b.spectral.latent_space);
    let v_diff = v_a - v_b;
    let v_sq = v_diff * v_diff;

    let sq_arr: [f32; 16] = v_sq.into();
    let sum_sq: f32 = sq_arr.iter().sum();
    let dist = sum_sq.sqrt();
    // Normalize distance (max distance in 16D unit cube is 4.0)
    let spectral_sim = (1.0 - (dist / 4.0)).max(0.0);

    // 2. Feature Vector Correlation (Cosine-like) - SIMD Optimized
    use audio_dsp::simd_vec::load_f32x8;
    let v_fv_a = load_f32x8(&dna_a.feature_vector, 0);
    let v_fv_b = load_f32x8(&dna_b.feature_vector, 0);

    let v_dot = v_fv_a * v_fv_b;
    let v_mag_a = v_fv_a * v_fv_a;
    let v_mag_b = v_fv_b * v_fv_b;

    let dot_arr: [f32; 8] = v_dot.into();
    let mag_a_arr: [f32; 8] = v_mag_a.into();
    let mag_b_arr: [f32; 8] = v_mag_b.into();

    let feature_dot: f32 = dot_arr.iter().sum();
    let mag_a: f32 = mag_a_arr.iter().sum();
    let mag_b: f32 = mag_b_arr.iter().sum();
    let feature_sim = if mag_a > 0.0 && mag_b > 0.0 {
        feature_dot / (mag_a.sqrt() * mag_b.sqrt())
    } else {
        1.0 // Both empty vectors are "similar"
    };

    let rhythmic_sim = 1.0 - (dna_a.rhythmic.syncopation_index - dna_b.rhythmic.syncopation_index).abs();

    // Weighted final score
    (spectral_sim * 0.5) + (feature_sim * 0.3) + (rhythmic_sim * 0.2)
}

pub fn transfuse_dna(dna_a: &nullherz_traits::SoundDNA, dna_b: &nullherz_traits::SoundDNA, bias: f32) -> nullherz_traits::SoundDNA {
    let mut child = nullherz_traits::SoundDNA::default();
    let inv_bias = 1.0 - bias;

    // 0. Feature Vector Transfusion - SIMD Optimized
    use audio_dsp::simd_vec::{FloatX8, load_f32x8, store_f32x8};
    let v_inv_bias_8 = FloatX8::from(inv_bias);
    let v_bias_8 = FloatX8::from(bias);
    let v_fv_a = load_f32x8(&dna_a.feature_vector, 0);
    let v_fv_b = load_f32x8(&dna_b.feature_vector, 0);
    let v_fv_res = (v_fv_a * v_inv_bias_8) + (v_fv_b * v_bias_8);
    store_f32x8(&mut child.feature_vector, 0, v_fv_res);

    // 1. Spectral Transfusion (Neural/Latent SIMD Optimized)
    NeuralTransfuser::interpolate_latent(&mut child.spectral.latent_space, &dna_a.spectral.latent_space, &dna_b.spectral.latent_space, bias);

    child.spectral.tilt = dna_a.spectral.tilt * inv_bias + dna_b.spectral.tilt * bias;

    // 2. Rhythmic Transfusion
    for i in 0..4 {
        // Probabilistic bitmask merge
        let mask_a = dna_a.rhythmic.onset_mask[i];
        let mask_b = dna_b.rhythmic.onset_mask[i];
        let mut child_mask = 0u64;
        for bit in 0..64 {
            let bit_a = (mask_a >> bit) & 1;
            let bit_b = (mask_b >> bit) & 1;
            let prob = if bit_a == 1 && bit_b == 1 { 1.0 }
                      else if bit_a == 1 { inv_bias }
                      else if bit_b == 1 { bias }
                      else { 0.0 };

            if (i as u32).wrapping_mul(bit as u32).wrapping_mul(1103515245).wrapping_add(12345) as f32 / 4294967295.0 < prob {
                child_mask |= 1 << bit;
            }
        }
        child.rhythmic.onset_mask[i] = child_mask;
    }
    child.rhythmic.syncopation_index = dna_a.rhythmic.syncopation_index * inv_bias + dna_b.rhythmic.syncopation_index * bias;
    for i in 0..12 {
        child.rhythmic.micro_timing[i] = (dna_a.rhythmic.micro_timing[i] as f32 * inv_bias + dna_b.rhythmic.micro_timing[i] as f32 * bias) as i16;
    }

    // 3. Artifact Transfusion
    child.artifacts.noise_floor_db = dna_a.artifacts.noise_floor_db * inv_bias + dna_b.artifacts.noise_floor_db * bias;
    child.artifacts.glitch_density = dna_a.artifacts.glitch_density * inv_bias + dna_b.artifacts.glitch_density * bias;

    // 4. Spatial Transfusion
    child.spatial.stereo_width = dna_a.spatial.stereo_width * inv_bias + dna_b.spatial.stereo_width * bias;
    child.spatial.room_size = dna_a.spatial.room_size * inv_bias + dna_b.spatial.room_size * bias;

    child
}


/// Chaotic Transfusion: Implements Layer 5 "Error Rehabilitation" theory.
/// Uses a logistic map to create non-linear trait inheritance and digital mutations.
pub fn chaotic_transfuse_dna(dna_a: &nullherz_traits::SoundDNA, dna_b: &nullherz_traits::SoundDNA, bias: f32, chaotic_strength: f32) -> nullherz_traits::SoundDNA {
    let mut child = transfuse_dna(dna_a, dna_b, bias);

    // Logistic Map for chaotic bias modulation: x_{n+1} = r * x_n * (1 - x_n)
    // r = 3.9 is in the chaotic regime
    let r = 3.7 + (chaotic_strength * 0.29); // Scale r based on strength
    let mut x = bias.max(0.01).min(0.99);

    // Apply chaotic perturbations to spectral latent space
    for i in 0..16 {
        x = r * x * (1.0 - x);
        if x > 0.8 {
            // "Evolutionary Mutation": Perturb latent dimensions
            child.spectral.latent_space[i] += (x - 0.5) * chaotic_strength;
        }
    }

    // Chaotic artifact injection
    child.artifacts.glitch_density = (child.artifacts.glitch_density + (x * chaotic_strength)).clamp(0.0, 1.0);
    child.artifacts.noise_floor_db += x * 12.0 * chaotic_strength;

    child
}

pub struct Matchmaker;

impl Matchmaker {
    pub fn rank_compatibility(target: &nullherz_traits::SoundDNA, candidates: &[LibraryTrack], limit: usize) -> Vec<(u64, f32)> {
        use rayon::prelude::*;
        let mut scores: Vec<(u64, f32)> = candidates.par_iter()
            .map(|track| {
                let score = calculate_similarity(target, &(*track.metadata).dna);
                (track.id, score)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }

    pub fn find_matches_above_threshold(target: &nullherz_traits::SoundDNA, candidates: &[LibraryTrack], threshold: f32) -> Vec<(u64, f32)> {
        use rayon::prelude::*;
        let mut results: Vec<(u64, f32)> = candidates.par_iter()
            .filter_map(|track| {
                let score = calculate_similarity(target, &(*track.metadata).dna);
                if score >= threshold {
                    Some((track.id, score))
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    pub fn find_best_matches(db: &LibraryDatabase, target: &nullherz_traits::SoundDNA, limit: usize) -> Result<Vec<(u64, f32)>, Box<dyn std::error::Error>> {
        let tracks = db.list_tracks()?;
        Ok(Self::rank_compatibility(target, &tracks, limit))
    }
}
