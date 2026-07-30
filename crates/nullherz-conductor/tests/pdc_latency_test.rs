//! Plugin delay compensation must be fed real latencies.
//!
//! `GraphCompiler` derives every path latency from `plan.node_latencies` — it
//! copies that array, it does not poll processors. Its comment says the array is
//! "populated by GraphManager", which is the RT-side path in `audio-core`. But
//! the plan the CONDUCTOR compiles is what gets pushed as `SetTopology`, and the
//! RT thread deliberately does not recompile it (`test_rt_topology_commit_is_no_op`
//! pins that — it is the point of off-thread compilation).
//!
//! So the conductor's plan is authoritative, and nothing conductor-side ever
//! wrote `node_latencies`. Every path latency was derived from zeros.
//!
//! This was invisible for a good reason: every deck carries an identical chain,
//! so there is nothing to compensate, and compensating with the wrong number
//! looks exactly like being right. It stops being invisible the moment one deck
//! gets an FFT insert and another does not — which is precisely what runtime FX
//! inserts introduce.

use nullherz_conductor::Conductor;
use nullherz_processors::registry::ProcessorRegistry;
use nullherz_traits::ProcessorTypeId;

/// The registry must be able to answer "how much latency does this type add?"
/// off the audio thread, before any graph exists.
#[test]
fn test_registry_reports_intrinsic_latency_per_type() {
    let reg = ProcessorRegistry::new();

    // KeySync is a phase vocoder: one analysis window, unconditionally.
    let keysync = reg.latency_for_type(ProcessorTypeId::KEY_SYNC.0, 48_000.0);
    assert_eq!(
        keysync, 1024,
        "KeySync must report its FFT window to the off-thread compiler; got {keysync}"
    );

    // A plain gain stage delays nothing.
    let gain = reg.latency_for_type(ProcessorTypeId::GAIN.0, 48_000.0);
    assert_eq!(gain, 0, "Gain must report zero latency; got {gain}");

    // An unknown type contributes nothing rather than panicking — a node that
    // does not exist delays nothing.
    assert_eq!(reg.latency_for_type(9_999, 48_000.0), 0);
}

/// The default console must be latency-FREE, and the compiler must agree.
///
/// Since roadmap 1.12 the deck chain contains no FFT at all — the vocoder lives
/// in the pitch slot and is installed only when KEY is engaged. So zero here is
/// the correct answer, and it is only meaningful alongside
/// `test_engaging_key_adds_declared_latency`, which proves the zero is measured
/// rather than merely never written.
#[test]
fn test_default_console_reports_no_latency() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let sr = conductor.topology_manager.current_sample_rate;
    nullherz_conductor::topology_manager::TopologyManager::sync_node_latencies(
        &mut conductor.topology_manager.current_topology,
        &conductor.topology_manager.active_node_types,
        &conductor.topology_manager.registry,
        sr,
    );

    // The master LIMITER legitimately declares its look-ahead delay line (2 ms).
    // It is terminal, so it delays every path equally and PDC has nothing to
    // align against it — but it is real latency and declaring it is correct.
    // This assertion is about the DECK CHAIN, so name the one expected node
    // rather than demanding the whole console be zero: the previous spelling
    // scanned every node, which made "the limiter started telling the truth"
    // look identical to "a deck grew a vocoder".
    let limiter = conductor.mixer_manager.node_names.get("master_limiter").copied();
    let plan = &conductor.topology_manager.current_topology.plan;
    let with_latency: Vec<(usize, u32)> = plan
        .node_latencies
        .iter()
        .enumerate()
        .filter(|(i, l)| **l > 0 && Some(*i as u32) != limiter)
        .map(|(i, l)| (i, *l))
        .collect();

    assert!(
        with_latency.is_empty(),
        "the default deck chain must contain no latency at all — every insert slot \
         is an identity pass-through until the operator engages something. Found: \
         {with_latency:?}"
    );

    // And the limiter must declare the delay it actually applies, not a
    // placeholder: an undeclared look-ahead was the last unaccounted latency in
    // the console (353 measured samples against 0 believed).
    if let Some(idx) = limiter {
        let expected = (0.002 * sr).round() as u32;
        assert_eq!(
            plan.node_latencies[idx as usize], expected,
            "master limiter declares {} samples but its look-ahead delay line is \
             {expected} (2 ms at {sr} Hz)",
            plan.node_latencies[idx as usize]
        );
    }
}

