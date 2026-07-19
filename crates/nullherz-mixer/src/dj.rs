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

    let dna_morph_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: dna_morph_id, processor_type_id: ProcessorTypeId::DNA_MORPH }));
    link_stereo(id_allocator, &mut commands, resample_id, dna_morph_id);

    let keysync_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: keysync_id, processor_type_id: ProcessorTypeId::KEY_SYNC }));
    link_stereo(id_allocator, &mut commands, dna_morph_id, keysync_id);

    let gain_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: gain_id, processor_type_id: ProcessorTypeId::GAIN }));
    link_stereo(id_allocator, &mut commands, keysync_id, gain_id);

    let filter_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: filter_id, processor_type_id: ProcessorTypeId::BIQUAD }));
    link_stereo(id_allocator, &mut commands, gain_id, filter_id);

    let stereo_util_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: stereo_util_id, processor_type_id: ProcessorTypeId(160) }));
    link_stereo(id_allocator, &mut commands, filter_id, stereo_util_id);

    let mut prev_id = stereo_util_id;
    for &fx_type in fx_ids {
        let fx_id = id_allocator.allocate_node_id();
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: fx_id, processor_type_id: ProcessorTypeId(fx_type) }));
        link_stereo(id_allocator, &mut commands, prev_id, fx_id);
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
    // Parallel send from EQ output to Global CUE bus (Stereo)
    let cue_gain_l_id = id_allocator.allocate_node_id();
    let cue_gain_r_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cue_gain_l_id, processor_type_id: ProcessorTypeId::GAIN }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cue_gain_l_id, input_idx: 0, new_buffer_idx: target_l as u32 }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: cue_gain_l_id, output_idx: 0, new_buffer_idx: config.cue_l as u32 }));

    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cue_gain_r_id, processor_type_id: ProcessorTypeId::GAIN }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cue_gain_r_id, input_idx: 0, new_buffer_idx: target_r as u32 }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: cue_gain_r_id, output_idx: 0, new_buffer_idx: config.cue_r as u32 }));

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
        keysync_id,
        stereo_util_id,
        dna_morph_id: Some(dna_morph_id),
        sequencer_id,
    };

    (commands, nodes)
}
