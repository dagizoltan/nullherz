use audio_core::{AudioEngine, AudioProcessor, SidecarProcessor, ProcessorChain, ThreadedBackend, AudioBackend};
use control_plane::{Command};
use ipc_layer::{RingBuffer, ShmRingBuffer, AudioBlock, SharedMemory};
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

fn main() {
    // 1. Setup Shared Memory for Sidecar
    let cmd_cap = 64;
    let (cmd_layout, _) = ShmRingBuffer::<Command>::layout(cmd_cap);
    let cmd_shm = SharedMemory::create("nullherz_cmd", cmd_layout.size()).unwrap();
    let cmd_rb_ptr = unsafe { ShmRingBuffer::<Command>::init(cmd_shm.ptr(), cmd_cap) };

    let audio_cap = 8;
    let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(audio_cap);

    let in_shm = SharedMemory::create("nullherz_in_0", audio_layout.size()).unwrap();
    let in_rb_ptr = unsafe { ShmRingBuffer::<AudioBlock>::init(in_shm.ptr(), audio_cap) };

    let out_shm = SharedMemory::create("nullherz_out_0", audio_layout.size()).unwrap();
    let out_rb_ptr = unsafe { ShmRingBuffer::<AudioBlock>::init(out_shm.ptr(), audio_cap) };

    // 2. Start Sidecar Thread (using the open() side to simulate process isolation)
    let _sidecar_handle = thread::spawn(move || {
        // In a real process, we would use SharedMemory::open("nullherz_cmd", ...)
        // For this thread simulation, we can just use the pointers or re-open.
        let cmd_shm_side = SharedMemory::open("nullherz_cmd", cmd_layout.size()).unwrap();
        let in_shm_side = SharedMemory::open("nullherz_in_0", audio_layout.size()).unwrap();
        let out_shm_side = SharedMemory::open("nullherz_out_0", audio_layout.size()).unwrap();

        let mut processor = MockSidecarProcessor;
        println!("Sidecar: Started loop");
        for _ in 0..200 {
            // Commands
            let cmd_rb = unsafe { &*(cmd_shm_side.ptr() as *const ShmRingBuffer<Command>) };
            while let Some(cmd) = cmd_rb.pop() {
                processor.apply_command(&cmd);
            }
            // Audio
            let in_rb = unsafe { &*(in_shm_side.ptr() as *const ShmRingBuffer<AudioBlock>) };
            let out_rb = unsafe { &mut *(out_shm_side.ptr() as *mut ShmRingBuffer<AudioBlock>) };

            if let Some(in_block) = in_rb.pop() {
                let mut out_block = AudioBlock { data: [0.0; 128] };
                processor.process(&[&in_block.data], &mut [&mut out_block.data]);
                let _ = out_rb.push(out_block);
            }
            thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    // 3. Setup Engine and Backend
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();

    let engine = AudioEngine::new(cons, garbage_prod, Box::new(ProcessorChain::new()));

    let sidecar_proxy = unsafe { SidecarProcessor::new(cmd_rb_ptr, &[in_rb_ptr], &[out_rb_ptr]) };
    engine.request_swap({
        let mut g = Box::new(ProcessorChain::new());
        g.add(Box::new(sidecar_proxy));
        g
    });

    let mut backend = ThreadedBackend::new();
    println!("Engine: Starting ThreadedBackend...");
    backend.start(engine).unwrap();

    thread::sleep(std::time::Duration::from_millis(100));

    println!("Engine: Simulation finished.");
    backend.stop();
}
