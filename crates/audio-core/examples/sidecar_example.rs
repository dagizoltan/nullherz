use audio_core::{AudioEngine, AudioProcessor, SidecarProcessor};
use control_plane::{Command, TimestampedCommand};
use ipc_layer::{RingBuffer, ShmRingBuffer};
use sidecar_sdk::SidecarContext;
use std::thread;

struct MockSidecarProcessor;
impl AudioProcessor for MockSidecarProcessor {
    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]]) {}
    fn apply_command(&mut self, command: &Command) {
        if let Command::Play = command {
            println!("Sidecar: Received Play command!");
        }
    }
}

// Wrapper to allow passing pointers across threads for this example
#[derive(Copy, Clone)]
struct ShmPtr(usize);
unsafe impl Send for ShmPtr {}

fn main() {
    // 1. Setup Shared Memory for Sidecar Commands
    let capacity = 64;
    let size = ShmRingBuffer::<Command>::size_required(capacity);
    let shm_mem = Box::leak(vec![0u8; size].into_boxed_slice());
    let shm_rb_ptr = unsafe { ShmRingBuffer::<Command>::init(shm_mem.as_mut_ptr(), capacity) };
    let shm_ptr_wrapper = ShmPtr(shm_rb_ptr as usize);

    // 2. Start Sidecar Thread (simulating another process)
    let _sidecar_handle = thread::spawn(move || {
        let ptr = shm_ptr_wrapper.0 as *const ShmRingBuffer<Command>;
        let mut context = unsafe {
            SidecarContext::new(MockSidecarProcessor, ptr)
        };
        println!("Sidecar: Started loop");
        // Run for a bit and exit for the example
        for _ in 0..10 {
            context.process_once();
            thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // 3. Setup Engine
    let rb = RingBuffer::new(1024);
    let (mut prod, cons) = rb.split();
    let mut engine = AudioEngine::new(cons);

    // Add SidecarProcessor proxy
    let sidecar_proxy = unsafe { SidecarProcessor::new(shm_rb_ptr) };
    engine.add_processor(Box::new(sidecar_proxy));

    println!("Engine: Sending Play command...");
    prod.push(TimestampedCommand {
        timestamp_samples: 0,
        command: Command::Play,
    }).unwrap();

    // Run engine block
    let mut out_buffer = [0.0f32; 128];
    let mut out_ptrs = [&mut out_buffer[..]];
    engine.process_block(&[], &mut out_ptrs, 128);

    thread::sleep(std::time::Duration::from_millis(50));
    println!("Engine: Simulation finished.");
}
