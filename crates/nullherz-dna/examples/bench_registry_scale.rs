//! What the sample registry costs as a library grows.
//!
//! Every fixture in the tree except `bench_studio_scale` measures the 4-deck
//! console, where the registry holds a handful of samples and its cost is
//! invisible. `AGENTS.md` §4 is explicit that numbers from one size do not
//! transfer to another, so this measures the two operations whose cost is a
//! function of how many samples are resident:
//!
//!   1. `register_with_metadata` — copy-on-write, so it clones the WHOLE map
//!      per insert. Scanning a library is therefore quadratic in its size.
//!   2. the residency walk the conductor runs twice a second on its tick
//!      (`list_ids()` then `get()` per id), which is linear in the same N and
//!      also holds the reader count up, deferring `drain_garbage`.
//!
//! Buffers are deliberately tiny: the cost under test is map cloning and `Arc`
//! traffic, not audio bytes.
//!
//! Run: cargo run --release -p nullherz-dna --example bench_registry_scale

use nullherz_traits::SampleRegistry as _;
use std::sync::Arc;
use std::time::Instant;

fn metadata() -> Arc<nullherz_traits::SampleMetadata> {
    Arc::new(nullherz_traits::SampleMetadata::new_empty())
}

fn main() {
    println!("registry scale — cost as a function of resident sample count\n");
    println!(
        "{:>7}  {:>14}  {:>13}  {:>14}  {:>12}  {:>13}  {:>8}",
        "N", "scan total", "per register", "walk (old)", "accounted", "single get", "garbage"
    );
    println!("{}", "-".repeat(94));

    for n in [100usize, 500, 1_000, 2_000, 4_000, 8_000, 16_000] {
        let registry = nullherz_dna::SampleRegistry::new();
        let buffer: nullherz_dna::SampleBuffer = Arc::new(vec![0.0f32; 16]).into();
        let meta = metadata();

        // (1) The scan path: register N samples one at a time, as the folder
        // monitor does as it decodes each file.
        let t = Instant::now();
        for id in 0..n as u64 {
            registry.register_with_metadata(id, buffer.clone(), meta.clone());
        }
        let scan = t.elapsed();

        // Retired maps still pending reclamation at the end of the scan. Each
        // one holds an `Arc` clone of every sample it contained.
        let garbage = registry.garbage_len();

        // (2a) The walk `refresh_residency` USED to perform, kept as the
        // baseline the accounted version has to beat.
        let t = Instant::now();
        let ids = registry.list_ids();
        let mut bytes = 0u64;
        for id in &ids {
            if let Some(s) = registry.get(*id) {
                bytes += (s.buffer.len() * std::mem::size_of::<f32>()) as u64;
            }
        }
        let walk = t.elapsed();
        assert_eq!(ids.len(), n, "registry lost samples");
        assert!(bytes > 0);

        // (2b) What the tick reads now: two relaxed atomic loads. Asserted
        // equal to the walk so this cannot drift into measuring nothing.
        let t = Instant::now();
        let reps = 10_000;
        for _ in 0..reps {
            std::hint::black_box(registry.residency());
        }
        let accounted = t.elapsed() / reps;
        let r = registry.residency();
        assert_eq!(
            (r.count as usize, r.bytes), (ids.len(), bytes),
            "accounted residency disagrees with the walk"
        );

        // (3) One lookup, which is what the AUDIO THREAD does for
        // `AddSourceFromRegistry`. Must stay flat in N.
        let probe_id = (n / 2) as u64;
        let t = Instant::now();
        let reps = 10_000;
        for _ in 0..reps {
            std::hint::black_box(registry.get(probe_id));
        }
        let per_get = t.elapsed() / reps;

        println!(
            "{:>7}  {:>14?}  {:>13?}  {:>14?}  {:>12?}  {:>13?}  {:>8}",
            n,
            scan,
            scan / n as u32,
            walk,
            accounted,
            per_get,
            garbage
        );
    }

    println!("\nReading it:");
    println!("  * 'per register' rising with N is the copy-on-write clone: scanning a");
    println!("    library of N tracks costs O(N^2) in total, not O(N).");
    println!("  * 'walk (old)' is what used to run on the conductor tick twice a second;");
    println!("    'accounted' is what replaced it, and must stay flat in N.");
    println!("  * 'single get' is the audio thread's path and must stay flat.");
    println!("  * 'garbage' is retired maps awaiting reclamation, each holding an Arc");
    println!("    clone of every sample in it.");
}
