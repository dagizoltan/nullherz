use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::{Mutex, RwLock};
use redb::{Database, TableDefinition, ReadableTable, ReadableTableMetadata, TableError};
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

/// The small, queryable subset of a track — exactly the fields the query /
/// smart-crate / matchmaking predicates read (genre, energy, bpm, key, and the
/// fixed-size `SoundDNA`), WITHOUT the heavy waveform metadata. Cached in memory
/// (see `LibraryDatabase::facets`) so those queries filter over ~hundreds of
/// bytes per track instead of re-reading and deserializing the entire library's
/// waveforms from redb on every call.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackFacets {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub energy_level: f32,
    pub bpm: f32,
    pub root_key: Option<f32>,
    pub dna: nullherz_traits::SoundDNA,
}

impl LibraryTrack {
    /// Extract the cacheable query facets. Cheap: clones the short text fields
    /// and a fixed-size DNA struct — no waveform data is copied.
    pub fn facets(&self) -> TrackFacets {
        TrackFacets {
            id: self.id,
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            genre: self.genre.clone(),
            energy_level: self.energy_level,
            bpm: self.metadata.bpm,
            root_key: self.metadata.root_key,
            dna: self.metadata.dna.clone(),
        }
    }
}

/// Sort order for library queries / search results.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TrackSort {
    /// Alphabetical by title (default).
    #[default]
    Title,
    Artist,
    Album,
    Genre,
    BpmAsc,
    BpmDesc,
    EnergyAsc,
    EnergyDesc,
}

impl TrackSort {
    /// Order two facets under this sort. Text is compared case-insensitively;
    /// numeric ties fall back to title so results are deterministic.
    fn cmp(self, a: &TrackFacets, b: &TrackFacets) -> std::cmp::Ordering {
        use std::cmp::Ordering::Equal;
        let by_title = || a.title.to_lowercase().cmp(&b.title.to_lowercase());
        match self {
            TrackSort::Title => by_title(),
            TrackSort::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()).then_with(by_title),
            TrackSort::Album => a.album.to_lowercase().cmp(&b.album.to_lowercase()).then_with(by_title),
            TrackSort::Genre => a.genre.to_lowercase().cmp(&b.genre.to_lowercase()).then_with(by_title),
            TrackSort::BpmAsc => a.bpm.partial_cmp(&b.bpm).unwrap_or(Equal).then_with(by_title),
            TrackSort::BpmDesc => b.bpm.partial_cmp(&a.bpm).unwrap_or(Equal).then_with(by_title),
            TrackSort::EnergyAsc => a.energy_level.partial_cmp(&b.energy_level).unwrap_or(Equal).then_with(by_title),
            TrackSort::EnergyDesc => b.energy_level.partial_cmp(&a.energy_level).unwrap_or(Equal).then_with(by_title),
        }
    }

    /// Sort a slice of full tracks by this order — for callers that already hold
    /// `LibraryTrack`s in memory (e.g. the Library view). Same ordering as `cmp`.
    pub fn order_tracks(self, tracks: &mut [LibraryTrack]) {
        use std::cmp::Ordering::Equal;
        let by_title = |a: &LibraryTrack, b: &LibraryTrack| a.title.to_lowercase().cmp(&b.title.to_lowercase());
        tracks.sort_by(|a, b| match self {
            TrackSort::Title => by_title(a, b),
            TrackSort::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()).then_with(|| by_title(a, b)),
            TrackSort::Album => a.album.to_lowercase().cmp(&b.album.to_lowercase()).then_with(|| by_title(a, b)),
            TrackSort::Genre => a.genre.to_lowercase().cmp(&b.genre.to_lowercase()).then_with(|| by_title(a, b)),
            TrackSort::BpmAsc => a.metadata.bpm.partial_cmp(&b.metadata.bpm).unwrap_or(Equal).then_with(|| by_title(a, b)),
            TrackSort::BpmDesc => b.metadata.bpm.partial_cmp(&a.metadata.bpm).unwrap_or(Equal).then_with(|| by_title(a, b)),
            TrackSort::EnergyAsc => a.energy_level.partial_cmp(&b.energy_level).unwrap_or(Equal).then_with(|| by_title(a, b)),
            TrackSort::EnergyDesc => b.energy_level.partial_cmp(&a.energy_level).unwrap_or(Equal).then_with(|| by_title(a, b)),
        });
    }
}

