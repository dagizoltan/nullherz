// Library query benchmark — measures query_tracks / suggest_matches /
// get_smart_crate_tracks against a populated redb, to A/B the facet-index change.
//   cargo run --release -p nullherz-dna --example bench_library_query
// Env: N=<tracks> ITERS=<calls/op>. Each track carries ~50k f32 peaks so the
// OLD full-scan path pays realistic per-track waveform deserialization.
use std::sync::Arc;
use std::time::Instant;
use nullherz_dna::{LibraryDatabase, LibraryTrack, GeneticLibrary, SmartCrateDefinition};

fn make_track(id: u64) -> LibraryTrack {
    let mut m = nullherz_traits::SampleMetadata::new_empty();
    m.bpm = 120.0 + (id % 60) as f32;
    m.root_key = Some((id % 12) as f32);
    m.peaks = Arc::new(vec![0.1f32; 50_000]); // simulate real waveform bulk
    m.dna.feature_vector[0] = (id % 100) as f32 / 100.0;
    let genre = match id % 3 { 0 => "techno", 1 => "house", _ => "dnb" };
    LibraryTrack {
        id,
        path: format!("/lib/track_{id}.wav"),
        title: format!("Track {id}"),
        artist: format!("Artist {}", id % 50),
        album: "Album".into(),
        genre: genre.into(),
        energy_level: (id % 10) as f32 / 10.0,
        metadata: Arc::new(m),
    }
}

fn main() {
    let n: u64 = std::env::var("N").ok().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let iters: u32 = std::env::var("ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);

    let tmp = std::env::temp_dir().join(format!("nh_libbench_{}.redb", std::process::id()));
    let _ = std::fs::remove_file(&tmp);

    let db = LibraryDatabase::load(tmp.to_str().unwrap()).unwrap();
    for id in 0..n { db.save_track(&make_track(id)).unwrap(); }
    drop(db);

    // Reload from disk so the (new) index is built the startup way and both
    // builds measure a cold process.
    let load_start = Instant::now();
    let db = LibraryDatabase::load(tmp.to_str().unwrap()).unwrap();
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    // The first query builds the facet index lazily — time that one-time cost on
    // its own, then all measured loops below reflect the warmed steady state.
    let target = make_track(0);
    let build_start = Instant::now();
    let _ = db.suggest_matches(&target.metadata.dna, 1).unwrap();
    let build_ms = build_start.elapsed().as_secs_f64() * 1000.0;

    let mut sink = 0usize;

    let t = Instant::now();
    for _ in 0..iters { sink += db.query_tracks(Some("techno"), None, None, None).unwrap().len(); }
    let q_us = t.elapsed().as_secs_f64() * 1e6 / iters as f64;

    let t = Instant::now();
    for _ in 0..iters { sink += db.suggest_matches(&target.metadata.dna, 20).unwrap().len(); }
    let s_us = t.elapsed().as_secs_f64() * 1e6 / iters as f64;

    let def = SmartCrateDefinition {
        name: "bench".into(), target_dna: None, threshold: 0.0,
        spectral_tilt_range: None, rhythmic_syncopation_range: None, glitch_density_range: None,
        genre: Some("house".into()), bpm_range: Some((120.0, 140.0)),
        energy_range: Some((0.2, 0.8)), root_key: None,
    };
    db.save_smart_crate(&def).unwrap();
    let t = Instant::now();
    for _ in 0..iters { sink += db.get_smart_crate_tracks("bench").unwrap().len(); }
    let c_us = t.elapsed().as_secs_f64() * 1e6 / iters as f64;

    println!("N={n} tracks (~50k f32 peaks each), {iters} iters/op");
    println!("load                : {load_ms:.1} ms  (lazy — index NOT built here)");
    println!("first query (builds): {build_ms:.1} ms  (one-time index build)");
    println!("query_tracks        : {q_us:.1} us/call  (warmed)");
    println!("suggest_matches     : {s_us:.1} us/call");
    println!("get_smart_crate     : {c_us:.1} us/call");
    println!("(sink {sink})");
    let _ = std::fs::remove_file(&tmp);
}
