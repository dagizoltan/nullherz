use nullherz_traits::Command;
use crate::common::*;
use nullherz_traits::ProcessorTypeId;

pub fn create_dj_deck(
    id_allocator: &nullherz_traits::IdAllocator,
    deck_id: char,
    fx_ids: &[u32],
    bus_assignment: char,
    config: &MixerConfig,
) -> Vec<Command> {
    let mut commands = Vec::new();
    println!("Creating DJ Deck: {} assigned to Bus {}", deck_id, bus_assignment);

    let resample_id = id_allocator.allocate_node_id();
    commands.push(Command::AddNode { node_idx: resample_id, processor_type_id: ProcessorTypeId::SAMPLER });

    let gain_id = id_allocator.allocate_node_id();
    commands.push(Command::AddNode { node_idx: gain_id, processor_type_id: ProcessorTypeId::GAIN });
    commands.push(Command::UpdateOutputEdge { node_idx: resample_id, output_idx: 0, new_buffer_idx: id_allocator.allocate_buffer_id(1) });
    commands.push(Command::UpdateEdge { node_idx: gain_id, input_idx: 0, new_buffer_idx: id_allocator.allocate_buffer_id(1) });


    let mut prev_id = gain_id;
    for &fx_type in fx_ids {
        let fx_id = id_allocator.allocate_node_id();
        let fx_buf = id_allocator.allocate_buffer_id(1);

        commands.push(Command::AddNode { node_idx: fx_id, processor_type_id: ProcessorTypeId(fx_type) });
        commands.push(Command::UpdateOutputEdge { node_idx: prev_id, output_idx: 0, new_buffer_idx: fx_buf });
        commands.push(Command::UpdateEdge { node_idx: fx_id, input_idx: 0, new_buffer_idx: fx_buf });
        prev_id = fx_id;
    }

    let eq_id = id_allocator.allocate_node_id();
    let eq_buf = id_allocator.allocate_buffer_id(1);
    commands.push(Command::AddNode { node_idx: eq_id, processor_type_id: ProcessorTypeId::BIQUAD_EQ });
    commands.push(Command::UpdateOutputEdge { node_idx: prev_id, output_idx: 0, new_buffer_idx: eq_buf });
    commands.push(Command::UpdateEdge { node_idx: eq_id, input_idx: 0, new_buffer_idx: eq_buf });

    let target_l = if bus_assignment == 'A' { config.dj_a_l } else { config.dj_b_l };
    let target_r = if bus_assignment == 'A' { config.dj_a_r } else { config.dj_b_r };
    println!("Routing Deck {} to Buffers {}-{}", deck_id, target_l, target_r);
    commands.push(Command::UpdateOutputEdge { node_idx: eq_id, output_idx: 0, new_buffer_idx: target_l as u32 });
    commands.push(Command::UpdateOutputEdge { node_idx: eq_id, output_idx: 1, new_buffer_idx: target_r as u32 });

    commands
}
