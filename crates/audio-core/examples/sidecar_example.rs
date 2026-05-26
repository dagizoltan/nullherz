use audio_core::{AudioEngine, AudioProcessor, SidecarProcessor, ProcessorChain};
use control_plane::{Command};
use ipc_layer::{RingBuffer, ShmRingBuffer, AudioBlock};
use std::thread;

struct MockSidecarProcessor;
impl AudioProcessor for MockSidecarProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for i in 0..inputs.len().min(outputs.len()) {
            for j in 0..inputs[i].len() {
                outputs[i][j] = inputs[i][j] * 0.5;
            }
        }
    }
    fn apply_command(&mut self, command: &Command) {
        if let Command::Play = command {
            println!("Sidecar: Received Play command!");
        }
    }
}

#[derive(Copy, Clone)]
struct ShmPtr(usize);
unsafe impl Send for ShmPtr {}

fn main() {
    // 1. Setup Shared Memory for Sidecar
    let cmd_cap = 64;
    let (cmd_layout, _) = ShmRingBuffer::<Command>::layout(cmd_cap);
    let cmd_mem = Box::leak(vec![0u8; cmd_layout.size() + 64].into_boxed_slice());
    let cmd_ptr = cmd_mem.as_mut_ptr();
    let cmd_aligned = unsafe { cmd_ptr.add(cmd_ptr.align_offset(64)) };
    let cmd_rb_ptr = unsafe { ShmRingBuffer::<Command>::init(cmd_aligned, cmd_cap) };

    let audio_cap = 8;
    let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(audio_cap);

    let in_mem = Box::leak(vec![0u8; audio_layout.size() + 64].into_boxed_slice());
    let in_ptr = in_mem.as_mut_ptr();
    let in_aligned = unsafe { in_ptr.add(in_ptr.align_offset(64)) };
    let in_rb_ptr = unsafe { ShmRingBuffer::<AudioBlock>::init(in_aligned, audio_cap) };

    let out_mem = Box::leak(vec![0u8; audio_layout.size() + 64].into_boxed_slice());
    let out_ptr = out_mem.as_mut_ptr();
    let out_aligned = unsafe { out_ptr.add(out_ptr.align_offset(64)) };
    let out_rb_ptr = unsafe { ShmRingBuffer::<AudioBlock>::init(out_aligned, audio_cap) };

    let cmd_ptr_wrapper = ShmPtr(cmd_rb_ptr as usize);
    let in_ptr_wrapper = ShmPtr(in_rb_ptr as usize);
    let out_ptr_wrapper = ShmPtr(out_rb_ptr as usize);

    // 2. Start Sidecar Thread
    let _sidecar_handle = thread::spawn(move || {
        let cmd_ptr = cmd_ptr_wrapper.0 as *const ShmRingBuffer<Command>;
        let in_ptr = in_ptr_wrapper.0 as *mut ShmRingBuffer<AudioBlock>;
        let out_ptr = out_ptr_wrapper.0 as *mut ShmRingBuffer<AudioBlock>;

        let mut processor = MockSidecarProcessor;
        println!("Sidecar: Started loop");
        for _ in 0..100 {
            while let Some(cmd) = unsafe { (*cmd_ptr).pop() } {
                processor.apply_command(&cmd);
            }
            if let Some(in_block) = unsafe { (*in_ptr).pop() } {
                let mut out_block = AudioBlock { data: [0.0; 128] };
                processor.process(&[&in_block.data], &mut [&mut out_block.data]);
                unsafe { let _ = (*out_ptr).push(out_block); }
            }
            thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    // 3. Setup Engine
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();

    let mut engine = AudioEngine::new(cons, garbage_prod, Box::new(ProcessorChain::new()));

    let sidecar_proxy = unsafe { SidecarProcessor::new(cmd_rb_ptr, &[in_rb_ptr], &[out_rb_ptr]) };
    engine.request_swap({
        let mut g = Box::new(ProcessorChain::new());
        g.add(Box::new(sidecar_proxy));
        g
    });

    // Run engine block with some dummy audio
    let mut out_buffer = [1.0f32; 128];
    {
        let mut out_ptrs = [&mut out_buffer[..]];
        engine.process_block(&[&[1.0; 128]], &mut out_ptrs, 128);

        thread::sleep(std::time::Duration::from_millis(10));

        engine.process_block(&[&[1.0; 128]], &mut out_ptrs, 128);

        println!("Engine: Sample 0 after sidecar: {} (expected ~0.5)", out_buffer[0]);
    }

    println!("Engine: Simulation finished.");
}
