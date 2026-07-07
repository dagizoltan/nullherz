use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer, SharedMemory, ShmRingBuffer, ShmSignal, EventFd, AudioBlock};
use nullherz_traits::{AudioProcessor, Command};
use audio_core::{AudioEngine, ProcessorGraph};
use nullherz_processors::SidecarProcessor;

fn main() {
    let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

    let graph = ProcessorGraph::new();

    // 1. Setup SHM for sidecar simulation
    let _name = "example";
    let cmd_shm_name = "/nullherz_cmd_example";
    let (cmd_layout, _) = ShmRingBuffer::<Command>::layout(64);
    let shm_cmd = SharedMemory::create(cmd_shm_name, cmd_layout.size()).unwrap();
    let cmd_rb_ptr = unsafe { ShmRingBuffer::init(shm_cmd.ptr(), 64) };

    let fb_shm_name = "/nullherz_fb_example";
    let (fb_layout, _) = ShmRingBuffer::<nullherz_traits::ProcessorMetadata>::layout(8);
    let shm_fb = SharedMemory::create(fb_shm_name, fb_layout.size()).unwrap();
    let fb_rb_ptr = unsafe { ShmRingBuffer::init(shm_fb.ptr(), 8) };

    let sig_name = "/nullherz_sig_example";
    let shm_sig = SharedMemory::create(sig_name, std::mem::size_of::<ShmSignal>()).unwrap();
    let sig_ptr = shm_sig.ptr() as *mut ShmSignal;
    unsafe { std::ptr::write(sig_ptr, ShmSignal::new()); }

    let efd = EventFd::create().unwrap();

    // 2. Create the SidecarProcessor
    // We simulate a single channel for simplicity
    let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);

    let in_shm = SharedMemory::create("/nullherz_in_example_0", audio_layout.size()).unwrap();
    let in_ptr = unsafe { ShmRingBuffer::init(in_shm.ptr(), 16) };
    let in_ptrs = [in_ptr];

    let out_shm = SharedMemory::create("/nullherz_out_example_0", audio_layout.size()).unwrap();
    let out_ptr = unsafe { ShmRingBuffer::init(out_shm.ptr(), 16) };
    let out_ptrs = [out_ptr as *const ShmRingBuffer<AudioBlock>];

    let _sidecar = unsafe {
        SidecarProcessor::new(
            cmd_rb_ptr,
            Some(fb_rb_ptr),
            &in_ptrs,
            &out_ptrs,
            sig_ptr,
            Some(efd)
        )
    };

    println!("Initialized SidecarProcessor SHM segments.");

    let cmd_buffer = Arc::new(MpscRingBuffer::new(1024));

    let resources = audio_core::engine::EngineResources {
        command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
        command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
        midi_consumer: None,
        bundle_consumer: None,
        topology_consumer: None,
        garbage_producer: garbage_prod,
        overflow_garbage_producer: None,
        bundle_garbage_producer: None,
        bundle_overflow_producer: None,
        telemetry_producer: Box::new(tel_prod),
        worker_count: None,
    };

    let _engine = AudioEngine::new(
        resources,
        Box::new(graph),
        Arc::new(audio_core::rt_logging::RtLogger::new(256)),
        audio_core::engine::processing_kernel::StandardKernel::default()
    );

    println!("Engine with decoupled traits initialized successfully.");
}
