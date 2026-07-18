use nullherz_traits::{AudioProcessor, TimestampedCommand, SubBlockIterator, SubBlock, Transport, Host, ParallelExecutor, ProcessingKernel, CommandConsumer};
use crate::engine::command_dispatcher::CommandDispatcher;

#[derive(Default, Clone)]
pub struct StandardKernel;

impl ProcessingKernel for StandardKernel {
    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        graph: &mut dyn AudioProcessor,
        transport: &mut Transport,
        host: Option<&dyn Host>,
        pool: &mut Option<Box<dyn ParallelExecutor>>,
        command_consumer: &mut Box<dyn CommandConsumer>,
        pending_command: &mut Option<TimestampedCommand>,
        sample_counter: u64,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        num_samples: usize,
    ) {
        let block_start_sample = sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;
        let mut commands_processed = 0;

        let mut sub_block_iter = SubBlockIterator::new(num_samples, ipc_layer::MAX_BLOCK_SIZE);

        let mut cmd = pending_command.take();

        // Optimized Loop: Minimize sub-block fragmentation and avoid unnecessary polls
        while sub_block_iter.current_offset < num_samples {
            if cmd.is_none() && commands_processed < nullherz_traits::MAX_COMMANDS_PER_BLOCK {
                cmd = command_consumer.pop_command();
            }

            match cmd.take() {
                Some(c) if c.timestamp_samples < block_end_sample => {
                    commands_processed += 1;

                    let target_offset = if c.timestamp_samples > block_start_sample {
                        (c.timestamp_samples - block_start_sample) as usize
                    } else {
                        sub_block_iter.current_offset
                    };

                    // Process up to the command's timestamp
                    while sub_block_iter.current_offset < target_offset {
                        if let Some(sb) = sub_block_iter.next_chunk_up_to(target_offset) {
                            Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
                        } else {
                            break;
                        }
                    }

                    // Apply the command at the correct sample-accurate point
                    let is_last_sub_block = target_offset + ipc_layer::MAX_BLOCK_SIZE >= num_samples;
                    CommandDispatcher::handle_single_command_with_context(transport, target_offset, is_last_sub_block, host, graph, &c.command);

                    // Batch processing: Drain all commands with the same timestamp to minimize sub-block fragmentation
                    loop {
                        if commands_processed >= nullherz_traits::MAX_COMMANDS_PER_BLOCK { break; }
                        if let Some(next_cmd) = command_consumer.pop_command() {
                            if next_cmd.timestamp_samples == c.timestamp_samples {
                                CommandDispatcher::handle_single_command_with_context(transport, target_offset, is_last_sub_block, host, graph, &next_cmd.command);
                                commands_processed += 1;
                            } else {
                                cmd = Some(next_cmd);
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                Some(c) => {
                    // Command is beyond this block
                    *pending_command = Some(c);
                    break;
                }
                None => {
                    // No more commands for this block
                    break;
                }
            }
        }

        // Process remaining audio in the block
        while let Some(sb) = sub_block_iter.next_chunk() {
            Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
        }

        if let Some(remaining) = cmd
            && pending_command.is_none() {
                *pending_command = Some(remaining);
            }
    }
}

impl StandardKernel {
    fn process_sub_block_and_advance_transport(
        graph: &mut dyn AudioProcessor,
        transport: &mut Transport,
        host: Option<&dyn Host>,
        pool: &mut Option<Box<dyn ParallelExecutor>>,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sb: SubBlock,
    ) {
        Self::process_sub_block(graph, transport, host, pool, inputs, outputs, sb.offset, sb.len, sb.is_last);
        if transport.is_playing {
            let beats = (sb.len as f64 / transport.sample_rate as f64) * (transport.bpm as f64 / 60.0);
            transport.beat_position += beats;
        }
        transport.absolute_samples += sb.len as u64;
    }

    #[allow(clippy::too_many_arguments)]
    fn process_sub_block(
        graph: &mut dyn AudioProcessor,
        transport: &Transport,
        host: Option<&dyn Host>,
        pool: &mut Option<Box<dyn ParallelExecutor>>,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        offset: usize,
        len: usize,
        is_last_sub_block: bool,
    ) {
        if len == 0 { return; }
        let mut context = nullherz_traits::ProcessContext {
            transport: Some(transport),
            host,
            sub_block_offset: offset,
            is_last_sub_block
        };

        let mut sub_inputs_ptr = [ &[][..]; crate::MAX_CHANNELS ];
        let num_inputs = inputs.len().min(crate::MAX_CHANNELS);
        let empty_input = &[][..];
        for (i, sub_input) in sub_inputs_ptr.iter_mut().enumerate().take(num_inputs) {
            let input = inputs.get(i).copied().unwrap_or(empty_input);
            let end = (offset + len).min(input.len());
            let act = offset.min(input.len());
            if end > act {
                *sub_input = &input[act..end];
            } else {
                 *sub_input = &[][..];
            }
        }

        let mut sub_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
        let num_outputs = outputs.len().min(crate::MAX_CHANNELS);
        for (i, out) in outputs.iter_mut().take(num_outputs).enumerate() {
            let end = (offset + len).min(out.len());
            let act = offset.min(out.len());
            if end > act { sub_outputs_reconstructed[i] = &mut out[act..end]; }
        }

        graph.process_parallel(
            &sub_inputs_ptr[..num_inputs],
            &mut sub_outputs_reconstructed[..num_outputs],
            &mut context,
            pool.as_deref_mut()
        );
    }
}
