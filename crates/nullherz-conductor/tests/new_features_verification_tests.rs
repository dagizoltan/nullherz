use std::sync::Arc;
use nullherz_backends::threaded::ThreadedBackend;

#[test]
fn test_threaded_backend_xrun_detection() {
    let tb = ThreadedBackend::new();
    // Verify initial xrun state is 0
    assert_eq!(tb.xrun_counter.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn test_streaming_sampler_registry() {
    let capacity = 1024;
    let size = capacity * 4 + 256;
    if let Ok(shm) = ipc_layer::SharedMemory::create("test_streaming_shm_reg", size) {
        let shm_arc = Arc::new(shm);
        nullherz_processors::streaming_sampler::register_streaming_buffer(999, shm_arc.clone());
        let retrieved = nullherz_processors::streaming_sampler::get_streaming_buffer(999);
        assert!(retrieved.is_some());
        assert_eq!(Arc::strong_count(&shm_arc), 2); // registry holds a Weak ref, but we retrieved it!
        drop(retrieved);
        assert_eq!(Arc::strong_count(&shm_arc), 1); // Only we hold it now!
    }
}
