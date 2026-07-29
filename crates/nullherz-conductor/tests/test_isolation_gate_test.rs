//! Tests must not touch the developer's real library.
//!
//! `Conductor::new()` opens `library.redb` in the working directory — on a
//! developer machine that is a 233 MB library of real tracks. Five test call
//! sites did exactly that.
//!
//! It did not fail loudly, which is the point. redb holds an EXCLUSIVE file
//! lock, and `with_library_path` catches the resulting error and hands back a
//! private fallback database. So under `cargo test --workspace`, where cargo
//! runs test binaries in parallel, the outcome was a race: the first opener got
//! the real library, everyone after got an empty one. The same test saw two
//! different worlds depending on scheduling — and the losers left
//! `fallback_*.redb` files behind (25 of them had accumulated in the crate
//! directory).
//!
//! That is a flake generator and a correctness hazard at once: a test asserting
//! anything about library contents passes or fails on who won a lock, and a test
//! that WINS is mutating real user data.
//!
//! Tests get `":memory:"`. This gate keeps it that way.

use std::path::Path;

/// Constructors that open the real `library.redb`, directly or transitively.
///
/// `OfflineRenderer::new` is on this list because scanning for `Conductor::new`
/// alone was NOT enough: the offline-renderer test called a production
/// constructor that opened the library one level down, and quietly wrote a
/// 3.6 MB database into the crate directory on every run while this gate passed.
/// A constructor belongs here the moment it reaches `Conductor::new()` by any
/// path — the gate can only see names, so the list is the part a human maintains.
const REAL_LIBRARY_CONSTRUCTORS: [&str; 2] = ["Conductor::new()", "OfflineRenderer::new("];

/// Files allowed to use them — production entry points, where opening the real
/// library is the whole intent. Their `#[cfg(test)]` modules are still checked.
const PRODUCTION_ENTRY_POINTS: [&str; 5] =
    ["main.rs", "survival.rs", "bounce.rs", "lib.rs", "command_handler.rs"];

fn scan_for_default_conductor(dir: &Path, hits: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_for_default_conductor(&path, hits);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // This file names the offending call in its own failure message.
        if name == "test_isolation_gate_test.rs" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };

        // In `lib.rs` the production API itself is fine; only its `mod tests`
        // is not. Track whether we have crossed into test code.
        let production_ok = PRODUCTION_ENTRY_POINTS.contains(&name.as_str());
        let mut in_tests = false;
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.contains("#[cfg(test)]") {
                in_tests = true;
            }
            if t.starts_with("//") {
                continue;
            }
            let Some(found) = REAL_LIBRARY_CONSTRUCTORS.iter().find(|c| t.contains(**c)) else {
                continue;
            };
            if production_ok && !in_tests {
                continue;
            }
            hits.push(format!("{name}:{}: {t}   [{found}]", i + 1));
        }
    }
}

#[test]
fn test_no_test_opens_the_real_library() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    scan_for_default_conductor(&root.join("src"), &mut hits);
    scan_for_default_conductor(&root.join("tests"), &mut hits);
    hits.sort();

    assert!(
        hits.is_empty(),
        "these tests call `Conductor::new()`, which opens the real `library.redb` \
         in the working directory:\n  {}\n\n\
         Use `Conductor::with_library_path(\":memory:\")`. `new()` does not fail when \
         the file is locked — it silently substitutes a private fallback database, so \
         whether a test sees the developer's library or an empty one depends on which \
         binary won the lock race.",
        hits.join("\n  ")
    );
}

/// The fallback must stay announced.
///
/// The branch is a race with silent, divergent outcomes; a caller that lands in
/// it and is not told has no way to explain the behaviour it then sees. This
/// pins the diagnostic rather than the mechanism — remove the message and the
/// next occurrence is as unfindable as the last one was.
#[test]
fn test_library_fallback_is_announced() {
    let orchestrator = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/orchestrator.rs");
    let text = std::fs::read_to_string(orchestrator).expect("orchestrator.rs must be readable");

    let fallback_block = text
        .split("nullherz_fallback_")
        .next()
        .expect("the fallback path construction must still exist");

    assert!(
        fallback_block.contains("falling back to a private"),
        "the library fallback branch no longer warns. It is a RACE — the first \
         opener gets the real library and the rest get empty ones — so a caller \
         that lands here must be told, or the divergence is invisible."
    );
}
