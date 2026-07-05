use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};
use redb::{Database, TableDefinition, ReadableTable, TableError};

pub type SampleBuffer = Arc<Vec<f32>>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct LibraryTrack {
    pub id: u64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub metadata: nullherz_traits::SampleMetadata,
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
}

#[derive(Clone)]
pub struct RegisteredSample {
    pub buffer: SampleBuffer,
    pub metadata: nullherz_traits::SampleMetadata,
}

pub struct LibraryDatabase {
    db: Database,
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
        Ok(Self { db })
    }

    pub fn save_track(&self, track: &LibraryTrack) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TRACKS_TABLE)?;
            let serialized = serde_json::to_vec(track)?;
            table.insert(track.id, serialized.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_track(&self, id: u64) -> Result<Option<LibraryTrack>, Box<dyn std::error::Error>> {
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

    pub fn list_tracks(&self) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>> {
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

    pub fn add_to_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CRATES_TABLE)?;
            table.insert((crate_name, track_id), ())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_tracks_in_crate(&self, crate_name: &str) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let crate_table = match read_txn.open_table(CRATES_TABLE) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let track_table = read_txn.open_table(TRACKS_TABLE)?;

        let mut tracks = Vec::new();
        // Use range scan for O(log N) retrieval of crate members
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

    pub fn list_crates(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
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

    pub fn remove_from_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CRATES_TABLE)?;
            table.remove((crate_name, track_id))?;
        }
        write_txn.commit()?;
        Ok(())
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
            if let Some(dna) = sync_service.request_dna(id) {
                println!("Sync: Inherited SoundDNA '{}' from cloud peer.", name);
                // Breeding logic or library integration here...
            }
        }
        Ok(())
    }
}

pub trait PeerSync {
    fn announce_dna(&self, dna: &nullherz_traits::SoundDNA);
    fn request_dna(&self, id: u64) -> Option<nullherz_traits::SoundDNA>;
    fn list_peer_dna(&self) -> Vec<(u64, String)>;
}

pub struct DiscoveryService {
    pub known_peers: Vec<String>,
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self { known_peers: Vec::new() }
    }

    pub fn discover(&mut self) {
        // Stub for P2P discovery logic (libp2p/mdns)
        println!("P2P Discovery: Searching for peers in the genetic cloud...");
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
                let val = t.metadata.dna.spectral.tilt;
                val >= min && val <= max
            });
        }

        // 3. Filter by Rhythmic Syncopation
        if let Some((min, max)) = def.rhythmic_syncopation_range {
            results.retain(|t| {
                let val = t.metadata.dna.rhythmic.syncopation_index;
                val >= min && val <= max
            });
        }

        // 4. Filter by Glitch Density
        if let Some((min, max)) = def.glitch_density_range {
            results.retain(|t| {
                let val = t.metadata.dna.artifacts.glitch_density;
                val >= min && val <= max
            });
        }

        results
    }
}

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

    pub fn register(&self, id: u64, buffer: SampleBuffer) {
        self.register_with_metadata(id, buffer, nullherz_traits::SampleMetadata::new_empty());
    }

    pub fn register_with_metadata(&self, id: u64, buffer: SampleBuffer, metadata: nullherz_traits::SampleMetadata) {
        let _lock = self.write_lock.lock().unwrap();

        let old_ptr = self.inner.load(Ordering::Acquire);
        let mut new_map = unsafe { (*old_ptr).clone() };
        new_map.insert(id, RegisteredSample { buffer, metadata });

        let new_ptr = Box::into_raw(Box::new(new_map));
        self.inner.store(new_ptr, Ordering::Release);
        self.garbage.lock().unwrap().push(old_ptr);
    }

    pub fn drain_garbage(&self) {
        if self.readers.load(Ordering::SeqCst) > 0 { return; }
        let mut g = self.garbage.lock().unwrap();
        for ptr in g.drain(..) {
            unsafe { drop(Box::from_raw(ptr)); }
        }
    }

    pub fn get(&self, id: u64) -> Option<RegisteredSample> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).get(&id).cloned() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }

    pub fn list_ids(&self) -> Vec<u64> {
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).keys().cloned().collect() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
    }
}

