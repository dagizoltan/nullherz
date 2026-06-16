use nullherz_traits::Command;
use crate::common::*;
use nullherz_traits::ProcessorTypeId;

pub fn create_dj_deck(
    next_node_id: &mut u32,
    next_buffer_id: &mut u32,
    deck_id: char,
    fx_ids: &[u32],
    bus_assignment: char,
    config: &MixerConfig,
) -> Vec<Command> {
    let mut commands = Vec::new();
    println!("Creating DJ Deck: {} assigned to Bus {}", deck_id, bus_assignment);

    let resample_id = *next_node_id;
    *next_node_id += 1;
    commands.push(Command::AddNode { node_idx: resample_id, processor_type_id: ProcessorTypeId::SAMPLER });

    let mut prev_id = resample_id;
    for &fx_type in fx_ids {
        let fx_id = *next_node_id;
        *next_node_id += 1;
        let fx_buf = *next_buffer_id;
        *next_buffer_id += 1;

        commands.push(Command::AddNode { node_idx: fx_id, processor_type_id: ProcessorTypeId(fx_type) });
        commands.push(Command::UpdateOutputEdge { node_idx: prev_id, output_idx: 0, new_buffer_idx: fx_buf });
        commands.push(Command::UpdateEdge { node_idx: fx_id, input_idx: 0, new_buffer_idx: fx_buf });
        prev_id = fx_id;
    }

    let eq_id = *next_node_id;
    *next_node_id += 1;
    let eq_buf = *next_buffer_id;
    *next_buffer_id += 1;
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
