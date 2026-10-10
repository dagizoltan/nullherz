//! A processor must not compute DSP against a hardcoded sample rate.
//!
//! `ProcessContext` carries the transport, and `SignalProcessor::setup` carries
//! an `AudioConfig`. A processor that ignores both and writes the rate as a
//! literal is tuned for one converter and wrong on every other. This project
//! has shipped that three times:
//!
//!   * `SamplerVoice` advanced one source frame per output frame, so a 48 kHz
//!     file played 8.8% slow on a 44.1 kHz device.
//!   * `AlgorithmicReverbProcessor` used Freeverb's sample-count delays
//!     unscaled, losing 10% of its tail at 48 kHz and 54% at 96 kHz.
//!   * `NeuralFilterProcessor` had `let sample_rate = 48000.0f32;` inside
//!     `process()`, putting a nominal 1 kHz corner at ~1088 Hz on 44.1 kHz.
//!
//! Each was found by reading. This gate is the alternative: it rejects the
//! FORM, so the fourth instance fails the build instead of shipping.
//!
//! What it deliberately does NOT flag:
//!
//!   * `ctx.transport...map(|t| t.sample_rate).unwrap_or(48000.0)` — reads the
//!     live rate and uses the literal only when there is no transport at all,
//!     which is the correct pattern and what the fixed processors now do.
//!   * Rate literals inside `#[cfg(test)]`, where fixing a rate is the point.
//!   * Constructor arguments, where a default before the first transport is
//!     legitimate.
//!
//! It flags exactly one shape: a rate literal bound straight to a local, which
//! is the form all three defects took.

use std::path::{Path, PathBuf};

/// Rates a processor might plausibly hardcode.
const RATE_LITERALS: &[&str] = &[
    "44100", "44_100", "48000", "48_000", "96000", "96_000", "88200", "88_200", "192000", "192_000",
];

/// Files whose rate literals are known and accepted. This list may only SHRINK.
///
/// Empty, deliberately: every instance found when this gate was written has
/// been fixed. An addition here needs a reason in the same commit, because the
/// next reader will take the list as permission.
const ACCEPTED: &[&str] = &[];

fn scan_hardcoded_rates(dir: &Path, out: &mut Vec<(String, usize, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_hardcoded_rates(&path, out);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        // `*_tests.rs` is test code that happens to live outside a cfg(test)
        // block; fixing a rate there is the point of the test.
        if name.ends_with("_tests.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut in_tests = false;
        for (i, line) in text.lines().enumerate() {
            if line.trim().contains("#[cfg(test)]") {
                in_tests = true;
            }
            let trimmed = line.trim();
            if in_tests || trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // The one shape being rejected: a rate literal bound straight to a
            // local. `let sr = 44_100.0;` / `let sample_rate = 48000.0f32;`
            if !trimmed.starts_with("let ") || !trimmed.contains('=') {
                continue;
            }
            let rhs = trimmed.splitn(2, '=').nth(1).unwrap_or("").trim();
            // A transport read is the correct pattern even with a literal
            // fallback, so only a BARE literal counts.
            if rhs.contains("transport") || rhs.contains("config") || rhs.contains("sample_rate()") {
                continue;
            }
            let bare = rhs.trim_end_matches(';').trim();
            let is_rate_literal = RATE_LITERALS.iter().any(|lit| {
                bare == *lit
                    || bare == format!("{lit}.0")
                    || bare == format!("{lit}.0f32")
                    || bare == format!("{lit}.0f64")
                    || bare == format!("{lit}f32")
            });
            if is_rate_literal {
                out.push((name.clone(), i + 1, trimmed.to_string()));
            }
        }
    }
}

#[test]
fn test_no_processor_hardcodes_a_sample_rate() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(src.is_dir(), "processor sources not found at {src:?}");

    let mut found = Vec::new();
    scan_hardcoded_rates(&src, &mut found);
    found.retain(|(file, _, _)| !ACCEPTED.contains(&file.as_str()));

    assert!(
        found.is_empty(),
        "these processors compute against a hardcoded sample rate:\n{}\n\n\
         Read the rate instead — `ctx.transport.map(|t| t.sample_rate)` with a \
         `DEFAULT_SAMPLE_RATE` fallback, or implement `SignalProcessor::setup`. A rate written \
         as a literal is tuned for one converter and wrong on every other; this is the fourth \
         time the project would have shipped that.",
        found
            .iter()
            .map(|(f, l, t)| format!("  {f}:{l}  {t}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

/// The gate must be able to fail, or it is decoration.
#[test]
fn test_the_scanner_recognises_the_form_it_rejects() {
    let dir = std::env::temp_dir().join(format!("nh_rate_gate_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    // The three shapes that must be caught, and the three that must not.
    let sample = r#"
impl SignalProcessor for Thing {
    fn process(&mut self, _i: &[&[f32]], _o: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        let sample_rate = 48000.0f32;
        let sr = 44_100.0;
        let fine = ctx.transport.map(|t| t.sample_rate).unwrap_or(48000.0);
        let also_fine = config.sample_rate;
        let not_a_rate = 256;
    }
}
#[cfg(test)]
mod tests {
    fn t() { let sample_rate = 96000.0; }
}
"#;
    let file = dir.join("thing.rs");
    std::fs::write(&file, sample).expect("write fixture");

    let mut found = Vec::new();
    scan_hardcoded_rates(&dir, &mut found);
    let _ = std::fs::remove_dir_all(&dir);

    let lines: Vec<&str> = found.iter().map(|(_, _, t)| t.as_str()).collect();
    assert_eq!(
        found.len(),
        2,
        "scanner found {} hardcoded rates, expected exactly 2 (the two bare bindings): {lines:?}",
        found.len()
    );
    assert!(lines.iter().any(|l| l.contains("48000.0f32")), "missed the suffixed literal: {lines:?}");
    assert!(lines.iter().any(|l| l.contains("44_100.0")), "missed the underscored literal: {lines:?}");
    assert!(
        !lines.iter().any(|l| l.contains("transport")),
        "flagged a correct transport read: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("96000")),
        "flagged a literal inside #[cfg(test)]: {lines:?}"
    );
}
