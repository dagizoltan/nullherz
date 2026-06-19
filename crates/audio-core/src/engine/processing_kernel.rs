use nullherz_traits::{AudioProcessor, TimestampedCommand, SubBlockIterator, SubBlock, Transport, Host, ParallelExecutor};
use crate::engine::command_dispatcher::CommandDispatcher;

pub struct ProcessingKernel {}

impl ProcessingKernel {
    #[allow(clippy::too_many_arguments)]
    pub fn execute_processing_kernel(
        graph: &mut dyn AudioProcessor,
        transport: &mut Transport,
        host: Option<&dyn Host>,
        pool: &mut Option<Box<dyn ParallelExecutor>>,
        command_consumer: &mut Box<dyn nullherz_traits::CommandConsumer>,
        pending_command: &mut Option<TimestampedCommand>,
        sample_counter: u64,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        num_samples: usize,
    ) {
        let block_start_sample = sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;
        let mut commands_processed = 0;
        const MAX_COMMANDS_PER_BLOCK: usize = 256;

        let mut sub_block_iter = SubBlockIterator::new(num_samples, ipc_layer::MAX_BLOCK_SIZE);

        while sub_block_iter.current_offset < num_samples {
            let cmd = if let Some(pending) = pending_command.take() { Some(pending) } else {
                if commands_processed < MAX_COMMANDS_PER_BLOCK { command_consumer.pop_command() } else { None }
            };

            if let Some(cmd) = cmd {
                if cmd.timestamp_samples < block_end_sample {
                    commands_processed += 1;
                    let cmd_offset = if cmd.timestamp_samples > block_start_sample { (cmd.timestamp_samples - block_start_sample) as usize } else { sub_block_iter.current_offset };

                    while let Some(sb) = sub_block_iter.next_chunk_up_to(cmd_offset) {
                        Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
                    }

                    CommandDispatcher::handle_single_command(transport, graph, &cmd.command);
                } else {
                    *pending_command = Some(cmd);
                    while let Some(sb) = sub_block_iter.next_chunk() {
                        Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
                    }
                }
            } else {
                while let Some(sb) = sub_block_iter.next_chunk() {
                    Self::process_sub_block_and_advance_transport(graph, transport, host, pool, inputs, outputs, sb);
                }
            }
        }
    }

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
        Self::advance_transport(transport, sb.len);
    }

    fn advance_transport(transport: &mut Transport, num_samples: usize) {
        if transport.is_playing {
            let beats = (num_samples as f64 / transport.sample_rate as f64) * (transport.bpm as f64 / 60.0);
            transport.beat_position += beats;
        }
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
        let mut context = nullherz_traits::ProcessContext { transport: Some(transport), host, sub_block_offset: offset, is_last_sub_block };
        let mut sub_inputs_ptr = [ &[][..]; crate::MAX_CHANNELS ];
        let num_inputs = inputs.len().min(crate::MAX_CHANNELS);
        let empty_input = &[][..];
        for (i, sub_input) in sub_inputs_ptr.iter_mut().enumerate().take(num_inputs) {
            let input = inputs.get(i).copied().unwrap_or(empty_input);
            let end = (offset + len).min(input.len());
            let act = offset.min(input.len());
            *sub_input = &input[act..end];
        }
        let mut sub_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
        let num_outputs = outputs.len().min(crate::MAX_CHANNELS);
        for (i, out) in outputs.iter_mut().take(num_outputs).enumerate() {
            let end = (offset + len).min(out.len());
            let act = offset.min(out.len());
            if end > act { sub_outputs_reconstructed[i] = &mut out[act..end]; }
        }

        graph.process_parallel(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs], &mut context, pool.as_deref_mut());
    }
}
