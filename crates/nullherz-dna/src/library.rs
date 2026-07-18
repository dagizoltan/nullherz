use std::sync::Arc;
use parking_lot::Mutex;
use redb::{Database, TableDefinition, ReadableTable, TableError};
use crate::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct LibraryTrack {
    pub id: u64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub energy_level: f32,
    #[serde(with = "crate::consensus::serde_arc")]
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
    transient_path: Option<String>,
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
            if let Some(g) = genre
                && t.genre != g { return false; }
            if let Some(min) = min_bpm
                && t.metadata.bpm < min { return false; }
            if let Some(max) = max_bpm
                && t.metadata.bpm > max { return false; }
            if let Some(key) = root_key
                && t.metadata.root_key != Some(key) { return false; }
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
        let is_transient = path == ":memory:";
        let db_path = if is_transient {
            let mut temp = std::env::temp_dir();
            static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            temp.push(format!("nullherz_transient_{}_{}_{}.redb", std::process::id(), count, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
            temp.to_string_lossy().to_string()
        } else {
            path.to_string()
        };
        let db = Database::create(&db_path)?;
        // Ensure table exists
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(TRACKS_TABLE)?;
            let _ = write_txn.open_table(CRATES_TABLE)?;
            let _ = write_txn.open_table(SMART_CRATES_TABLE)?;
        }
        write_txn.commit()?;
        Ok(Self {
            db,
            merkle_root: Mutex::new([0u8; 32]),
            transient_path: if is_transient { Some(db_path) } else { None },
        })
    }

    pub fn update_merkle_root(&self) -> Result<[u8; 32], Box<dyn std::error::Error>> {
        use sha2::{Sha256, Digest};
        let tracks = self.list_tracks()?;
        let mut hasher = Sha256::new();
        for track in tracks {
            let dna_bytes = serde_json::to_vec(&track.metadata.dna)?;
            hasher.update(&dna_bytes);
        }
        let hash: [u8; 32] = hasher.finalize().into();
        let mut root = self.merkle_root.lock();
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
            sync_service.announce_dna(&track.metadata.dna);
        }

        let remote_dna = sync_service.list_peer_dna();
        for (id, name) in remote_dna {
            if self.get_track(id)?.is_none()
                && let Some(dna) = sync_service.request_dna(id) {
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
        Ok(())
    }
}

impl Drop for LibraryDatabase {
    fn drop(&mut self) {
        if let Some(ref path) = self.transient_path {
            let _ = std::fs::remove_file(path);
        }
    }
}
