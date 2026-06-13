use control_plane::Command;
use crate::common::*;
use nullherz_traits::ProcessorType;

pub fn create_studio_strip(
    next_node_id: &mut u32,
    next_buffer_id: &mut u32,
    name: &str,
    fx_ids: &[u32],
    config: &MixerConfig,
) -> Vec<Command> {
    let mut commands = Vec::new();
    let input_buf = *next_buffer_id;
    *next_buffer_id += 2;

    println!("Creating Studio Strip: {} (Input: {}-{})", name, input_buf, input_buf + 1);

    let gain_id = *next_node_id;
    *next_node_id += 1;
    commands.push(Command::AddNode { node_idx: gain_id, processor_type_id: ProcessorType::Gain as u32 });
    commands.push(Command::UpdateEdge { node_idx: gain_id, input_idx: 0, new_buffer_idx: input_buf });

    let mut prev_node = gain_id;
    let mut prev_buf_l = *next_buffer_id;
    let prev_buf_r = *next_buffer_id + 1;
    *next_buffer_id += 2;

    commands.push(Command::UpdateOutputEdge { node_idx: gain_id, output_idx: 0, new_buffer_idx: prev_buf_l });

    for &fx_type in fx_ids {
        let fx_id = *next_node_id;
        *next_node_id += 1;
        let fx_buf = *next_buffer_id;
        *next_buffer_id += 1;

        commands.push(Command::AddNode { node_idx: fx_id, processor_type_id: fx_type });
        commands.push(Command::UpdateOutputEdge { node_idx: prev_node, output_idx: 0, new_buffer_idx: fx_buf });
        commands.push(Command::UpdateEdge { node_idx: fx_id, input_idx: 0, new_buffer_idx: fx_buf });
        prev_node = fx_id;
        prev_buf_l = fx_buf;
    }

    let fader_id = *next_node_id;
    *next_node_id += 1;
    commands.push(Command::AddNode { node_idx: fader_id, processor_type_id: ProcessorType::Gain as u32 });

    commands.push(Command::UpdateEdge { node_idx: fader_id, input_idx: 0, new_buffer_idx: prev_buf_l });
    commands.push(Command::UpdateEdge { node_idx: fader_id, input_idx: 1, new_buffer_idx: prev_buf_r });
    commands.push(Command::UpdateOutputEdge { node_idx: fader_id, output_idx: 0, new_buffer_idx: config.master_l as u32 });
    commands.push(Command::UpdateOutputEdge { node_idx: fader_id, output_idx: 1, new_buffer_idx: config.master_r as u32 });

    commands
}
