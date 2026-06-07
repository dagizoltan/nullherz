use audio_core::{AudioEngine, AudioProcessor, SidecarProcessor, ProcessorGraph, ThreadedBackend, AudioBackend};
use control_plane::{Command};
use ipc_layer::{RingBuffer, ShmRingBuffer, AudioBlock, SharedMemory, ShmSignal};
use std::thread;

struct MockSidecarProcessor;
impl AudioProcessor for MockSidecarProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for i in 0..inputs.len().min(outputs.len()) {
            for j in 0..inputs[i].len() { outputs[i][j] = inputs[i][j] * 0.5; }
        }
    }
    fn apply_command(&mut self, command: &Command) {
        if let Command::Play = command { println!("Sidecar: Received Play command!"); }
    }
}

fn main() {
    // 1. Setup SHM
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

    let sig_shm = SharedMemory::create("nullherz_sig", 64).unwrap();
    let sig_ptr = sig_shm.ptr() as *mut ShmSignal;
    unsafe { std::ptr::write(sig_ptr, ShmSignal::new()); }

    // 2. Start Sidecar Thread
    let _sidecar_handle = thread::spawn(move || {
        let cmd_shm_side = SharedMemory::open("nullherz_cmd", cmd_layout.size()).unwrap();
        let in_shm_side = SharedMemory::open("nullherz_in_0", audio_layout.size()).unwrap();
        let out_shm_side = SharedMemory::open("nullherz_out_0", audio_layout.size()).unwrap();
        let sig_shm_side = SharedMemory::open("nullherz_sig", 64).unwrap();
        let signal = unsafe { &*(sig_shm_side.ptr() as *const ShmSignal) };

        let mut processor = MockSidecarProcessor;
        println!("Sidecar: Started loop");
        for _ in 0..200 {
            if signal.check_and_clear() {
                let cmd_rb = unsafe { &*(cmd_shm_side.ptr() as *const ShmRingBuffer<Command>) };
                while let Some(cmd) = cmd_rb.pop() { processor.apply_command(&cmd); }
                let in_rb = unsafe { &*(in_shm_side.ptr() as *const ShmRingBuffer<AudioBlock>) };
                let out_rb = unsafe { &mut *(out_shm_side.ptr() as *mut ShmRingBuffer<AudioBlock>) };
                if let Some(in_block) = in_rb.pop() {
                    let mut out_block = AudioBlock { data: [0.0; 128] };
                    processor.process(&[&in_block.data], &mut [&mut out_block.data]);
                    let _ = out_rb.push(out_block);
                }
            }
            thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    // 3. Setup Engine
    let rb = RingBuffer::new(1024);
    let (_, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, mut tel_cons) = tel_rb.split();

    let mut graph = ProcessorGraph::new();
    let sidecar_proxy = unsafe { SidecarProcessor::new(cmd_rb_ptr, None, &[in_rb_ptr], &[out_rb_ptr], sig_ptr, None) };
    graph.add_node(Box::new(sidecar_proxy), vec![], vec![0]);

    let engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

    let mut backend = ThreadedBackend::new();
    backend.start(engine).unwrap();
    thread::sleep(std::time::Duration::from_millis(100));

    while let Some(t) = tel_cons.pop() {
        if t.sample_counter < 1000 { println!("Telemetry: process_time_ns={}", t.process_time_ns); }
    }

    println!("Engine: Simulation finished.");
    backend.stop();
}
