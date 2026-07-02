use nullherz_traits::Command;
use crate::common::*;
use nullherz_traits::ProcessorTypeId;

pub fn create_studio_strip(
    id_allocator: &nullherz_traits::IdAllocator,
    name: &str,
    fx_ids: &[u32],
    config: &MixerConfig,
) -> Vec<Command> {
    let mut commands = Vec::new();
    let input_buf = id_allocator.allocate_buffer_id(2);

    println!("Creating Studio Strip: {} (Input: {}-{})", name, input_buf, input_buf + 1);

    let gain_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: gain_id, processor_type_id: ProcessorTypeId::GAIN }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: gain_id, input_idx: 0, new_buffer_idx: input_buf }));

    let mut prev_node = gain_id;
    let mut prev_buf_l = id_allocator.allocate_buffer_id(2);
    let prev_buf_r = prev_buf_l + 1;

    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: gain_id, output_idx: 0, new_buffer_idx: prev_buf_l }));

    for &fx_type in fx_ids {
        let fx_id = id_allocator.allocate_node_id();
        let fx_buf = id_allocator.allocate_buffer_id(1);

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: fx_id, processor_type_id: ProcessorTypeId(fx_type) }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: prev_node, output_idx: 0, new_buffer_idx: fx_buf }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: fx_id, input_idx: 0, new_buffer_idx: fx_buf }));
        prev_node = fx_id;
        prev_buf_l = fx_buf;
    }

    let fader_id = id_allocator.allocate_node_id();
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: fader_id, processor_type_id: ProcessorTypeId::GAIN }));

    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: fader_id, input_idx: 0, new_buffer_idx: prev_buf_l }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: fader_id, input_idx: 1, new_buffer_idx: prev_buf_r }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: fader_id, output_idx: 0, new_buffer_idx: config.master_l as u32 }));
    commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: fader_id, output_idx: 1, new_buffer_idx: config.master_r as u32 }));

    commands
}
