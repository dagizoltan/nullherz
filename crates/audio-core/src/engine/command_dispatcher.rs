use nullherz_traits::AudioProcessor;

pub struct CommandDispatcher;

impl CommandDispatcher {
    pub fn handle_single_command(
        transport: &mut nullherz_traits::Transport,
        graph: &mut dyn AudioProcessor,
        cmd: &nullherz_traits::Command,
    ) {
        match cmd {
            nullherz_traits::Command::Play => {
                if !transport.is_playing {
                    transport.is_playing = true;
                    graph.apply_command(cmd);
                }
            }
            nullherz_traits::Command::Stop => {
                if transport.is_playing {
                    transport.is_playing = false;
                    graph.apply_command(cmd);
                }
            }
            nullherz_traits::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                Self::apply_topology_update(graph, *node_idx, Some(*input_idx), None, *new_buffer_idx);
            }
            nullherz_traits::Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                Self::apply_topology_update(graph, *node_idx, None, Some(*output_idx), *new_buffer_idx);
            }
            nullherz_traits::Command::CommitTopology => {
                Self::commit_topology(graph);
            }
            nullherz_traits::Command::Bundle { count, data } => {
                Self::handle_bundle_command(graph, *count, *data);
            }
            nullherz_traits::Command::AddNode { .. } | nullherz_traits::Command::SwapProcessor { .. } => {
                // Ignore structural mutations in RT command loop.
            }
            _ => { graph.apply_command(cmd); }
        }
    }

    fn apply_topology_update(graph: &mut dyn AudioProcessor, node_idx: u32, input_idx: Option<u32>, output_idx: Option<u32>, new_buffer_idx: u32) {
        if let Some(input_idx) = input_idx {
            let _ = graph.apply_topology_mutation(nullherz_traits::TopologyMutation::UpdateEdge {
                node_idx,
                input_idx,
                new_buffer_idx,
            });
        } else if let Some(output_idx) = output_idx {
            let _ = graph.apply_topology_mutation(nullherz_traits::TopologyMutation::UpdateOutputEdge {
                node_idx,
                output_idx,
                new_buffer_idx,
            });
        }
    }

    fn commit_topology(graph: &mut dyn AudioProcessor) {
        graph.apply_command(&nullherz_traits::Command::CommitTopology);
    }

    fn handle_bundle_command(graph: &mut dyn AudioProcessor, count: u32, data: [u64; 12]) {
        for i in 0..(count as usize).min(4) {
            let node_id = data[i * 3];
            let param_id = data[i * 3 + 1] as u32;
            let value = f32::from_bits(data[i * 3 + 2] as u32);

            // Optimization: if the graph is a ProcessorGraph, it might want to handle it specially.
            // But for uniform trait access, we use set_parameter if it were targeted at the graph itself.
            // Actually, SetParam usually targets individual nodes via target_id.
            // So we still need to pass it to the graph to dispatch.
            graph.apply_command(&nullherz_traits::Command::SetParam {
                target_id: node_id, param_id, value, ramp_duration_samples: 0,
            });
        }
    }
}
