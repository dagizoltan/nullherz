use std::sync::Arc;
use parking_lot::Mutex;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AssetKind {
    AudioTrack,
    AudioSample,
    MusicalSequence,
    AudioInstrument,
    AudioInsert,
    VisualGenerator,
    VisualInsert,
}

impl AssetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AssetKind::AudioTrack => "AudioTrack",
            AssetKind::AudioSample => "AudioSample",
            AssetKind::MusicalSequence => "MusicalSequence",
            AssetKind::AudioInstrument => "AudioInstrument",
            AssetKind::AudioInsert => "AudioInsert",
            AssetKind::VisualGenerator => "VisualGenerator",
            AssetKind::VisualInsert => "VisualInsert",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "AudioTrack" => Some(AssetKind::AudioTrack),
            "AudioSample" => Some(AssetKind::AudioSample),
            "MusicalSequence" => Some(AssetKind::MusicalSequence),
            "AudioInstrument" => Some(AssetKind::AudioInstrument),
            "AudioInsert" => Some(AssetKind::AudioInsert),
            "VisualGenerator" => Some(AssetKind::VisualGenerator),
            "VisualInsert" => Some(AssetKind::VisualInsert),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetManifest {
    pub asset_id: String,             // SHA-256 / UUID
    pub name: String,
    pub kind: AssetKind,
    pub version: String,
    pub author: String,
    pub is_free: bool,
    pub download_size_bytes: u64,
    pub tags: Vec<String>,
    pub description: String,
    pub signature: Option<String>,    // Ed25519 cryptographic signature
    pub binary_filename: Option<String>,
}

impl AssetManifest {
    pub fn new(
        asset_id: impl Into<String>,
        name: impl Into<String>,
        kind: AssetKind,
        version: impl Into<String>,
        author: impl Into<String>,
    ) -> Self {
        Self {
            asset_id: asset_id.into(),
            name: name.into(),
            kind,
            version: version.into(),
            author: author.into(),
            is_free: true,
            download_size_bytes: 0,
            tags: Vec::new(),
            description: String::new(),
            signature: None,
            binary_filename: None,
        }
    }
}

/// Hybrid SQLite relational asset database for searching, filtering, and entitlements
pub struct AssetDatabase {
    conn: Arc<Mutex<Connection>>,
}

impl AssetDatabase {
    /// Open or create an SQLite database at `db_path` (or `":memory:"` for tests)
    pub fn new(db_path: &str) -> SqlResult<Self> {
        let conn = if db_path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            Connection::open(db_path)?
        };

        conn.execute(
            "CREATE TABLE IF NOT EXISTS asset_manifests (
                asset_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                version TEXT NOT NULL,
                author TEXT NOT NULL,
                is_free INTEGER NOT NULL,
                download_size_bytes INTEGER NOT NULL,
                tags TEXT NOT NULL,
                description TEXT NOT NULL,
                signature TEXT,
                binary_filename TEXT,
                installed_at INTEGER NOT NULL
            )",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Save or replace an AssetManifest in SQLite
    pub fn save_manifest(&self, manifest: &AssetManifest) -> SqlResult<()> {
        let conn = self.conn.lock();
        let tags_json = serde_json::to_string(&manifest.tags).unwrap_or_else(|_| "[]".to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        conn.execute(
            "INSERT OR REPLACE INTO asset_manifests
             (asset_id, name, kind, version, author, is_free, download_size_bytes, tags, description, signature, binary_filename, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                manifest.asset_id,
                manifest.name,
                manifest.kind.as_str(),
                manifest.version,
                manifest.author,
                if manifest.is_free { 1 } else { 0 },
                manifest.download_size_bytes as i64,
                tags_json,
                manifest.description,
                manifest.signature,
                manifest.binary_filename,
                now,
            ],
        )?;

        Ok(())
    }

    /// Retrieve an AssetManifest by asset_id
    pub fn get_manifest(&self, asset_id: &str) -> SqlResult<Option<AssetManifest>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT asset_id, name, kind, version, author, is_free, download_size_bytes, tags, description, signature, binary_filename
             FROM asset_manifests WHERE asset_id = ?1",
        )?;

        let mut rows = stmt.query(params![asset_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_manifest(row)?))
        } else {
            Ok(None)
        }
    }

    /// List all AssetManifests in database
    pub fn list_manifests(&self) -> SqlResult<Vec<AssetManifest>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT asset_id, name, kind, version, author, is_free, download_size_bytes, tags, description, signature, binary_filename
             FROM asset_manifests ORDER BY name ASC",
        )?;

        let rows = stmt.query_map([], |row| Self::row_to_manifest(row))?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    /// Query manifests by AssetKind
    pub fn query_by_kind(&self, kind: AssetKind) -> SqlResult<Vec<AssetManifest>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT asset_id, name, kind, version, author, is_free, download_size_bytes, tags, description, signature, binary_filename
             FROM asset_manifests WHERE kind = ?1 ORDER BY name ASC",
        )?;

        let rows = stmt.query_map(params![kind.as_str()], |row| Self::row_to_manifest(row))?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    /// Delete an AssetManifest by asset_id
    pub fn delete_manifest(&self, asset_id: &str) -> SqlResult<bool> {
        let conn = self.conn.lock();
        let count = conn.execute("DELETE FROM asset_manifests WHERE asset_id = ?1", params![asset_id])?;
        Ok(count > 0)
    }

    fn row_to_manifest(row: &rusqlite::Row) -> SqlResult<AssetManifest> {
        let asset_id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let kind_str: String = row.get(2)?;
        let version: String = row.get(3)?;
        let author: String = row.get(4)?;
        let is_free_num: i32 = row.get(5)?;
        let size_num: i64 = row.get(6)?;
        let tags_str: String = row.get(7)?;
        let description: String = row.get(8)?;
        let signature: Option<String> = row.get(9)?;
        let binary_filename: Option<String> = row.get(10)?;

        let kind = AssetKind::from_str(&kind_str).unwrap_or(AssetKind::AudioInstrument);
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

        Ok(AssetManifest {
            asset_id,
            name,
            kind,
            version,
            author,
            is_free: is_free_num != 0,
            download_size_bytes: size_num as u64,
            tags,
            description,
            signature,
            binary_filename,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_database_crud() {
        let db = AssetDatabase::new(":memory:").unwrap();

        let manifest = AssetManifest::new(
            "neural-saturation",
            "Neural Saturation Insert",
            AssetKind::AudioInsert,
            "1.2.0",
            "Nullherz Core",
        );

        db.save_manifest(&manifest).unwrap();

        let fetched = db.get_manifest("neural-saturation").unwrap().unwrap();
        assert_eq!(fetched.name, "Neural Saturation Insert");
        assert_eq!(fetched.kind, AssetKind::AudioInsert);

        let list = db.list_manifests().unwrap();
        assert_eq!(list.len(), 1);

        let filtered = db.query_by_kind(AssetKind::AudioInsert).unwrap();
        assert_eq!(filtered.len(), 1);

        let deleted = db.delete_manifest("neural-saturation").unwrap();
        assert!(deleted);
        assert!(db.get_manifest("neural-saturation").unwrap().is_none());
    }
}
