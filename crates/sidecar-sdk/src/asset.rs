pub use nullherz_dna::{AssetKind, AssetManifest, AssetDatabase};
use crate::package::SidecarPackageManifest;
use crate::store::SidecarType;

pub fn asset_kind_to_sidecar_type(kind: &AssetKind) -> SidecarType {
    match kind {
        AssetKind::AudioTrack | AssetKind::AudioSample | AssetKind::MusicalSequence => SidecarType::AudioInstrument,
        AssetKind::AudioInstrument => SidecarType::AudioInstrument,
        AssetKind::AudioInsert => SidecarType::AudioInsert,
        AssetKind::VisualGenerator => SidecarType::VisualGenerator,
        AssetKind::VisualInsert => SidecarType::VisualInsert,
    }
}

pub fn sidecar_type_to_asset_kind(st: SidecarType) -> AssetKind {
    match st {
        SidecarType::AudioInstrument | SidecarType::Instrument => AssetKind::AudioInstrument,
        SidecarType::AudioInsert | SidecarType::Insert => AssetKind::AudioInsert,
        SidecarType::VisualGenerator => AssetKind::VisualGenerator,
        SidecarType::VisualInsert => AssetKind::VisualInsert,
        SidecarType::NeuralProcessor | SidecarType::NeuralAnalyzer => AssetKind::AudioInsert,
    }
}

pub fn asset_manifest_from_pkg(pkg: &SidecarPackageManifest) -> AssetManifest {
    AssetManifest {
        asset_id: pkg.id.clone(),
        name: pkg.name.clone(),
        kind: sidecar_type_to_asset_kind(pkg.sidecar_type),
        version: pkg.version.clone(),
        author: pkg.author.clone(),
        is_free: true,
        download_size_bytes: 0,
        tags: pkg.tags.clone(),
        description: pkg.description.clone(),
        signature: None,
        binary_filename: Some(pkg.binary_filename.clone()),
    }
}

pub fn pkg_from_asset_manifest(asset: &AssetManifest) -> SidecarPackageManifest {
    SidecarPackageManifest {
        id: asset.asset_id.clone(),
        name: asset.name.clone(),
        version: asset.version.clone(),
        author: asset.author.clone(),
        sidecar_type: asset_kind_to_sidecar_type(&asset.kind),
        tags: asset.tags.clone(),
        description: asset.description.clone(),
        latency_samples: 0,
        binary_filename: asset.binary_filename.clone().unwrap_or_else(|| format!("{}_bin", asset.asset_id)),
        thumbnail_filename: None,
        parameter_metadata_filename: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_manifest_conversions() {
        let manifest = AssetManifest::new(
            "test-synth",
            "Test Synth",
            AssetKind::AudioInstrument,
            "1.0.0",
            "Nullherz Core",
        );
        let pkg = pkg_from_asset_manifest(&manifest);
        assert_eq!(pkg.id, "test-synth");
        assert_eq!(pkg.sidecar_type, SidecarType::AudioInstrument);

        let roundtrip = asset_manifest_from_pkg(&pkg);
        assert_eq!(roundtrip.asset_id, manifest.asset_id);
        assert_eq!(roundtrip.kind, manifest.kind);
    }
}