/// Engaging KEY must install the vocoder AND tell the compiler about it.
///
/// This is the invariant the whole insert design rests on. The latency arrives as
/// a TYPE change, so it reaches PDC through the topology commit — if the swap did
/// not recompile, the engine would run a vocoder the plan knows nothing about and
/// this deck would drift ~1024 samples against the others (about a fifth of a
/// beat at 120 BPM).
#[test]
fn test_engaging_key_adds_declared_latency() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let pitch_slot = conductor.mixer_manager.deck_mappings[&'A'].pitch_slot_id;

    conductor.apply_mixer_commands(vec![nullherz_traits::Command::Performance(
        nullherz_traits::PerformanceCommand::SetDeckKeySync { deck_id: 'A', enabled: true },
    )]);

    assert_eq!(
        conductor.topology_manager.active_node_types.get(&pitch_slot),
        Some(&nullherz_traits::ProcessorTypeId::KEY_SYNC.0),
        "engaging KEY must swap the vocoder into deck A's pitch slot"
    );
    assert_eq!(
        conductor.topology_manager.current_topology.plan.node_latencies[pitch_slot as usize],
        1024,
        "the vocoder is in the graph but the compiled plan still reports zero \
         latency for its slot — PDC would not compensate it"
    );

    // Releasing must take it back out, latency and all.
    conductor.apply_mixer_commands(vec![nullherz_traits::Command::Performance(
        nullherz_traits::PerformanceCommand::SetDeckKeySync { deck_id: 'A', enabled: false },
    )]);

    assert_eq!(
        conductor.topology_manager.active_node_types.get(&pitch_slot),
        Some(&nullherz_traits::ProcessorTypeId::BYPASS.0),
        "releasing KEY must restore the slot to an identity pass-through"
    );
    assert_eq!(
        conductor.topology_manager.current_topology.plan.node_latencies[pitch_slot as usize],
        0,
        "releasing KEY left the deck declaring vocoder latency it no longer has"
    );
}

/// The sync must actually run on the COMMIT path, not merely exist.
///
/// The first version of this file called `sync_node_latencies` directly and
/// asserted the result. Deleting the call from `CommitTopology` left all three
/// tests green — they proved the function worked, not that anything invoked it.
/// This drives the real command instead, so removing the call fails the suite.
#[test]
fn test_commit_topology_populates_latencies() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    // Put a vocoder in a slot WITHOUT going through the commit, then wipe the
    // array. Only a commit that re-reads the chain can restore the 1024.
    let pitch_slot = conductor.mixer_manager.deck_mappings[&'A'].pitch_slot_id;
    conductor
        .topology_manager
        .active_node_types
        .insert(pitch_slot, nullherz_traits::ProcessorTypeId::KEY_SYNC.0);
    conductor
        .topology_manager
        .current_topology
        .plan
        .node_latencies
        .fill(0);

    let committed = conductor
        .topology_manager
        .handle_topology_command(&nullherz_traits::Command::Core(
            nullherz_traits::CoreCommand::CommitTopology,
        ));
    assert!(committed, "CommitTopology must be handled by the topology manager");

    // Assert the SLOT specifically, not a count of latent nodes. A count also
    // moves when an unrelated node starts declaring honestly (the master
    // limiter's look-ahead did exactly that), which would fail this test for a
    // reason that has nothing to do with the commit path it exists to guard.
    let restored = conductor
        .topology_manager
        .current_topology
        .plan
        .node_latencies[pitch_slot as usize];

    assert!(
        restored > 0,
        "committing the topology did not re-read the chain: a vocoder sits in deck \
         A's pitch slot (node {pitch_slot}) but the compiled plan still declares \
         zero latency for it. The commit path must call sync_node_latencies, not \
         merely have it."
    );
}

/// A node type swapped into a slot must change the latency the compiler sees.
///
/// This is the property runtime inserts depend on: engaging an FFT effect on ONE
/// deck has to move that deck's path latency, or PDC silently lets it drift
/// against the others. At 120 BPM, 1024 samples is about a fifth of a beat.
#[test]
fn test_swapping_a_node_type_changes_reported_latency() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let sr = conductor.topology_manager.current_sample_rate;
    let gain_idx = *conductor
        .mixer_manager
        .node_names
        .get("deck_a_gain")
        .expect("bootstrap names the deck gain node");

    let sync = |c: &mut Conductor| {
        nullherz_conductor::topology_manager::TopologyManager::sync_node_latencies(
            &mut c.topology_manager.current_topology,
            &c.topology_manager.active_node_types,
            &c.topology_manager.registry,
            sr,
        );
    };

    sync(&mut conductor);
    assert_eq!(
        conductor.topology_manager.current_topology.plan.node_latencies[gain_idx as usize],
        0,
        "a Gain node should start at zero latency"
    );

    // Pretend that slot now holds a vocoder, as a runtime insert would.
    conductor
        .topology_manager
        .active_node_types
        .insert(gain_idx, ProcessorTypeId::KEY_SYNC.0);
    sync(&mut conductor);

    assert_eq!(
        conductor.topology_manager.current_topology.plan.node_latencies[gain_idx as usize],
        1024,
        "swapping an FFT processor into a slot did not change the latency the \
         compiler sees — PDC would not compensate the insert"
    );
}
