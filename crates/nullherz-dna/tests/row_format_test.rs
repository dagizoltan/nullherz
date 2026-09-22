//! The rkyv row format must round-trip exactly, and must still read the JSON
//! rows every existing library is full of.
use nullherz_dna::{GeneticLibrary, LibraryDatabase, LibraryTrack};
use std::sync::Arc;

fn sample_track(id: u64) -> LibraryTrack {
    let mut md = nullherz_traits::SampleMetadata::new_empty();
    md.bpm = 128.5;
    md.sample_rate = 48_000;
    md.channels = 2;
    md.total_samples = 9_123_456;
    md.root_key = Some(7.0);
    for (i, v) in md.dna.spectral.latent_space.iter_mut().enumerate() {
        *v = i as f32 / 16.0;
    }
    md.dna.rhythmic.syncopation_index = 0.625;
    LibraryTrack {
        id,
        path: format!("/music/track_{id}.flac"),
        title: "Ünïcode Títle — em dash".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        genre: "Techno".into(),
        energy_level: 0.8125,
        metadata: Arc::new(md),
    }
}

#[test]
fn rkyv_rows_round_trip_through_the_database() {
    let db = LibraryDatabase::load(":memory:").expect("in-memory db");
    let original = sample_track(4242);
    db.save_track(&original).expect("save");

    let read = db.get_track(4242).expect("get").expect("present");
    assert_eq!(read.id, original.id);
    assert_eq!(read.path, original.path);
    assert_eq!(read.title, original.title, "non-ASCII title did not survive");
    assert_eq!(read.genre, original.genre);
    assert_eq!(read.energy_level, original.energy_level);
    assert_eq!(read.metadata.bpm, original.metadata.bpm);
    assert_eq!(read.metadata.sample_rate, original.metadata.sample_rate);
    assert_eq!(read.metadata.channels, original.metadata.channels);
    assert_eq!(read.metadata.total_samples, original.metadata.total_samples);
    assert_eq!(read.metadata.root_key, original.metadata.root_key);
    assert_eq!(
        read.metadata.dna.spectral.latent_space,
        original.metadata.dna.spectral.latent_space,
        "the 16-D latent space is what every match query reads"
    );
    assert_eq!(
        read.metadata.dna.rhythmic.syncopation_index,
        original.metadata.dna.rhythmic.syncopation_index
    );
}

/// A library written before the format change must keep working. Rows are
/// converted as they are saved, not rewritten on startup — a 470 MB migration
/// behind the user's back is not something to do on open.
#[test]
fn legacy_json_rows_are_still_readable() {
    let track = sample_track(77);
    let json = serde_json::to_vec(&track).expect("legacy encode");
    assert_eq!(json[0], b'{', "legacy rows are JSON objects");

    let decoded = nullherz_dna::library::decode_track_for_test(&json).expect("legacy decode");
    assert_eq!(decoded.id, track.id);
    assert_eq!(decoded.title, track.title);
    assert_eq!(decoded.metadata.bpm, track.metadata.bpm);
}

/// A truncated or corrupt row must be an error, never a wild read. This is what
/// `#[archive(check_bytes)]` buys, and it is only worth having if it is checked.
#[test]
fn corrupt_rkyv_rows_are_rejected() {
    let track = sample_track(9);
    let good = nullherz_dna::library::encode_track_for_test(&track).expect("encode");
    assert_eq!(&good[..8], b"NHZTRK01", "rows carry the format tag");

    for cut in [9usize, good.len() / 2, good.len() - 1] {
        assert!(
            nullherz_dna::library::decode_track_for_test(&good[..cut]).is_err(),
            "a row truncated to {cut} bytes was accepted"
        );
    }
    let mut mangled = good.clone();
    let n = mangled.len();
    mangled[n - 4..].copy_from_slice(&[0xFF; 4]);
    // Either an error or a value — never a crash. The point is it does not read
    // wild memory; check_bytes may still accept a payload whose bytes happen to
    // remain valid.
    let _ = nullherz_dna::library::decode_track_for_test(&mangled);
}

/// A row must decode from ANY base address, not just a lucky one.
///
/// This is the regression test for the bug that shipped in the first draft of
/// this format: an rkyv archive is read in place through pointers derived from
/// the buffer's base, so the base must be aligned — and redb returns a `&[u8]`
/// at whatever address its page cache chose, with an 8-byte magic prefix
/// shifting it further. Validation then succeeded or failed depending on where
/// the allocator happened to put the row. It passed every isolated test here and
/// failed on the second track written by `conductor::mixing_test`.
///
/// Decoding from every byte offset in a word forces the case that luck hid.
#[test]
fn rows_decode_from_any_alignment() {
    let track = sample_track(31337);
    let encoded = nullherz_dna::library::encode_track_for_test(&track).expect("encode");

    for shift in 0..16usize {
        // Place the row at a deliberately odd offset inside a larger buffer.
        let mut padded = vec![0u8; shift];
        padded.extend_from_slice(&encoded);
        let at_offset = &padded[shift..];
        assert_eq!(
            at_offset.as_ptr() as usize % 16,
            (encoded.as_ptr() as usize + shift) % 16 % 16,
            "test setup did not actually move the base address"
        );

        let decoded = nullherz_dna::library::decode_track_for_test(at_offset)
            .unwrap_or_else(|e| panic!("row at +{shift} failed to decode: {e}"));
        assert_eq!(decoded.id, track.id, "row at +{shift} decoded wrong");
        assert_eq!(decoded.title, track.title, "row at +{shift} decoded wrong");
        assert_eq!(decoded.metadata.bpm, track.metadata.bpm, "row at +{shift} decoded wrong");
    }
}
