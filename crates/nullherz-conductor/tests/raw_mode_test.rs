//! RAW mode: loading a track must not change how it sounds.
//!
//! Every `LoadTrackToDeck` used to fire four pieces of processing that nobody
//! asked for and two of which had no UI affordance at all:
//!
//!   * `Core::SetBpm(track.bpm)` — GLOBAL, so the deck loaded last re-tempoed the
//!     whole console and time-stretched every other playing deck to match it.
//!   * harmonic key-sync toward a master key hardcoded to C — measured at
//!     **−5 semitones** on real material. A DJ tool that silently transposes the
//!     incoming track is not shippable at any level of RT rigor.
//!   * groove transfusion onto the deck's sequencer, displacing step fire times.
//!   * `quantize_enabled = true` on the sampler, which both time-stretched the
//!     deck and gated its output on the global transport running.
//!
//! The SYNC button that appeared to control this sent `SyncDecks`, which the
//! orchestrator matched and then ignored — so the visible control did nothing
//! while the invisible automation did everything.
//!
//! RAW is now the default and both behaviours are per-deck latches. These tests
//! pin the default, because "we removed the automatic pitch shift" is the kind
//! of claim that quietly stops being true.

use nullherz_conductor::Conductor;
use nullherz_conductor::mixer_orchestrator::MixerOrchestrator;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{Command, CoreCommand, MixerCommand, PerformanceCommand};
use std::sync::Arc;

const SR: f32 = 48_000.0;
/// Deliberately not the console's idle tempo, so a stray `SetBpm` moves it.
const TRACK_BPM: f32 = 174.0;
/// D (2 semitones above C). The old auto-sync drove every track toward C, so a
/// non-C key is what makes the unrequested shift observable.
const TRACK_ROOT_KEY: f32 = 2.0;
/// F — the key of the track on the MASTER deck.
///
/// Deliberately NOT C. The bug being guarded is that the master key was
/// hardcoded to C, so a fixture with a C master would produce identical numbers
/// whether the code reads the master deck or ignores it, and would pass against
/// the broken version. With master = F a follower in D shifts +3; under the old
/// hardcoded C it shifted −2.
const MASTER_ROOT_KEY: f32 = 5.0;

fn console() -> Conductor {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();
    conductor
}

/// A library track carrying both a BPM and a root key — the two facets the
/// automatic path consumed.
fn register_track(conductor: &Conductor, id: u64) {
    register_track_with_key(conductor, id, Some(TRACK_ROOT_KEY));
}

/// Put a track on the master deck (A by default) so the master key is knowable.
/// Returns the sample id.
fn register_master_track(conductor: &mut Conductor, id: u64, key: Option<f32>) -> u64 {
    register_track_with_key(conductor, id, key);
    let master = conductor.mixer_manager.active_master_deck;
    conductor.mixer_manager.deck_samples.insert(master, id);
    id
}

fn register_track_with_key(conductor: &Conductor, id: u64, root_key: Option<f32>) {
    let frames = (SR * 2.0) as usize;
    let mut samples = Vec::with_capacity(frames * 2);
    for _ in 0..2 {
        for i in 0..frames {
            samples.push((i as f32 * 440.0 * std::f32::consts::TAU / SR).sin() * 0.25);
        }
    }

    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = TRACK_BPM;
    metadata.root_key = root_key;
    metadata.total_samples = frames as u64;
    metadata.channels = 2;
    metadata.sample_rate = SR as u32;
    let metadata = Arc::new(metadata);

    conductor
        .transfusion_manager
        .sample_registry
        .register_with_metadata(id, Arc::new(samples), metadata.clone());

    let lib = conductor.library.lock();
    lib.save_track(&nullherz_dna::LibraryTrack {
        id,
        path: format!("tone://{id}"),
        title: "raw mode fixture".to_string(),
        artist: "test".to_string(),
        album: "test".to_string(),
        genre: "test tone".to_string(),
        energy_level: 0.5,
        metadata,
    })
    .expect("in-memory library save cannot fail");
}

