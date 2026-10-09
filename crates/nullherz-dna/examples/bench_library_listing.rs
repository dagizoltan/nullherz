//! What it costs to list a library, with and without the waveform payload.
//!
//! A `LibraryTrack` carries its whole analysis result: peaks, an 8-level MIP
//! pyramid, and five more pyramids for the band/envelope series. `list_tracks`
//! calls `decode_track` on every row, so listing a library deserializes all of
//! that for every track — and the library VIEW reads exactly two fields off
//! `metadata` (`bpm` and `dna`), both of which are already in `TrackFacets`.
//!
//! The facet index exists and is already used for single lookups
//! (`get_track_facets` — "Index hit, no waveform parsed") and for filtering in
//! `query_tracks`. Listing never got it.
//!
//! Run: cargo run --release -p nullherz-dna --example bench_library_listing

use nullherz_dna::GeneticLibrary;
use std::sync::Arc;
use std::time::Instant;

/// Analysis metadata sized the way the worker really produces it: one peak per
/// 128 frames, an 8-level pyramid, and five band/envelope series at the same
/// resolution.
fn analysed_metadata(frames: u64) -> Arc<nullherz_traits::SampleMetadata> {
    let n = (frames / 128).max(1) as usize;
    let mip = |len: usize| {
        let mut levels = Vec::new();
        let mut l = len;
        for _ in 0..8 {
            levels.push(Arc::new(vec![0.25f32; l.max(1)]));
            l = (l / 2).max(1);
        }
        nullherz_traits::MipWaveform { levels }
    };
    let mut m = nullherz_traits::SampleMetadata::new_empty();
    m.bpm = 128.0;
    m.root_key = Some(5.0);
    m.total_samples = frames;
    m.channels = 2;
    m.peaks = Arc::new(vec![0.25f32; n]);
    m.mip_waveform = mip(n);
    m.band_waveform = nullherz_traits::BandWaveform {
        low: mip(n),
        mid: mip(n),
        high: mip(n),
        env_min: mip(n),
        env_max: mip(n),
    };
    Arc::new(m)
}

fn main() {
    // 4-minute tracks: ordinary DJ material, not a worst case.
    const FRAMES: u64 = 4 * 60 * 44_100;

    println!("library listing — full rows vs the facet index\n");
    println!(
        "{:>7}  {:>14}  {:>16}  {:>14}  {:>9}",
        "tracks", "list_tracks", "x2 (default UI)", "facets (1 id)", "ratio"
    );
    println!("{}", "-".repeat(70));

    for n in [25usize, 50, 100, 200] {
        let lib = nullherz_dna::LibraryDatabase::load(":memory:").expect("transient library");
        for id in 0..n as u64 {
            lib.save_track(&nullherz_dna::LibraryTrack {
                id,
                path: format!("synthetic://{id}"),
                title: format!("track_{id}"),
                artist: "bench".into(),
                album: "bench".into(),
                genre: "test".into(),
                energy_level: 0.5,
                metadata: analysed_metadata(FRAMES),
                stems: None,
            })
            .expect("save");
        }

        let t = Instant::now();
        let tracks = lib.list_tracks().expect("list");
        let listing = t.elapsed();
        assert_eq!(tracks.len(), n);

        // One facet lookup, for scale: this is the shape the list view needs.
        let t = Instant::now();
        let f = lib.get_track_facets((n / 2) as u64).expect("facets");
        let facet = t.elapsed();
        assert!(f.is_some());

        println!(
            "{:>7}  {:>14?}  {:>16?}  {:>14?}  {:>8.0}x",
            n,
            listing,
            listing * 2,
            facet,
            listing.as_secs_f64() / facet.as_secs_f64().max(1e-9)
        );
    }

    println!("\nReading it:");
    println!("  * 'list_tracks' deserializes peaks + 6 MIP pyramids per track.");
    println!("  * 'x2' is what `trigger_library_refresh` actually costs: with no");
    println!("    crate selected (the default) it runs the same query twice, once");
    println!("    for crate_tracks and once for all_tracks.");
    println!("  * the view reads only metadata.bpm and metadata.dna off those rows,");
    println!("    and both are already in TrackFacets.");
}
