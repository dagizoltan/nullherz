use nullherz_traits::Command;
use crate::common::*;
use nullherz_traits::ProcessorTypeId;

/// Wire one stereo hop: `from`'s outputs 0/1 into `to`'s inputs 0/1 through a
/// freshly allocated L/R buffer pair.
fn link_stereo(
    id_allocator: &nullherz_traits::IdAllocator,
    commands: &mut Vec<Command>,
    from: u32,
    to: u32,
) {
    let buf_l = id_allocator.allocate_buffer_id(2);
    let buf_r = buf_l + 1;
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: from, output_idx: 0, new_buffer_idx: buf_l }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: from, output_idx: 1, new_buffer_idx: buf_r }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: to, input_idx: 0, new_buffer_idx: buf_l }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: to, input_idx: 1, new_buffer_idx: buf_r }));
}

pub fn create_dj_deck(
    id_allocator: &nullherz_traits::IdAllocator,
    deck_id: char,
    fx_ids: &[u32],
    bus_assignment: char,
    config: &MixerConfig,
) -> (Vec<Command>, crate::DeckNodes) {
    let mut commands = Vec::new();
    println!("Creating DJ Deck: {} assigned to Bus {}", deck_id, bus_assignment);

    // The strip is stereo end to end: every hop carries an L/R buffer pair.
    // The sampler renders planar stereo into outputs 0/1, and every stage
    // processor is per-channel (MultiChannelDspProcessor, vocoder lanes, or
    // native stereo). A single-buffer hop here silently discards the right
    // channel — that was the "stereo collapses at the strip" bug.
    let resample_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: resample_id, processor_type_id: ProcessorTypeId::SAMPLER }));

    // SOURCE INSERT SLOT — empty by default.
    //
    // This position used to hold a DnaMorph and a KeySync unconditionally. The
    // KeySync is a phase vocoder, so **every deck carried 1024 samples (21.3 ms)
    // of latency on every block** to serve the KEY latch — which defaults to
    // OFF. Measured deck→master latency was 28.7 ms; detaching it gives 7.3 ms
    // at a 256 block and 3.33 ms at 64 (`examples/probe_deck_latency.rs`).
    //
    // The slot stays in the graph as an identity pass-through and is swapped to
    // a real processor when the operator engages something
    // (`TopologyCommand::SwapProcessor`, which preserves routing — the same
    // mechanism `SidecarSupervisor` uses for its fallback swap). Holding it open
    // costs one buffer copy and zero latency.
    //
    // Making "engaged" a TYPE change rather than a parameter change is also what
    // keeps PDC honest: the swap triggers the commit, and the commit is where
    // `sync_node_latencies` re-reads the chain.
    // TWO source slots, because pitch and DNA are independently engageable and
    // a single slot could only hold one of them.
    let pitch_slot_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: pitch_slot_id, processor_type_id: ProcessorTypeId::BYPASS }));
    link_stereo(id_allocator, &mut commands, resample_id, pitch_slot_id);

    let dna_slot_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: dna_slot_id, processor_type_id: ProcessorTypeId::BYPASS }));
    link_stereo(id_allocator, &mut commands, pitch_slot_id, dna_slot_id);

    let gain_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: gain_id, processor_type_id: ProcessorTypeId::GAIN }));
    link_stereo(id_allocator, &mut commands, dna_slot_id, gain_id);

    let filter_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: filter_id, processor_type_id: ProcessorTypeId::BIQUAD }));
    link_stereo(id_allocator, &mut commands, gain_id, filter_id);

    let stereo_util_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: stereo_util_id, processor_type_id: ProcessorTypeId(160) }));
    link_stereo(id_allocator, &mut commands, filter_id, stereo_util_id);

    // FX INSERT SLOTS — the traditional post-EQ effect chain.
    //
    // Same slot discipline as the source insert: whatever type is requested here
    // is what the slot starts as, and `SwapProcessor` changes it later. Passing
    // `BYPASS` gives an empty, swappable slot; passing a real type pre-loads it.
    let mut fx_slot_ids = Vec::with_capacity(fx_ids.len());
    let mut prev_id = stereo_util_id;
    for &fx_type in fx_ids {
        let fx_id = id_allocator.allocate_node_id();
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: fx_id, processor_type_id: ProcessorTypeId(fx_type) }));
        link_stereo(id_allocator, &mut commands, prev_id, fx_id);
        fx_slot_ids.push(fx_id);
        prev_id = fx_id;
    }

    let eq_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: eq_id, processor_type_id: ProcessorTypeId::DJ_ISOLATOR }));
    link_stereo(id_allocator, &mut commands, prev_id, eq_id);

    // Unique per-deck output buffers; the bus SUMMING nodes (added by
    // create_4channel_mixer) mix decks onto the shared bus. (bus_assignment
    // decides which summing pair picks these up.)
    let _ = bus_assignment;
    let target_l = id_allocator.allocate_buffer_id(2) as usize;
    let target_r = target_l + 1;
    println!("Routing Deck {} to private buffers {}-{}", deck_id, target_l, target_r);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: eq_id, output_idx: 0, new_buffer_idx: target_l as u32 }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: eq_id, output_idx: 1, new_buffer_idx: target_r as u32 }));

    // CUE BUS ROUTING
    // Parallel send from EQ output into PRIVATE per-deck cue buffers; the
    // global CUE bus is mixed by SUMMING nodes in create_4channel_mixer.
    // (All four decks used to write config.cue_l/r DIRECTLY — four
    // producers on one buffer, last writer wins, so the cue bus only ever
    // carried deck D. Same multi-producer disease the preview node had.)
    let _ = config;
    let cue_gain_l_id = id_allocator.allocate_node_id();
    let cue_gain_r_id = id_allocator.allocate_node_id();
    let cue_out_l = id_allocator.allocate_buffer_id(2);
    let cue_out_r = cue_out_l + 1;
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cue_gain_l_id, processor_type_id: ProcessorTypeId::GAIN }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cue_gain_l_id, input_idx: 0, new_buffer_idx: target_l as u32 }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: cue_gain_l_id, output_idx: 0, new_buffer_idx: cue_out_l }));

    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cue_gain_r_id, processor_type_id: ProcessorTypeId::GAIN }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cue_gain_r_id, input_idx: 0, new_buffer_idx: target_r as u32 }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: cue_gain_r_id, output_idx: 0, new_buffer_idx: cue_out_r }));

    // DECK SEQUENCER — trigger generator for DNA groove transfusion and
    // patterns. Not part of the audio path, but it MUST have an output edge:
    // a node with no output buffers gets zero-length outputs and its
    // process() early-returns, so it would never tick. Nothing reads the
    // buffer. (The old design pointed groove commands at the LOGICAL
    // sentinel ids 70-73, which no node backed — dropped silently.)
    let sequencer_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: sequencer_id, processor_type_id: ProcessorTypeId::SEQUENCER }));
    let seq_tick_buf = id_allocator.allocate_buffer_id(1);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: sequencer_id, output_idx: 0, new_buffer_idx: seq_tick_buf }));

    let nodes = crate::DeckNodes {
        sampler_id: resample_id,
        out_l: target_l as u32,
        out_r: target_r as u32,
        isolator_id: eq_id,
        gain_id,
        filter_id,
        pitch_slot_id,
        dna_slot_id,
        fx_slot_ids,
        stereo_util_id,
        sequencer_id,
        cue_out_l,
        cue_out_r,
    };

    (commands, nodes)
}