fn translate(conductor: &Conductor, cmd: Command) -> Vec<Command> {
    MixerOrchestrator::translate_command(&cmd, &conductor.mixer_manager, &conductor.library)
}

fn load(conductor: &Conductor, deck_id: char, sample_id: u64) -> Vec<Command> {
    translate(conductor, Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id, sample_id }))
}

fn pitch_slot_node(conductor: &Conductor, deck_id: char) -> u64 {
    conductor.mixer_manager.deck_mappings[&deck_id].pitch_slot_id as u64
}

fn sampler_node(conductor: &Conductor, deck_id: char) -> u64 {
    conductor.mixer_manager.deck_mappings[&deck_id].sampler_id as u64
}

/// Every `SetParam` aimed at `target`/`param`, in emission order.
fn params_for(cmds: &[Command], target: u64, param: u32) -> Vec<f32> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::Mixer(MixerCommand::SetParam { target_id, param_id, value, .. })
                if *target_id == target && *param_id == param => Some(*value),
            _ => None,
        })
        .collect()
}

fn set_bpms(cmds: &[Command]) -> Vec<f32> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::Core(CoreCommand::SetBpm(bpm)) => Some(*bpm),
            _ => None,
        })
        .collect()
}

#[test]
fn test_load_does_not_change_tempo_or_pitch_by_default() {
    let conductor = console();
    register_track(&conductor, 7_001);
    let translated = load(&conductor, 'A', 7_001);

    assert!(
        set_bpms(&translated).is_empty(),
        "a plain load published a transport tempo: {:?}. SetBpm is GLOBAL — this is \
         how loading a deck re-tempoed every other playing deck.",
        set_bpms(&translated)
    );

    let shifts = params_for(&translated, pitch_slot_node(&conductor, 'A'), 0);
    assert!(
        shifts.is_empty(),
        "a plain load pitch-shifted the deck by {:?} semitones. This is the −5 \
         semitone transposition applied to every track, unasked.",
        shifts
    );
}

/// The sampler's own tempo-sync latch must be stated, not assumed.
///
/// Asserting the emitted value (rather than trusting the constructor default)
/// is deliberate: a node rebuilt by a topology change gets a fresh processor,
/// and "the default is false" stops protecting anything the moment some other
/// path sets it.
#[test]
fn test_load_disengages_sampler_quantize_by_default() {
    let conductor = console();
    register_track(&conductor, 7_002);
    let translated = load(&conductor, 'A', 7_002);

    let quantize = params_for(
        &translated,
        sampler_node(&conductor, 'A'),
        nullherz_processors::sampler::PARAM_QUANTIZE,
    );
    assert_eq!(
        quantize,
        vec![0.0],
        "a plain load must state quantize=off exactly once; got {quantize:?}"
    );
}

