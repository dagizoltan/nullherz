//! Sidecar Package Manager (.sidecar Bundling & 1-Click Installer).
//! Packages binary executables / WASM modules, YAML/JSON parameter metadata,
//! and preview thumbnails into self-contained 1-click installable `.sidecar` archives.

use std::fs::{self, File};
use std::io::{Read, Write, Error, ErrorKind};
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use crate::store::SidecarType;

pub const SIDECAR_MAGIC_HEADER: &[u8; 8] = b"NHZSDCAR";
pub const SIDECAR_PACKAGE_VERSION: u32 = 1;

pub const ENTRY_TAG_MANIFEST: u8 = 0x01;
pub const ENTRY_TAG_BINARY: u8 = 0x02;
pub const ENTRY_TAG_PARAMETER_METADATA: u8 = 0x03;
pub const ENTRY_TAG_THUMBNAIL: u8 = 0x04;

/// Manifest descriptor stored inside a `.sidecar` package archive
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidecarPackageManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub sidecar_type: SidecarType,
    pub tags: Vec<String>,
    pub description: String,
    pub latency_samples: usize,
    pub binary_filename: String,
    #[serde(default)]
    pub thumbnail_filename: Option<String>,
    #[serde(default)]
    pub parameter_metadata_filename: Option<String>,
}

/// Self-contained `.sidecar` package bundle containing binary payload, metadata, and preview thumbnail
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarBundle {
    pub manifest: SidecarPackageManifest,
    pub binary_bytes: Vec<u8>,
    pub parameter_metadata: Option<String>,
    pub thumbnail_bytes: Option<Vec<u8>>,
}

impl SidecarBundle {
    pub fn new(
        manifest: SidecarPackageManifest,
        binary_bytes: Vec<u8>,
        parameter_metadata: Option<String>,
        thumbnail_bytes: Option<Vec<u8>>,
    ) -> Self {
        Self {
            manifest,
            binary_bytes,
            parameter_metadata,
            thumbnail_bytes,
        }
    }

    /// Export the bundle into an in-memory byte array formatted as a `.sidecar` archive
    pub fn export_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();

        // 1. Magic Header (8 bytes) + Version (u32 LE)
        buf.write_all(SIDECAR_MAGIC_HEADER)?;
        buf.write_all(&SIDECAR_PACKAGE_VERSION.to_le_bytes())?;

        // Helper to write a TLV entry
        let mut write_entry = |tag: u8, payload: &[u8]| -> Result<(), Error> {
            buf.write_all(&[tag])?;
            let len = payload.len() as u32;
            buf.write_all(&len.to_le_bytes())?;
            buf.write_all(payload)?;
            Ok(())
        };

        // 2. Manifest entry (Tag 0x01)
        let manifest_json = serde_json::to_string_pretty(&self.manifest)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        write_entry(ENTRY_TAG_MANIFEST, manifest_json.as_bytes())?;

        // 3. Binary payload entry (Tag 0x02)
        write_entry(ENTRY_TAG_BINARY, &self.binary_bytes)?;

        // 4. Parameter metadata entry (Tag 0x03)
        if let Some(ref param_meta) = self.parameter_metadata {
            write_entry(ENTRY_TAG_PARAMETER_METADATA, param_meta.as_bytes())?;
        }

        // 5. Preview thumbnail entry (Tag 0x04)
        if let Some(ref thumb_bytes) = self.thumbnail_bytes {
            write_entry(ENTRY_TAG_THUMBNAIL, thumb_bytes)?;
        }

