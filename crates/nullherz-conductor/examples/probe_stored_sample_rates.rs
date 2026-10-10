//! Does the library's stored `sample_rate` match what is actually in the file?
//!
//! `SamplerVoice::source_rate_ratio` converts playback by
//! `metadata.sample_rate / device_rate`. If the stored rate is wrong, the
//! conversion is wrong, and it is wrong only for files whose true rate differs
//! from the device's — which presents as "the tracks that don't match the new
//! setting pitch", with everything else sounding correct.
//!
//! `SampleMetadata::sample_rate` defaults to `LEGACY_SOURCE_SAMPLE_RATE`
//! (44100) on deserialization, commented "Pre-`sample_rate` library rows were
//! all implicitly 44.1 kHz" — so a row written before that field existed claims
//! 44.1 kHz whatever the file is.
//!
//! Run: cargo run --release -p nullherz-conductor --example probe_stored_sample_rates [library.redb]

use nullherz_dna::GeneticLibrary;

/// The file's real rate, from the container header. No decode.
fn true_rate(path: &str) -> Option<u32> {
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &Default::default(), &Default::default())
        .ok()?;
    probed
        .format
        .tracks()
        .iter()
        .find_map(|t| t.codec_params.sample_rate)
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "storage/db/library.redb".to_string());
    let lib = match nullherz_dna::LibraryDatabase::load(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("could not open {path}: {e}");
            std::process::exit(1);
        }
    };
    let tracks = lib.list_tracks().unwrap_or_default();
    println!("library: {path}  ({} tracks)\n", tracks.len());
    if tracks.is_empty() {
        println!("nothing stored — the selective-pitch question cannot be answered from this library.");
        return;
    }

    println!("{:<44} {:>10} {:>10}  verdict", "file", "stored", "actual");
    println!("{}", "-".repeat(84));

    let (mut agree, mut disagree, mut missing) = (0, 0, 0);
    for t in &tracks {
        let name: String = std::path::Path::new(&t.path)
            .file_name()
            .map(|n| n.to_string_lossy().chars().take(42).collect())
            .unwrap_or_else(|| t.path.chars().take(42).collect());
        let stored = t.metadata.sample_rate;
        match true_rate(&t.path) {
            None => {
                missing += 1;
                let dash = "-";
                println!("{name:<44} {stored:>10} {dash:>10}  file unreadable (moved or deleted)");
            }
            Some(actual) if actual == stored => {
                agree += 1;
                println!("{name:<44} {stored:>10} {actual:>10}  ok");
            }
            Some(actual) => {
                disagree += 1;
                let cents = 1200.0 * (stored as f64 / actual as f64).log2();
                println!(
                    "{name:<44} {stored:>10} {actual:>10}  WRONG — would play {cents:+.0} cents off \
                     on a {actual} Hz device"
                );
            }
        }
    }

    println!("\n{agree} correct, {disagree} wrong, {missing} unreadable");
    if disagree > 0 {
        println!(
            "\nSELECTIVE pitch error is REAL in this library: {disagree} track(s) carry a stored rate\n\
             that disagrees with the file. Those play transposed by the ratio between the two,\n\
             while tracks whose stored rate is correct play fine — which is exactly the\n\
             'only the mismatched ones pitch' symptom."
        );
    } else {
        println!(
            "\nNo stored-rate disagreement in this library, so the SELECTIVE mechanism is not\n\
             present here. Any remaining pitch error would be the uniform kind."
        );
    }
}