/// A default-constructed sampler plays its source at the source's own rate.
///
/// The console-level tests above only prove the orchestrator stopped ASKING for
/// a stretch. This proves the processor does not apply one on its own: with
/// quantize engaged the sampler computed `transport.bpm / meta.bpm`, so a
/// 174 BPM track on a 128 BPM transport ran ~26% slow with no command involved.
#[test]
fn test_fresh_sampler_does_not_time_stretch() {
    use nullherz_traits::{AudioProcessor, SignalProcessor};

    let mut sampler = nullherz_processors::sampler::SamplerProcessor::new(0);
    let frames = 4_096;
    let mut samples = Vec::with_capacity(frames);
    for i in 0..frames {
        samples.push((i as f32 * 440.0 * std::f32::consts::TAU / SR).sin() * 0.5);
    }
    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = TRACK_BPM;
    metadata.total_samples = frames as u64;
    metadata.channels = 1;
    metadata.sample_rate = SR as u32;

    sampler.apply_topology_mutation(nullherz_traits::TopologyMutation::AddSource {
        node_idx: 0,
        sample_id: 1,
        buffer: Arc::new(samples),
        metadata: Some(Arc::new(metadata)),
    });

    // Transport tempo deliberately unequal to the track's.
    let transport = nullherz_traits::Transport {
        sample_rate: SR,
        bpm: 128.0,
        is_playing: true,
        beat_position: 0.0,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    };
    assert_ne!(
        transport.bpm, TRACK_BPM,
        "fixture must exercise a tempo MISMATCH or it proves nothing"
    );

    sampler.apply_command(&Command::Performance(PerformanceCommand::PlayNode { node_idx: 0 }));

    let mut out = vec![0.0f32; 256];
    {
        let mut slice: [&mut [f32]; 1] = [&mut out];
        let mut ctx = nullherz_traits::ProcessContext {
            transport: Some(&transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };
        sampler.process(&[], &mut slice, &mut ctx);
    }

    let rate = sampler.voices.iter().find(|v| v.is_active).map(|v| v.playback_rate);
    assert_eq!(
        rate,
        Some(1.0),
        "a fresh sampler time-stretched a {TRACK_BPM} BPM source against a \
         {} BPM transport (rate {:?}). RAW playback must be rate 1.0.",
        transport.bpm,
        rate
    );
}

#[test]
fn test_sync_latch_restores_tempo_matching() {
    let mut conductor = console();
    register_track(&conductor, 7_003);
    conductor.mixer_manager.deck_samples.insert('A', 7_003);
    conductor.mixer_manager.sync_decks.insert('A');

    let translated = load(&conductor, 'A', 7_003);

    assert_eq!(
        set_bpms(&translated),
        vec![TRACK_BPM],
        "SYNC on: the load must publish the track's tempo to the transport"
    );
    assert_eq!(
        params_for(&translated, sampler_node(&conductor, 'A'), nullherz_processors::sampler::PARAM_QUANTIZE),
        vec![1.0],
        "SYNC on: the sampler's tempo-sync latch must be engaged"
    );
}

/// KEY must shift toward the MASTER DECK's key, read from the track on it.
///
/// The fixture's master is in F and the follower in D, so the correct shift is
/// **+3**. The version this replaces resolved the master key to a hardcoded C
/// and would answer −2 — which is the whole point of choosing a non-C master.
#[test]
fn test_key_latch_shifts_toward_the_master_decks_key() {
    let mut conductor = console();
    register_master_track(&mut conductor, 7_100, Some(MASTER_ROOT_KEY));
    register_track(&conductor, 7_004);
    conductor.mixer_manager.deck_samples.insert('B', 7_004);
    conductor.mixer_manager.key_sync_decks.insert('B');

    let translated = load(&conductor, 'B', 7_004);
    let shifts = params_for(&translated, pitch_slot_node(&conductor, 'B'), 0);

    assert_eq!(
        shifts,
        vec![MASTER_ROOT_KEY - TRACK_ROOT_KEY],
        "KEY on: deck in D against a master in F must shift +3, not toward C"
    );
}

/// An unknowable master key means NO shift — never "assume C".
///
/// This is the defect itself. A track that has simply never been key-analysed
/// still got transposed toward C, measured at −5 semitones on real material.
/// "I don't know what key to match" must resolve to playing the file as
/// recorded.
#[test]
fn test_unknown_master_key_does_not_shift() {
    let mut conductor = console();
    // Master deck loaded, but its track has no analysed key.
    register_master_track(&mut conductor, 7_101, None);
    register_track(&conductor, 7_102);
    conductor.mixer_manager.deck_samples.insert('B', 7_102);
    conductor.mixer_manager.key_sync_decks.insert('B');

    let translated = load(&conductor, 'B', 7_102);

    assert_eq!(
        params_for(&translated, pitch_slot_node(&conductor, 'B'), 0),
        vec![0.0],
        "an unanalysed master key must mean no shift; a hardcoded-C fallback \
         would transpose this deck by {}",
        -TRACK_ROOT_KEY
    );
}

/// With no track on the master deck at all, likewise: no shift.
#[test]
fn test_no_master_track_does_not_shift() {
    let mut conductor = console();
    register_track(&conductor, 7_103);
    conductor.mixer_manager.deck_samples.insert('B', 7_103);
    conductor.mixer_manager.key_sync_decks.insert('B');

    let translated = load(&conductor, 'B', 7_103);

    assert_eq!(
        params_for(&translated, pitch_slot_node(&conductor, 'B'), 0),
        vec![0.0],
        "an empty master deck must mean no shift"
    );
}

/// The master deck cannot be harmonically synced to itself.
#[test]
fn test_master_deck_never_shifts_itself() {
    let mut conductor = console();
    register_master_track(&mut conductor, 7_104, Some(MASTER_ROOT_KEY));
    conductor.mixer_manager.key_sync_decks.insert('A');
    assert_eq!(conductor.mixer_manager.active_master_deck, 'A');

    let translated = load(&conductor, 'A', 7_104);

    assert_eq!(
        params_for(&translated, pitch_slot_node(&conductor, 'A'), 0),
        vec![0.0],
        "the master deck defines the key; syncing it to itself must be a no-op"
    );
}

/// Loading a new track on the MASTER deck moves the reference, so every
/// KEY-latched follower must be re-aligned.
///
/// Without this the KEY light stays lit while the deck is matched to a track
/// that is no longer the master — a stale shift that looks intentional.
#[test]
fn test_new_master_track_realigns_latched_decks() {
    let mut conductor = console();
    register_master_track(&mut conductor, 7_105, Some(MASTER_ROOT_KEY));
    register_track(&conductor, 7_106);
    conductor.mixer_manager.deck_samples.insert('B', 7_106);
    conductor.mixer_manager.key_sync_decks.insert('B');

    // A new master track, a semitone up from the old one (F -> F#).
    let new_master_key = MASTER_ROOT_KEY + 1.0;
    register_track_with_key(&conductor, 7_107, Some(new_master_key));
    conductor.mixer_manager.deck_samples.insert('A', 7_107);

    let translated = load(&conductor, 'A', 7_107);

    assert_eq!(
        params_for(&translated, pitch_slot_node(&conductor, 'B'), 0),
        vec![new_master_key - TRACK_ROOT_KEY],
        "loading a new master track left deck B on its old, now-wrong shift"
    );
}

/// Moving master to another deck likewise moves the reference key.
#[test]
fn test_changing_master_deck_realigns_latched_decks() {
    let mut conductor = console();
    register_master_track(&mut conductor, 7_108, Some(MASTER_ROOT_KEY));
    register_track(&conductor, 7_109);
    conductor.mixer_manager.deck_samples.insert('B', 7_109);
    conductor.mixer_manager.key_sync_decks.insert('B');

    // Deck C carries a different key; making it master must re-aim deck B.
    let c_key = 9.0f32;
    register_track_with_key(&conductor, 7_110, Some(c_key));
    conductor.mixer_manager.deck_samples.insert('C', 7_110);

    conductor.apply_mixer_commands(vec![Command::Core(CoreCommand::SetMasterDeck('C'))]);
    assert_eq!(conductor.mixer_manager.active_master_deck, 'C');

    let translated = translate(&conductor, Command::Core(CoreCommand::SetMasterDeck('C')));
    let shift = params_for(&translated, pitch_slot_node(&conductor, 'B'), 0);

    // C(9) - D(2) = 7, which wraps to -5 as the shorter path around the circle.
    let mut expected = c_key - TRACK_ROOT_KEY;
    while expected > 6.0 { expected -= 12.0; }
    assert_eq!(
        shift,
        vec![expected],
        "changing the master deck left deck B aimed at the old master's key"
    );
}

/// Releasing KEY must RE-CENTRE the node, not merely stop updating it.
///
/// Stopping at "the latch is off so we emit nothing" would leave the last shift
/// applied: the light goes out, the button reads RAW, and the deck is still
/// transposed. That is worse than the original bug, because now the UI actively
/// says it isn't happening.
#[test]
fn test_releasing_key_recentres_the_deck() {
    let mut conductor = console();
    register_master_track(&mut conductor, 7_111, Some(MASTER_ROOT_KEY));
    register_track(&conductor, 7_005);
    conductor.mixer_manager.deck_samples.insert('B', 7_005);

    conductor.mixer_manager.key_sync_decks.insert('B');
    let engaged = translate(
        &conductor,
        Command::Performance(PerformanceCommand::SetDeckKeySync { deck_id: 'B', enabled: true }),
    );
    assert_eq!(
        params_for(&engaged, pitch_slot_node(&conductor, 'B'), 0),
        vec![MASTER_ROOT_KEY - TRACK_ROOT_KEY],
        "engaging KEY must shift toward the master deck's key"
    );

    conductor.mixer_manager.key_sync_decks.remove(&'B');
    let released = translate(
        &conductor,
        Command::Performance(PerformanceCommand::SetDeckKeySync { deck_id: 'B', enabled: false }),
    );
    assert_eq!(
        params_for(&released, pitch_slot_node(&conductor, 'B'), 0),
        vec![0.0],
        "releasing KEY left the deck transposed"
    );
}

/// The latch must be updated before translation, not after.
///
/// `translate_command` reads the latch sets to decide whether a load in the SAME
/// batch is raw or synced. If `CommandHandler` applied the latch after
/// translating, pressing SYNC and loading a track together would translate the
/// load against stale state — the button would appear to need pressing twice.
#[test]
fn test_sync_latch_applies_to_a_load_in_the_same_batch() {
    let mut conductor = console();
    register_track(&conductor, 7_006);

    conductor.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::SetDeckSync { deck_id: 'A', enabled: true }),
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: 7_006 }),
    ]);

    assert!(
        conductor.mixer_manager.sync_decks.contains(&'A'),
        "SetDeckSync must latch on the mixer manager"
    );

    // Re-translating the same load now takes the synced path.
    assert_eq!(
        set_bpms(&load(&conductor, 'A', 7_006)),
        vec![TRACK_BPM],
        "the latch set in this batch must govern the load in the same batch"
    );
}

