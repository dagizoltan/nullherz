use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};
use redb::{Database, TableDefinition, ReadableTable, TableError};

pub type SampleBuffer = Arc<Vec<f32>>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SampleMetadata {
    pub bpm: f32,
    #[serde(skip)]
    pub transients: Arc<Vec<u64>>,
    pub root_key: Option<f32>,
    pub hot_cues: [Option<u64>; 8],
    pub loop_points: Option<(u64, u64)>,
    pub beat_grid_offset: u64,
    #[serde(skip)]
    pub peaks: Arc<Vec<f32>>,
}

impl SampleMetadata {
    pub fn new_empty() -> Self {
        Self {
            bpm: 0.0,
            transients: Arc::new(Vec::new()),
            root_key: None,
            hot_cues: [None; 8],
            loop_points: None,
            beat_grid_offset: 0,
            peaks: Arc::new(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct LibraryTrack {
    pub id: u64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub metadata: SampleMetadata,
}

const TRACKS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("tracks");

#[derive(Clone)]
pub struct RegisteredSample {
    pub buffer: SampleBuffer,
    pub metadata: SampleMetadata,
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
        self.register_with_metadata(id, buffer, SampleMetadata::new_empty());
    }

    pub fn register_with_metadata(&self, id: u64, buffer: SampleBuffer, metadata: SampleMetadata) {
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
            metadata: SampleMetadata::new_empty(),
        };

        db.save_track(&track).unwrap();

        let loaded = db.get_track(1).unwrap().unwrap();
        assert_eq!(loaded, track);

        let list = db.list_tracks().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], track);

        let _ = fs::remove_file(db_path);
    }
}
