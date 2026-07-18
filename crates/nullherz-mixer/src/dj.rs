use nullherz_traits::Command;
use crate::common::*;
use nullherz_traits::ProcessorTypeId;

pub fn create_dj_deck(
    id_allocator: &nullherz_traits::IdAllocator,
    deck_id: char,
    fx_ids: &[u32],
    bus_assignment: char,
    config: &MixerConfig,
) -> (Vec<Command>, crate::DeckNodes) {
    let mut commands = Vec::new();
    println!("Creating DJ Deck: {} assigned to Bus {}", deck_id, bus_assignment);

    let resample_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: resample_id, processor_type_id: ProcessorTypeId::SAMPLER }));

    let dna_morph_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: dna_morph_id, processor_type_id: ProcessorTypeId::DNA_MORPH }));
    let resample_out = id_allocator.allocate_buffer_id(1);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: resample_id, output_idx: 0, new_buffer_idx: resample_out }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: dna_morph_id, input_idx: 0, new_buffer_idx: resample_out }));

    let keysync_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: keysync_id, processor_type_id: ProcessorTypeId::KEY_SYNC }));
    let dna_morph_out = id_allocator.allocate_buffer_id(1);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: dna_morph_id, output_idx: 0, new_buffer_idx: dna_morph_out }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: keysync_id, input_idx: 0, new_buffer_idx: dna_morph_out }));

    let gain_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: gain_id, processor_type_id: ProcessorTypeId::GAIN }));
    let keysync_out = id_allocator.allocate_buffer_id(1);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: keysync_id, output_idx: 0, new_buffer_idx: keysync_out }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: gain_id, input_idx: 0, new_buffer_idx: keysync_out }));

    let filter_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: filter_id, processor_type_id: ProcessorTypeId::BIQUAD }));
    let gain_out = id_allocator.allocate_buffer_id(1);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: gain_id, output_idx: 0, new_buffer_idx: gain_out }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: filter_id, input_idx: 0, new_buffer_idx: gain_out }));

    let stereo_util_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: stereo_util_id, processor_type_id: ProcessorTypeId(160) }));
    let filter_out = id_allocator.allocate_buffer_id(1);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: filter_id, output_idx: 0, new_buffer_idx: filter_out }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: stereo_util_id, input_idx: 0, new_buffer_idx: filter_out }));

    let mut prev_id = stereo_util_id;
    for &fx_type in fx_ids {
        let fx_id = id_allocator.allocate_node_id();
        let fx_buf = id_allocator.allocate_buffer_id(1);

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: fx_id, processor_type_id: ProcessorTypeId(fx_type) }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: prev_id, output_idx: 0, new_buffer_idx: fx_buf }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: fx_id, input_idx: 0, new_buffer_idx: fx_buf }));
        prev_id = fx_id;
    }

    let eq_id = id_allocator.allocate_node_id();
    let eq_buf = id_allocator.allocate_buffer_id(1);
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: eq_id, processor_type_id: ProcessorTypeId::DJ_ISOLATOR }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: prev_id, output_idx: 0, new_buffer_idx: eq_buf }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: eq_id, input_idx: 0, new_buffer_idx: eq_buf }));

    // Unique per-deck output buffers; the bus SUMMING nodes (added by
    // create_4channel_mixer) mix decks onto the shared bus. (bus_assignment
    // decides which summing pair picks these up.)
    let _ = bus_assignment;
    let target_l = id_allocator.allocate_buffer_id(1) as usize;
    let target_r = id_allocator.allocate_buffer_id(1) as usize;
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
    };

    (commands, nodes)
}