/// RAW must not merely skip the processing — it must skip the library READ.
///
/// The facet lookup sits on the latency-critical command path holding the
/// library mutex. With both latches off there is nothing to read, and a load
/// that still paid for the lookup would keep the cost the roadmap's Phase 0
/// work removed.
#[test]
fn test_raw_load_emits_only_the_source_and_the_tempo_mode() {
    let conductor = console();
    register_track(&conductor, 7_007);
    let translated = load(&conductor, 'A', 7_007);

    let unexpected: Vec<&Command> = translated
        .iter()
        .filter(|c| {
            !matches!(
                c,
                Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry { .. })
            ) && !matches!(
                c,
                Command::Mixer(MixerCommand::SetParam { param_id, .. })
                    if *param_id == nullherz_processors::sampler::PARAM_QUANTIZE
            )
        })
        .collect();

    assert!(
        unexpected.is_empty(),
        "a RAW load must be the source plus its tempo mode and nothing else; \
         also emitted: {unexpected:?}"
    );
}

// ---------------------------------------------------------------------------
// KEY LOCK (master tempo)
// ---------------------------------------------------------------------------

/// Key lock must cancel the pitch shift that tempo sync introduces.
///
/// Tempo sync resamples, so a deck pulled from 174 to 120 BPM runs at rate
/// 120/174 and drops `12·log2(rate)` semitones — over 6 semitones, more than a
/// tritone. Key lock applies the inverse so the track keeps its pitch.
#[test]
fn test_key_lock_cancels_tempo_pitch_shift() {
    let mut conductor = console();
    register_track(&conductor, 7_200); // TRACK_BPM = 174
    conductor.mixer_manager.deck_samples.insert('B', 7_200);
    conductor.mixer_manager.sync_decks.insert('B');
    conductor.mixer_manager.key_lock_decks.insert('B');
    conductor.mixer_manager.transport_bpm = 120.0;

    let translated = translate(
        &conductor,
        Command::Performance(PerformanceCommand::SetDeckKeyLock { deck_id: 'B', enabled: true }),
    );

    let expected = -12.0 * (120.0f32 / TRACK_BPM).log2();
    let got = params_for(&translated, pitch_slot_node(&conductor, 'B'), 0);
    assert_eq!(got.len(), 1, "expected exactly one pitch correction, got {got:?}");
    assert!(
        (got[0] - expected).abs() < 0.001,
        "key lock applied {} semitones; resampling 174->120 BPM shifts pitch by \
         {:.3}, so the correction must be {expected:.3}",
        got[0],
        12.0 * (120.0f32 / TRACK_BPM).log2()
    );
}

