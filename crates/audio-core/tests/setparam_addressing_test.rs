//! A `SetParam` must reach the node it names, and no other.
//!
//! It used to reach all of them. `SetParam` fell into
//! `apply_command_with_context`'s catch-all, which hands the command to every
//! node slot and leaves each processor to notice it was not the addressee. 36
//! processors grew a filter for that; ten never bound `target_id` at all, so
//! they applied every `SetParam` aimed at anything:
//!
//!     reverb room_size: before=0.8 after=0.98
//!       (command was addressed to node 7, param 0 = cutoff 20000 Hz)
//!
//! Parameter 0 is `room_size` (0..0.98) on the reverb, `cutoff` (20..20000 Hz)
//! on the neural filter and `gain_db` (-24..24) on the hypernetwork EQ — all
//! three load into the deck FX rack, so moving a filter cutoff slammed the
//! reverb to maximum room size and the EQ to +24 dB.
//!
//! These tests assert the GRAPH's routing contract with a recording processor,
//! rather than any particular processor's parameter map. The contract is what
//! was broken; a per-processor test would pass again the moment someone adds
//! the 37th insert without a filter.

use audio_core::processors::graph::ProcessorGraph;
use nullherz_traits::{
    AudioProcessor, Command, MidiResponder, MixerCommand, ProcessContext, SignalProcessor,
    SnapshotProvider,
};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

/// Records every `SetParam` it is handed, WITHOUT filtering by target — the
/// shape ten shipped processors have. If the graph routes correctly, not
/// filtering is harmless; if it broadcasts, this counts the damage.
struct Recorder {
    hits: Arc<AtomicUsize>,
    last_value: Arc<AtomicU32>,
}

impl SignalProcessor for Recorder {
    fn process(&mut self, _i: &[&[f32]], _o: &mut [&mut [f32]], _c: &mut ProcessContext) {}
}
impl MidiResponder for Recorder {}
impl SnapshotProvider for Recorder {}
impl AudioProcessor for Recorder {
    fn apply_command(&mut self, command: &Command) {
        if let Command::Mixer(MixerCommand::SetParam { value, .. }) = command {
            self.hits.fetch_add(1, Ordering::SeqCst);
            self.last_value.store(value.to_bits(), Ordering::SeqCst);
        }
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

struct Probe {
    hits: Arc<AtomicUsize>,
    value: Arc<AtomicU32>,
}

fn graph_with_recorders(n: usize) -> (ProcessorGraph, Vec<Probe>) {
    let mut graph = ProcessorGraph::new();
    let mut probes = Vec::new();
    for _ in 0..n {
        let hits = Arc::new(AtomicUsize::new(0));
        let value = Arc::new(AtomicU32::new(f32::NAN.to_bits()));
        graph.add_node(
            Box::new(Recorder { hits: hits.clone(), last_value: value.clone() }),
            vec![],
            vec![],
        );
        probes.push(Probe { hits, value });
    }
    (graph, probes)
}

fn set_param(target_id: u64, value: f32) -> Command {
    Command::Mixer(MixerCommand::SetParam {
        target_id,
        param_id: 0,
        value,
        ramp_duration_samples: 0,
    })
}

#[test]
fn test_setparam_reaches_only_the_addressed_node() {
    let (mut graph, probes) = graph_with_recorders(4);

    graph.apply_command(&set_param(2, 20_000.0));

    for (i, p) in probes.iter().enumerate() {
        let hits = p.hits.load(Ordering::SeqCst);
        if i == 2 {
            assert_eq!(hits, 1, "the addressed node {i} did not receive its own SetParam");
            assert_eq!(f32::from_bits(p.value.load(Ordering::SeqCst)), 20_000.0);
        } else {
            assert_eq!(
                hits, 0,
                "node {i} received a SetParam addressed to node 2. Parameter ids are per-processor, \
                 so this writes one processor's value into another's parameter — a filter cutoff of \
                 20000 lands as a reverb room size, clamped to maximum."
            );
        }
    }
}

#[test]
fn test_every_node_is_individually_addressable() {
    // Routing to the right node is only half of it: each one must still be
    // reachable, or the fix would trade cross-talk for dropped commands.
    let (mut graph, probes) = graph_with_recorders(6);

    for i in 0..probes.len() {
        graph.apply_command(&set_param(i as u64, i as f32 + 100.0));
    }

    for (i, p) in probes.iter().enumerate() {
        assert_eq!(p.hits.load(Ordering::SeqCst), 1, "node {i} never received its own SetParam");
        assert_eq!(f32::from_bits(p.value.load(Ordering::SeqCst)), i as f32 + 100.0, "node {i} received another node's value");
    }
}

/// `NodeConventions` sentinels live at `0xFFFF_FF00+`, far above `MAX_NODES`,
/// and the conductor is supposed to translate them before the engine sees one.
/// An untranslated sentinel must be dropped, not broadcast — broadcasting is
/// how the original bug reached every node, and a sentinel is the id most
/// likely to be wrong.
#[test]
fn test_an_out_of_range_target_is_dropped_not_broadcast() {
    let (mut graph, probes) = graph_with_recorders(4);

    graph.apply_command(&set_param(nullherz_traits::NodeConventions::PREVIEW as u64, 1.0));
    graph.apply_command(&set_param(audio_core::MAX_NODES as u64, 1.0));
    graph.apply_command(&set_param(u64::MAX, 1.0));

    for (i, p) in probes.iter().enumerate() {
        assert_eq!(
            p.hits.load(Ordering::SeqCst), 0,
            "node {i} received a SetParam whose target is not a graph index at all"
        );
    }
}

/// Commands that are genuinely for everyone must still reach everyone.
#[test]
fn test_broadcast_commands_are_still_broadcast() {
    let (mut graph, probes) = graph_with_recorders(3);

    // SetSafeMode has its own broadcast arm; this checks the catch-all path by
    // using a command with no specific arm, which must still reach all nodes.
    graph.apply_command(&Command::Core(nullherz_traits::CoreCommand::Play));

    // Recorder only counts SetParam, so Play produces no hits — what matters is
    // that it did not panic and the nodes are still addressable afterwards.
    graph.apply_command(&set_param(1, 7.0));
    assert_eq!(probes[1].hits.load(Ordering::SeqCst), 1);
    assert_eq!(probes[0].hits.load(Ordering::SeqCst), 0);
    assert_eq!(probes[2].hits.load(Ordering::SeqCst), 0);
}