impl Drop for SampleRegistry {
    fn drop(&mut self) {
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
    fn test_library_database_crud() {
        let db_path = "test_library.redb";
        let _ = fs::remove_file(db_path);

        let db = LibraryDatabase::load(db_path).unwrap();

        let track = LibraryTrack {
            id: 1,
            path: "/test/path.wav".to_string(),
            title: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
            metadata: nullherz_traits::SampleMetadata::new_empty(),
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
    fn test_library_crating() {
        let db_path = "test_crates.redb";
        let _ = fs::remove_file(db_path);
        let db = LibraryDatabase::load(db_path).unwrap();

        let track = LibraryTrack {
            id: 101,
            path: "/test/track.wav".to_string(),
            title: "Crate Track".to_string(),
            artist: "Crate Artist".to_string(),
            metadata: nullherz_traits::SampleMetadata::new_empty(),
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
            assert!((child.spectral.latent_space[i] - 0.5).abs() < 0.001);
        }

        let child_025 = transfuse_dna(&dna_a, &dna_b, 0.25);
        for i in 0..16 {
            // (0.2 * 0.75) + (0.8 * 0.25) = 0.15 + 0.2 = 0.35
            assert!((child_025.spectral.latent_space[i] - 0.35).abs() < 0.001);
        }
    }

    #[test]
    fn test_smart_crate_filtering() {
        let mut track_a = LibraryTrack {
            id: 1,
            path: "a.wav".into(),
            title: "A".into(),
            artist: "A".into(),
            metadata: nullherz_traits::SampleMetadata::new_empty(),
        };
        track_a.metadata.dna.spectral.tilt = 0.5;

        let mut track_b = LibraryTrack {
            id: 2,
            path: "b.wav".into(),
            title: "B".into(),
            artist: "B".into(),
            metadata: nullherz_traits::SampleMetadata::new_empty(),
        };
        track_b.metadata.dna.spectral.tilt = -0.5;

        let def = SmartCrateDefinition {
            name: "Tilt High".into(),
            target_dna: None,
            threshold: 0.0,
            spectral_tilt_range: Some((0.1, 1.0)),
            rhythmic_syncopation_range: None,
            glitch_density_range: None,
        };

        let filtered = SmartCrateManager::filter_tracks(&def, vec![track_a.clone(), track_b.clone()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 1);
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
}

pub struct NeuralTransfuser;

impl NeuralTransfuser {
    pub fn interpolate_latent(dest: &mut [f32; 16], src_a: &[f32; 16], src_b: &[f32; 16], bias: f32) {
        use audio_dsp::simd_vec::FloatX16;
        let v_inv_bias = FloatX16::splat(1.0 - bias);
        let v_bias = FloatX16::splat(bias);

        let v_a = FloatX16::new(*src_a);
        let v_b = FloatX16::new(*src_b);
        let v_res = (v_a * v_inv_bias) + (v_b * v_bias);

        *dest = v_res.into();
    }
}

pub struct FeatureMutator;

impl FeatureMutator {
    pub fn mutate(dna: &mut nullherz_traits::SoundDNA, feature_name: &str, strength: f32) {
        match feature_name {
            "metallic" => {
                // Metallic textures often involve high-frequency resonances.
                // We simulate this by perturbing specific dimensions of the latent space.
                dna.spectral.latent_space[2] += 0.2 * strength;
                dna.spectral.latent_space[7] += 0.3 * strength;
                dna.artifacts.glitch_density = (dna.artifacts.glitch_density + 0.1 * strength).clamp(0.0, 1.0);
            }
            "organic" => {
                // Organic sounds often have smoother spectral tilts and lower glitch density.
                dna.spectral.tilt -= 0.1 * strength;
                dna.artifacts.glitch_density = (dna.artifacts.glitch_density - 0.2 * strength).clamp(0.0, 1.0);
                dna.spectral.latent_space[0] += 0.1 * strength;
            }
            "warm" => {
                dna.spectral.tilt -= 0.2 * strength;
                dna.spectral.latent_space[1] += 0.15 * strength;
            }
            "aggressive" => {
                dna.artifacts.noise_floor_db += 6.0 * strength;
                dna.spectral.latent_space[5] += 0.25 * strength;
            }
            _ => {
                // Default: minor random perturbation of feature vector
                for i in 0..8 {
                    dna.feature_vector[i] += 0.05 * strength;
                }
            }
        }
    }
}

pub fn calculate_similarity(dna_a: &nullherz_traits::SoundDNA, dna_b: &nullherz_traits::SoundDNA) -> f32 {
    let mut spectral_sim = 0.0;
    for i in 0..16 {
        let diff = (dna_a.spectral.latent_space[i] - dna_b.spectral.latent_space[i]).abs();
        spectral_sim += 1.0 - diff.min(1.0);
    }
    spectral_sim /= 16.0;

    let rhythmic_sim = 1.0 - (dna_a.rhythmic.syncopation_index - dna_b.rhythmic.syncopation_index).abs();

    (spectral_sim * 0.7) + (rhythmic_sim * 0.3)
}

pub fn transfuse_dna(dna_a: &nullherz_traits::SoundDNA, dna_b: &nullherz_traits::SoundDNA, bias: f32) -> nullherz_traits::SoundDNA {
    let mut child = nullherz_traits::SoundDNA::default();
    let inv_bias = 1.0 - bias;

    // 0. Feature Vector Transfusion
    for i in 0..8 {
        child.feature_vector[i] = dna_a.feature_vector[i] * inv_bias + dna_b.feature_vector[i] * bias;
    }

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
                let score = calculate_similarity(target, &track.metadata.dna);
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
                let score = calculate_similarity(target, &track.metadata.dna);
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