/// Key lock on an UNSYNCED deck is a no-op, because rate is 1.0.
#[test]
fn test_key_lock_without_tempo_sync_does_not_shift() {
    let mut conductor = console();
    register_track(&conductor, 7_201);
    conductor.mixer_manager.deck_samples.insert('B', 7_201);
    conductor.mixer_manager.key_lock_decks.insert('B');
    conductor.mixer_manager.transport_bpm = 120.0;

    let translated = translate(
        &conductor,
        Command::Performance(PerformanceCommand::SetDeckKeyLock { deck_id: 'B', enabled: true }),
    );
    assert_eq!(
        params_for(&translated, pitch_slot_node(&conductor, 'B'), 0),
        vec![0.0],
        "a deck at native tempo has no resampling shift to cancel"
    );
}

/// KEY and KEY LOCK drive the same vocoder, and their corrections ADD.
///
/// This is why one function decides the slot's state: handled separately, the
/// second latch would overwrite the first's shift, or releasing one would remove
/// a node the other still needs.
#[test]
fn test_key_and_key_lock_compose() {
    let mut conductor = console();
    register_master_track(&mut conductor, 7_202, Some(MASTER_ROOT_KEY));
    register_track(&conductor, 7_203);
    conductor.mixer_manager.deck_samples.insert('B', 7_203);
    conductor.mixer_manager.sync_decks.insert('B');
    conductor.mixer_manager.key_sync_decks.insert('B');
    conductor.mixer_manager.key_lock_decks.insert('B');
    conductor.mixer_manager.transport_bpm = 120.0;

    let translated = translate(
        &conductor,
        Command::Performance(PerformanceCommand::SetDeckKeyLock { deck_id: 'B', enabled: true }),
    );

    let harmonic = MASTER_ROOT_KEY - TRACK_ROOT_KEY;
    let lock = -12.0 * (120.0f32 / TRACK_BPM).log2();
    let got = params_for(&translated, pitch_slot_node(&conductor, 'B'), 0);
    assert!(
        (got[0] - (harmonic + lock)).abs() < 0.001,
        "with both latches engaged the vocoder must receive harmonic ({harmonic:.3}) \
         PLUS tempo compensation ({lock:.3}) = {:.3}; got {}",
        harmonic + lock,
        got[0]
    );
}

/// Releasing ONE latch must not remove a vocoder the other still needs.
#[test]
fn test_releasing_key_lock_keeps_the_vocoder_for_key() {
    let mut conductor = console();
    register_master_track(&mut conductor, 7_204, Some(MASTER_ROOT_KEY));
    register_track(&conductor, 7_205);
    conductor.mixer_manager.deck_samples.insert('B', 7_205);
    conductor.mixer_manager.key_sync_decks.insert('B');
    // KEY LOCK was on and is now released; KEY stays engaged.
    conductor.mixer_manager.key_lock_decks.remove(&'B');

    let translated = translate(
        &conductor,
        Command::Performance(PerformanceCommand::SetDeckKeyLock { deck_id: 'B', enabled: false }),
    );

    let swapped_to: Vec<u32> = translated
        .iter()
        .filter_map(|c| match c {
            Command::Topology(nullherz_traits::TopologyCommand::SwapProcessor { processor_type_id, .. }) =>
                Some(processor_type_id.0),
            _ => None,
        })
        .collect();
    assert_eq!(
        swapped_to,
        vec![nullherz_traits::ProcessorTypeId::KEY_SYNC.0],
        "releasing KEY LOCK removed the vocoder while KEY was still engaged"
    );
}
