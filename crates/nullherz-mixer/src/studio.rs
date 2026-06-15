use nullherz_traits::Command;
use crate::common::*;
use nullherz_traits::ProcessorType;

pub fn create_studio_strip(
    next_node_id: &mut u32,
    buffer_allocator: &mut BufferAllocator,
    _name: &str,
    fx_ids: &[u32],
    config: &MixerConfig,
) -> Result<Vec<Command>, String> {
    let mut commands = Vec::new();
    let input_buf = buffer_allocator.allocate()?;
    let _ = buffer_allocator.allocate()?; // second channel

    let gain_id = *next_node_id;
    *next_node_id += 1;
    commands.push(Command::AddNode { node_idx: gain_id, processor_type_id: ProcessorType::Gain as u32 });
    commands.push(Command::UpdateEdge { node_idx: gain_id, input_idx: 0, new_buffer_idx: input_buf });

    let mut prev_node = gain_id;
    let prev_buf_l = buffer_allocator.allocate()?;
    let prev_buf_r = buffer_allocator.allocate()?;

    commands.push(Command::UpdateOutputEdge { node_idx: gain_id, output_idx: 0, new_buffer_idx: prev_buf_l });

    let mut current_prev_buf_l = prev_buf_l;

    for &fx_type in fx_ids {
        let fx_id = *next_node_id;
        *next_node_id += 1;
        let fx_buf = buffer_allocator.allocate()?;

        commands.push(Command::AddNode { node_idx: fx_id, processor_type_id: fx_type });
        commands.push(Command::UpdateOutputEdge { node_idx: prev_node, output_idx: 0, new_buffer_idx: fx_buf });
        commands.push(Command::UpdateEdge { node_idx: fx_id, input_idx: 0, new_buffer_idx: fx_buf });
        prev_node = fx_id;
        current_prev_buf_l = fx_buf;
    }

    let fader_id = *next_node_id;
    *next_node_id += 1;
    commands.push(Command::AddNode { node_idx: fader_id, processor_type_id: ProcessorType::Gain as u32 });

    commands.push(Command::UpdateEdge { node_idx: fader_id, input_idx: 0, new_buffer_idx: current_prev_buf_l });
    commands.push(Command::UpdateEdge { node_idx: fader_id, input_idx: 1, new_buffer_idx: prev_buf_r });
    commands.push(Command::UpdateOutputEdge { node_idx: fader_id, output_idx: 0, new_buffer_idx: config.master_l as u32 });
    commands.push(Command::UpdateOutputEdge { node_idx: fader_id, output_idx: 1, new_buffer_idx: config.master_r as u32 });

    Ok(commands)
}