        Ok(buf)
    }

    /// Import and parse a `.sidecar` package bundle from in-memory archive bytes
    pub fn import_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 12 {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid .sidecar archive: header too short"));
        }

        if &bytes[0..8] != SIDECAR_MAGIC_HEADER {
            return Err(Error::new(ErrorKind::InvalidData, "Invalid .sidecar archive: magic header mismatch"));
        }

        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if version != SIDECAR_PACKAGE_VERSION {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Unsupported .sidecar archive version {}", version),
            ));
        }

        let mut pos = 12;
        let mut manifest: Option<SidecarPackageManifest> = None;
        let mut binary_bytes: Option<Vec<u8>> = None;
        let mut parameter_metadata: Option<String> = None;
        let mut thumbnail_bytes: Option<Vec<u8>> = None;

        while pos < bytes.len() {
            if pos + 5 > bytes.len() {
                return Err(Error::new(ErrorKind::InvalidData, "Truncated .sidecar entry header"));
            }

            let tag = bytes[pos];
            let len = u32::from_le_bytes(bytes[pos + 1..pos + 5].try_into().unwrap()) as usize;
            pos += 5;

            if pos + len > bytes.len() {
                return Err(Error::new(ErrorKind::InvalidData, "Truncated .sidecar entry payload"));
            }

            let payload = &bytes[pos..pos + len];
            pos += len;

            match tag {
                ENTRY_TAG_MANIFEST => {
                    let manifest_str = std::str::from_utf8(payload)
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                    let parsed: SidecarPackageManifest = serde_json::from_str(manifest_str)
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                    manifest = Some(parsed);
                }
                ENTRY_TAG_BINARY => {
                    binary_bytes = Some(payload.to_vec());
                }
                ENTRY_TAG_PARAMETER_METADATA => {
                    let param_str = std::str::from_utf8(payload)
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                    parameter_metadata = Some(param_str.to_string());
                }
                ENTRY_TAG_THUMBNAIL => {
                    thumbnail_bytes = Some(payload.to_vec());
                }
                _ => {
                    // Ignore unknown future entry tags for forward compatibility
                }
            }
        }

        let manifest = manifest.ok_ok_or_data("Missing manifest in .sidecar archive")?;
        let binary_bytes = binary_bytes.ok_ok_or_data("Missing binary payload in .sidecar archive")?;

        Ok(Self {
            manifest,
            binary_bytes,
            parameter_metadata,
            thumbnail_bytes,
        })
    }

    /// Export bundle to a `.sidecar` file
    pub fn export_to_file(&self, path: &Path) -> Result<(), Error> {
        let bytes = self.export_bytes()?;
        let mut file = File::create(path)?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }

    /// Import bundle from a `.sidecar` file
    pub fn import_from_file(path: &Path) -> Result<Self, Error> {
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Self::import_bytes(&bytes)
    }
}

trait OptionExt<T> {
    fn ok_ok_or_data(self, msg: &'static str) -> Result<T, Error>;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_ok_or_data(self, msg: &'static str) -> Result<T, Error> {
        self.ok_or_else(|| Error::new(ErrorKind::InvalidData, msg))
    }
}

/// Installation paths resulting from 1-click bundle installation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackageInfo {
    pub target_directory: PathBuf,
    pub manifest_path: PathBuf,
    pub binary_path: PathBuf,
    pub parameter_metadata_path: Option<PathBuf>,
    pub thumbnail_path: Option<PathBuf>,
}

/// Sidecar Package Manager utility for packaging and 1-click installation
pub struct SidecarPackageManager;

impl SidecarPackageManager {
    /// Pack a SidecarBundle into a `.sidecar` archive file
    pub fn pack_bundle(bundle: &SidecarBundle, output_path: &Path) -> Result<(), Error> {
        bundle.export_to_file(output_path)
    }

    /// Unpack a `.sidecar` archive file into a SidecarBundle
    pub fn unpack_bundle(input_path: &Path) -> Result<SidecarBundle, Error> {
        SidecarBundle::import_from_file(input_path)
    }

    /// 1-Click Install a `.sidecar` bundle into a target directory (e.g. `plugins/` or `sidecars/`)
    pub fn install_bundle(bundle: &SidecarBundle, target_dir: &Path) -> Result<InstalledPackageInfo, Error> {
        if !target_dir.exists() {
            fs::create_dir_all(target_dir)?;
        }

        // 1. Write Manifest JSON file
        let manifest_path = target_dir.join(format!("{}.json", bundle.manifest.id));
        let manifest_json = serde_json::to_string_pretty(&bundle.manifest)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        fs::write(&manifest_path, manifest_json)?;

        // 2. Write Binary Executable / WASM file
        let binary_path = target_dir.join(&bundle.manifest.binary_filename);
        fs::write(&binary_path, &bundle.binary_bytes)?;

        // On Unix platforms, set executable permissions (rwxr-xr-x)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary_path, perms)?;
        }

        // 3. Write Parameter Metadata file (if present)
        let parameter_metadata_path = if let Some(ref param_str) = bundle.parameter_metadata {
            let fname = bundle
                .manifest
                .parameter_metadata_filename
                .clone()
                .unwrap_or_else(|| format!("{}_params.yaml", bundle.manifest.id));
            let p_path = target_dir.join(fname);
            fs::write(&p_path, param_str)?;
            Some(p_path)
        } else {
            None
        };

        // 4. Write Preview Thumbnail file (if present)
        let thumbnail_path = if let Some(ref thumb_bytes) = bundle.thumbnail_bytes {
            let fname = bundle
                .manifest
                .thumbnail_filename
                .clone()
                .unwrap_or_else(|| format!("{}_thumb.png", bundle.manifest.id));
            let t_path = target_dir.join(fname);
            fs::write(&t_path, thumb_bytes)?;
            Some(t_path)
        } else {
            None
        };

