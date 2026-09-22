//! What the row format costs, on a real library.
//!
//!   cargo run --release -p nullherz-dna --example row_format_bench -- <path.redb>
//!
//! Read-only on the database it is pointed at. Measures the encode/decode of
//! real rows both ways; it does not rewrite anything.
use nullherz_dna::{GeneticLibrary, LibraryDatabase};
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "library.redb".into());
    let db = match LibraryDatabase::load(&path) {
        Ok(d) => d,
        Err(e) => { eprintln!("cannot open {path}: {e}"); std::process::exit(1); }
    };

    let ids: Vec<u64> = match db.list_tracks() {
        Ok(t) => t.into_iter().map(|t| t.id).take(60).collect(),
        Err(e) => { eprintln!("list_tracks: {e}"); std::process::exit(1); }
    };
    if ids.is_empty() { println!("no tracks in {path}"); return; }
    println!("{} tracks sampled from {path}\n", ids.len());

    let mut json_bytes = 0usize;
    let mut rkyv_bytes = 0usize;
    let mut json_decode = std::time::Duration::ZERO;
    let mut rkyv_decode = std::time::Duration::ZERO;
    let mut n = 0usize;

    for id in &ids {
        let Ok(Some(track)) = db.get_track(*id) else { continue };

        let j = serde_json::to_vec(&track).expect("json encode");
        let r = nullherz_dna::library::encode_track_for_test(&track).expect("rkyv encode");
        json_bytes += j.len();
        rkyv_bytes += r.len();

        // Decode each 20 times so a single row's cost is above timer noise.
        let t = Instant::now();
        for _ in 0..20 { std::hint::black_box(nullherz_dna::library::decode_track_for_test(&j).is_ok()); }
        json_decode += t.elapsed();

        let t = Instant::now();
        for _ in 0..20 { std::hint::black_box(nullherz_dna::library::decode_track_for_test(&r).is_ok()); }
        rkyv_decode += t.elapsed();
        n += 1;
    }
    if n == 0 { println!("no rows decoded"); return; }

    let jb = json_bytes as f64 / n as f64;
    let rb = rkyv_bytes as f64 / n as f64;
    let jd = json_decode.as_secs_f64() * 1e3 / (n * 20) as f64;
    let rd = rkyv_decode.as_secs_f64() * 1e3 / (n * 20) as f64;

    println!("  per row, mean over {n} real tracks:");
    println!("    size    json {:>9.1} KB   rkyv {:>9.1} KB   {:.2}x smaller", jb/1024.0, rb/1024.0, jb/rb);
    println!("    decode  json {:>9.3} ms   rkyv {:>9.3} ms   {:.1}x faster", jd, rd, jd/rd);
    println!();
    println!("  extrapolated over the whole table:");
    println!("    a 449 MB library becomes about {:.0} MB", 449.0 * rb / jb);
}
