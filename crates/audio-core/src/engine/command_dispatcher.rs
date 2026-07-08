use nullherz_traits::AudioProcessor;

pub struct CommandDispatcher;

impl CommandDispatcher {
    pub fn handle_single_command(
        transport: &mut nullherz_traits::Transport,
        graph: &mut dyn AudioProcessor,
        cmd: &nullherz_traits::Command,
    ) {
        use nullherz_traits::{Command, CoreCommand, MixerCommand, TopologyCommand};
        match cmd {
            Command::Core(CoreCommand::Play) => {
                if !transport.is_playing {
                    transport.is_playing = true;
                    graph.apply_command(cmd);
                }
            }
            Command::Core(CoreCommand::Stop) => {
                if transport.is_playing {
                    transport.is_playing = false;
                    graph.apply_command(cmd);
                }
            }
            Command::Topology(TopologyCommand::UpdateEdge { node_idx, input_idx, new_buffer_idx }) => {
                Self::apply_topology_update(graph, *node_idx, Some(*input_idx), None, *new_buffer_idx);
            }
            Command::Topology(TopologyCommand::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx }) => {
                Self::apply_topology_update(graph, *node_idx, None, Some(*output_idx), *new_buffer_idx);
            }
            Command::Core(CoreCommand::CommitTopology) => {
                Self::commit_topology(graph);
            }
            Command::Core(CoreCommand::SetBpm(bpm)) => {
                transport.bpm = *bpm;
            }
            Command::Mixer(MixerCommand::Bundle { count, data }) => {
                Self::handle_bundle_command(graph, *count, *data);
            }
            Command::Topology(TopologyCommand::AddNode { .. }) | Command::Topology(TopologyCommand::SwapProcessor { .. }) => {
                // Ignore structural mutations in RT command loop.
            }
            Command::Topology(TopologyCommand::SetBypass { node_idx, enabled }) => {
                 graph.apply_topology_mutation(nullherz_traits::TopologyMutation::SetBypass {
                     node_idx: *node_idx,
                     enabled: *enabled,
                 });
            }
            _ => { graph.apply_command(cmd); }
        }
    }

    fn apply_topology_update(graph: &mut dyn AudioProcessor, node_idx: u32, input_idx: Option<u32>, output_idx: Option<u32>, new_buffer_idx: u32) {
        if let Some(input_idx) = input_idx {
            graph.apply_topology_mutation(nullherz_traits::TopologyMutation::UpdateEdge {
                node_idx,
                input_idx,
                new_buffer_idx,
            });
        } else if let Some(output_idx) = output_idx {
            graph.apply_topology_mutation(nullherz_traits::TopologyMutation::UpdateOutputEdge {
                node_idx,
                output_idx,
                new_buffer_idx,
            });
        }
    }

    fn commit_topology(graph: &mut dyn AudioProcessor) {
        use nullherz_traits::{Command, CoreCommand};
        graph.apply_command(&Command::Core(CoreCommand::CommitTopology));
    }

    fn handle_bundle_command(graph: &mut dyn AudioProcessor, count: u32, data: [u8; 128]) {
        let cmd = nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::Bundle { count, data });
        if let Some(iter) = cmd.bundle_iter() {
            for sub_cmd in iter {
                graph.apply_command(&sub_cmd);
            }
        }
    }
}