        Ok(InstalledPackageInfo {
            target_directory: target_dir.to_path_buf(),
            manifest_path,
            binary_path,
            parameter_metadata_path,
            thumbnail_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_manifest() -> SidecarPackageManifest {
        SidecarPackageManifest {
            id: "test-sidecar-insert".to_string(),
            name: "Test Sidecar Insert".to_string(),
            version: "1.2.3".to_string(),
            author: "Nullherz Core".to_string(),
            sidecar_type: SidecarType::VisualInsert,
            tags: vec!["visual".to_string(), "insert".to_string(), "real-time".to_string()],
            description: "Unit test package bundle".to_string(),
            latency_samples: 0,
            binary_filename: "test_sidecar_bin".to_string(),
            thumbnail_filename: Some("thumb.png".to_string()),
            parameter_metadata_filename: Some("params.yaml".to_string()),
        }
    }

    #[test]
    fn test_bundle_export_and_import_roundtrip() {
        let manifest = sample_manifest();
        let binary_bytes = vec![0x7F, 0x45, 0x4C, 0x46, 0x01, 0x02, 0x03, 0x04]; // ELF magic dummy
        let param_meta = "cutoff: 1000.0\nresonance: 0.707\n".to_string();
        let thumb_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG magic dummy

        let bundle = SidecarBundle::new(
            manifest.clone(),
            binary_bytes.clone(),
            Some(param_meta.clone()),
            Some(thumb_bytes.clone()),
        );

        let exported = bundle.export_bytes().expect("Exporting bundle bytes must succeed");
        assert!(exported.len() > 12);
        assert_eq!(&exported[0..8], SIDECAR_MAGIC_HEADER);

        let imported = SidecarBundle::import_bytes(&exported).expect("Importing bundle bytes must succeed");

        assert_eq!(imported.manifest, manifest);
        assert_eq!(imported.binary_bytes, binary_bytes);
        assert_eq!(imported.parameter_metadata, Some(param_meta));
        assert_eq!(imported.thumbnail_bytes, Some(thumb_bytes));
    }

    #[test]
    fn test_one_click_installation() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("nullherz-pkg-test-{}", nonce));

        let manifest = sample_manifest();
        let binary_bytes = b"#!/bin/sh\necho Hello Sidecar\n".to_vec();
        let param_meta = "mode: 1\ngain: 0.8\n".to_string();
        let thumb_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG magic dummy

        let bundle = SidecarBundle::new(
            manifest.clone(),
            binary_bytes.clone(),
            Some(param_meta.clone()),
            Some(thumb_bytes.clone()),
        );

        let bundle_file = temp_dir.join("test_sidecar.sidecar");
        fs::create_dir_all(&temp_dir).unwrap();

        SidecarPackageManager::pack_bundle(&bundle, &bundle_file).expect("Packing bundle file must succeed");
        assert!(bundle_file.exists());

        let unpacked = SidecarPackageManager::unpack_bundle(&bundle_file).expect("Unpacking bundle file must succeed");
        assert_eq!(unpacked, bundle);

        let install_dir = temp_dir.join("installed_plugins");
        let installed_info = SidecarPackageManager::install_bundle(&unpacked, &install_dir)
            .expect("1-click installation must succeed");

        assert!(installed_info.manifest_path.exists());
        assert!(installed_info.binary_path.exists());
        assert!(installed_info.parameter_metadata_path.as_ref().unwrap().exists());
        assert!(installed_info.thumbnail_path.as_ref().unwrap().exists());

        // Verify content
        let read_bin = fs::read(&installed_info.binary_path).unwrap();
        assert_eq!(read_bin, binary_bytes);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::metadata(&installed_info.binary_path).unwrap().permissions();
            assert_ne!(perms.mode() & 0o111, 0, "Installed binary must have executable permissions");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_invalid_and_corrupted_bundles() {
        // Mismatched magic header
        let mut bad_header = Vec::new();
        bad_header.extend_from_slice(b"BADMAGIC");
        bad_header.extend_from_slice(&1u32.to_le_bytes());
        assert!(SidecarBundle::import_bytes(&bad_header).is_err());

        // Header too short
        assert!(SidecarBundle::import_bytes(b"SHORT").is_err());

        // Unsupported version
        let mut bad_ver = Vec::new();
        bad_ver.extend_from_slice(SIDECAR_MAGIC_HEADER);
        bad_ver.extend_from_slice(&999u32.to_le_bytes());
        assert!(SidecarBundle::import_bytes(&bad_ver).is_err());
    }
}
