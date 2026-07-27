//! Durations must be durations, not frame counts that happen to be right at
//! 44.1 kHz.
//!
//! Every one of these would have passed on the old code *only because* the
//! engine's default rate and the literal `44100` scattered through it agreed.
//! That agreement is what task 1.8 breaks on purpose, so each test here drives
//! a rate the default is not, which is the only way the class of bug becomes
//! visible. A test that runs at the default rate cannot distinguish "derived
//! from the session rate" from "hardcoded to the session rate".

use nullherz_conductor::bounce::OfflineRenderer;
use nullherz_conductor::orchestrator::Conductor;

/// Drive the engine at `rate`, the same way the ALSA backend does once the
/// device has negotiated: `set_config` through the shared handle.
fn conductor_at(rate: f32) -> Conductor {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();
    {
        let lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
        if let Some(engine) = lock.as_ref() {
            // Mirrors crates/nullherz-backends/src/alsa.rs: the backend owns the
            // only mutable path to the engine and reaches it exactly like this.
            let ptr = std::sync::Arc::as_ptr(engine) as *mut dyn nullherz_traits::RenderingEngine;
            unsafe {
                (*ptr).set_config(nullherz_traits::AudioConfig { sample_rate: rate, block_size: 256 });
            }
        }
    }
    conductor
}

/// Sample rate field of a canonical 44-byte WAV header, at byte offset 24.
/// Read from the bytes rather than via a decoder: the point is what a *player*
/// will believe about this file, not what our own code thinks it wrote.
fn wav_header_rate(path: &str) -> u32 {
    let bytes = std::fs::read(path).expect("bounce produced no file");
    assert!(bytes.len() >= 28, "file too short to contain a WAV header");
    assert_eq!(&bytes[0..4], b"RIFF", "not a RIFF file");
    assert_eq!(&bytes[8..12], b"WAVE", "not a WAVE file");
    u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]])
}

#[test]
fn test_bounce_header_states_the_rate_the_engine_rendered_at() {
    // The failure this pins is silent and permanent: render at one rate, stamp
    // another in the header, and every player transposes the export. Nothing in
    // the app ever reports it — the file is simply wrong on disk.
    let rate = 96_000.0;
    let conductor = conductor_at(rate);
    let engine_rate = {
        let lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
        lock.as_ref().map(|e| e.target_sample_rate()).unwrap_or(0.0)
    };
    assert_eq!(
        engine_rate, rate,
        "precondition failed: set_config did not take, so this test cannot \
         distinguish a derived rate from a hardcoded one"
    );

    let dir = std::env::temp_dir().join("nullherz_bounce_rate_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bounce_96k.wav");
    let path_str = path.to_str().unwrap().to_string();

    let mut renderer = OfflineRenderer { conductor };
    renderer.bounce_to_wav(&path_str, 0.05).expect("bounce failed");

    let header_rate = wav_header_rate(&path_str);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        header_rate, rate as u32,
        "engine rendered at {rate} Hz but the WAV header claims {header_rate} Hz — \
         the export is transposed by a factor of {:.4}",
        rate / header_rate as f32
    );
}

#[test]
fn test_bounce_length_is_the_requested_duration_at_any_rate() {
    // The same literal that stamps the header also decides how many frames get
    // rendered. If only one of the two is fixed, the file is the wrong length.
    let rate = 96_000.0;
    let seconds = 0.05_f32;
    let conductor = conductor_at(rate);

    let dir = std::env::temp_dir().join("nullherz_bounce_rate_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bounce_len_96k.wav");
    let path_str = path.to_str().unwrap().to_string();

    let mut renderer = OfflineRenderer { conductor };
    renderer.bounce_to_wav(&path_str, seconds).expect("bounce failed");

    let bytes = std::fs::read(&path_str).unwrap();
    let header_rate = wav_header_rate(&path_str);
    // 32-bit float stereo => 8 bytes per frame; 44-byte canonical header.
    let frames = (bytes.len().saturating_sub(44)) / 8;
    let _ = std::fs::remove_file(&path);

    let expected = (seconds * rate) as usize;
    let tolerance = expected / 20 + 1024; // block granularity
    assert!(
        frames.abs_diff(expected) <= tolerance,
        "asked for {seconds}s at {rate} Hz ({expected} frames) but wrote {frames} frames \
         (header says {header_rate} Hz) — duration is being computed against the wrong rate"
    );
}