const TRACKS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("tracks");
const CRATES_TABLE: TableDefinition<(&str, u64), ()> = TableDefinition::new("crates_v2");
const SMART_CRATES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("smart_crates");
/// Persisted small query facets (id → serialized `TrackFacets`, no waveform),
/// written in the same transaction as the full track. Lets the in-memory index
/// be (re)built by reading these tiny rows instead of every full track blob —
/// so the first query after boot is fast, and it survives restarts.
/// `_v2`: the facet schema gained title/artist/album for text search; the old
/// `facets` rows lack them, so a fresh table name forces a one-time backfill
/// from the full tracks (which carry the text) rather than a lossy serde default.
const FACETS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("facets_v2");

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
    /// In-memory facet index (id → queryable fields), lazily built on the first
    /// query (`None` until then, so `load` stays cheap and boot never regresses),
    /// then updated incrementally in `save_track`/`remove_track` — coherent
    /// because every track write in the workspace goes through those two methods.
    /// `query_tracks`, `get_smart_crate_tracks`, and `suggest_matches` filter
    /// over this instead of deserializing the whole library from redb per call.
    /// `RwLock` gives interior mutability under the `&self` trait methods.
    facets: RwLock<Option<HashMap<u64, TrackFacets>>>,
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
        let facets = track.facets();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TRACKS_TABLE)?;
            let serialized = serde_json::to_vec(track)?;
            table.insert(track.id, serialized.as_slice())?;
            // Persist the small query facets alongside the full track IN THE SAME
            // transaction, so the two tables can never drift.
            let mut facets_table = write_txn.open_table(FACETS_TABLE)?;
            facets_table.insert(track.id, serde_json::to_vec(&facets)?.as_slice())?;
        }
        write_txn.commit()?;
        // Keep the in-memory facet index coherent (insert-or-replace) — only if it
        // has been built; otherwise the lazy build picks this up from redb later.
        if let Some(map) = self.facets.write().as_mut() {
            map.insert(track.id, facets);
        }
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
        // Filter the in-memory facet index (no waveform deserialization), then
        // fetch full tracks only for the matches.
        self.ensure_index()?;
        let ids: Vec<u64> = {
            let guard = self.facets.read();
            let facets = guard.as_ref().expect("ensure_index just built it");
            facets.values().filter(|f| {
                if let Some(g) = genre
                    && f.genre != g { return false; }
                if let Some(min) = min_bpm
                    && f.bpm < min { return false; }
                if let Some(max) = max_bpm
                    && f.bpm > max { return false; }
                if let Some(key) = root_key
                    && f.root_key != Some(key) { return false; }
                true
            }).map(|f| f.id).collect()
        };
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(t) = self.get_track(id)? {
                results.push(t);
            }
        }
        Ok(results)
    }

    fn suggest_matches(&self, target_dna: &nullherz_traits::SoundDNA, limit: usize) -> Result<Vec<(u64, f32)>, Box<dyn std::error::Error>> {
        // Rank over the facet index (DNA is cached there) — matchmaking returns
        // only (id, score), so no full-track fetch is needed at all.
        self.ensure_index()?;
        let mut scores: Vec<(u64, f32)> = {
            let guard = self.facets.read();
            let facets = guard.as_ref().expect("ensure_index just built it");
            facets.values()
                .map(|f| (f.id, crate::transfusion::calculate_similarity(target_dna, &f.dna)))
                .collect()
        };
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        Ok(scores)
    }

    fn remove_track(&self, id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut track_table = write_txn.open_table(TRACKS_TABLE)?;
            track_table.remove(id)?;
            write_txn.open_table(FACETS_TABLE)?.remove(id)?;

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
        if let Some(map) = self.facets.write().as_mut() {
            map.remove(&id);
        }
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
            let _ = write_txn.open_table(FACETS_TABLE)?;
        }
        write_txn.commit()?;
        Ok(Self {
            db,
            merkle_root: Mutex::new([0u8; 32]),
            transient_path: if is_transient { Some(db_path) } else { None },
            facets: RwLock::new(None),
        })
    }

    /// Ensure the facet index is built (lazily, on first query). Deserializes
    /// each track ONE AT A TIME straight from redb — the full `LibraryTrack`
    /// (waveforms and all) is dropped as soon as its small facets are extracted,
    /// so peak memory is one track, not the whole library. This is the same
    /// deserialization cost the old per-query full scans paid, now paid a single
    /// Ensure the in-memory facet index is built (lazily, on first query). Fast
    /// path: read the persisted `FACETS_TABLE` (tiny rows, no waveform blobs).
    /// If it is incomplete versus the track count — a library written before the
    /// facets table existed, or any drift — backfill it once from the full
    /// tracks (streamed one at a time). After a backfill, every future load takes
    /// the fast path.
    fn ensure_index(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.facets.read().is_some() {
            return Ok(());
        }
        let read_txn = self.db.begin_read()?;
        let tracks_len = match read_txn.open_table(TRACKS_TABLE) {
            Ok(t) => t.len()?,
            Err(TableError::TableDoesNotExist(_)) => 0,
            Err(e) => return Err(e.into()),
        };
        let mut map = HashMap::new();
        match read_txn.open_table(FACETS_TABLE) {
            Ok(t) => {
                for res in t.iter()? {
                    let (id, val) = res?;
                    let facet: TrackFacets = serde_json::from_slice(val.value())?;
                    map.insert(id.value(), facet);
                }
            }
            Err(TableError::TableDoesNotExist(_)) => {}
            Err(e) => return Err(e.into()),
        }
        drop(read_txn);

        if map.len() as u64 != tracks_len {
            // Migration / self-heal: (re)populate the facets table once.
            map = self.backfill_facets()?;
        }

        let mut w = self.facets.write();
        if w.is_none() {
            *w = Some(map);
        }
        Ok(())
    }

    /// One-time backfill: scan the full tracks (streaming one at a time, so peak
    /// memory is a single track), extract facets, persist them to `FACETS_TABLE`,
    /// and return the in-memory map. Runs only when the facets table is missing
    /// or out of sync with the tracks table.
    fn backfill_facets(&self) -> Result<HashMap<u64, TrackFacets>, Box<dyn std::error::Error>> {
        let mut map = HashMap::new();
        {
            let read_txn = self.db.begin_read()?;
            match read_txn.open_table(TRACKS_TABLE) {
                Ok(table) => {
                    for res in table.iter()? {
                        let (_id, val) = res?;
                        let track: LibraryTrack = serde_json::from_slice(val.value())?;
                        map.insert(track.id, track.facets());
                        // `track` (with its waveform metadata) drops here.
                    }
                }
                Err(TableError::TableDoesNotExist(_)) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let write_txn = self.db.begin_write()?;
        {
            let mut facets_table = write_txn.open_table(FACETS_TABLE)?;
            for (id, facet) in &map {
                facets_table.insert(*id, serde_json::to_vec(facet)?.as_slice())?;
            }
        }
        write_txn.commit()?;
        Ok(map)
    }

    /// Test hook: wipe the persisted facets table to simulate a pre-facets-table
    /// library, so the backfill/self-heal path can be exercised.
    #[cfg(test)]
    pub(crate) fn wipe_facets_table_for_test(&self) -> Result<(), Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut t = write_txn.open_table(FACETS_TABLE)?;
            let ids: Vec<u64> = t.iter()?.filter_map(|r| r.ok().map(|(k, _)| k.value())).collect();
            for id in ids { t.remove(id)?; }
        }
        write_txn.commit()?;
        *self.facets.write() = None; // force a rebuild on next query
        Ok(())
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
            // Filter the facet index for matching ids, then fetch only those
            // full tracks from redb.
            self.ensure_index()?;
            let ids = {
                let guard = self.facets.read();
                let facets = guard.as_ref().expect("ensure_index just built it");
                SmartCrateManager::filter_facet_ids(&def, facets.values())
            };
            let mut results = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(t) = self.get_track(id)? {
                    results.push(t);
                }
            }
            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    /// Case-insensitive substring search over title / artist / album / genre,
    /// filtered on the in-memory facet index (no full-track deserialization for
    /// non-matches), sorted by `sort`, then full tracks fetched for the matches
    /// only. An empty/whitespace query returns the whole library in sorted order.
    pub fn search_tracks(&self, query: &str, sort: TrackSort) -> Result<Vec<LibraryTrack>, Box<dyn std::error::Error>> {
        self.ensure_index()?;
        let q = query.trim().to_lowercase();
        let ids: Vec<u64> = {
            let guard = self.facets.read();
            let facets = guard.as_ref().expect("ensure_index just built it");
            let mut matched: Vec<&TrackFacets> = facets.values().filter(|f| {
                q.is_empty()
                    || f.title.to_lowercase().contains(&q)
                    || f.artist.to_lowercase().contains(&q)
                    || f.album.to_lowercase().contains(&q)
                    || f.genre.to_lowercase().contains(&q)
            }).collect();
            matched.sort_by(|a, b| sort.cmp(a, b));
            matched.into_iter().map(|f| f.id).collect()
        };
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(t) = self.get_track(id)? {
                results.push(t);
            }
        }
        Ok(results)
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
