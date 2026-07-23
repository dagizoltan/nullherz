// Non-RT plane (disk-streaming worker thread): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use std::sync::Arc;
use ipc_layer::ShmRingBuffer;
use std::thread;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct StreamingManager {
    streams: HashMap<u64, Arc<ShmRingBuffer<f32>>>,
    shutdown_signals: HashMap<u64, Arc<AtomicBool>>,
    pub is_streaming: bool,
    pub start_time: Option<std::time::Instant>,
    pub bitrate: f32,
    pub dropped_frames: u32,
    pub viewer_count: u32,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
            shutdown_signals: HashMap::new(),
            is_streaming: false,
            start_time: None,
            bitrate: 256.0,
            dropped_frames: 0,
            viewer_count: 42,
        }
    }

    pub fn start_stream(&mut self, id: u64, path: String, ring_buffer: Arc<ShmRingBuffer<f32>>) {
        let shutdown = Arc::new(AtomicBool::new(false));
        self.streams.insert(id, ring_buffer.clone());
        self.shutdown_signals.insert(id, shutdown.clone());
        self.is_streaming = true;
        self.start_time = Some(std::time::Instant::now());

        // Create a bounded channel for double-buffered blocks of samples (each block is e.g. 1024 samples)
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(64);

        // 1. Spawn high-priority feeder thread to drain intermediate channel and fill shared-memory ring buffer
        // Uses Weak reference to ShmRingBuffer to avoid circular strong count leaks when the consumer is dropped,
        // and drops upgraded strong reference `rb` during buffer-full sleep to prevent strong count locks.
        let ring_buffer_weak = Arc::downgrade(&ring_buffer);
        let shutdown_feeder = shutdown.clone();
        thread::spawn(move || {
            while let Ok(block) = rx.recv() {
                if shutdown_feeder.load(Ordering::Relaxed) {
                    break;
                }
                for &sample in &block {
                    loop {
                        if shutdown_feeder.load(Ordering::Relaxed) {
                            return;
                        }
                        if ring_buffer_weak.strong_count() <= 1 {
                            return;
                        }
                        if let Some(rb) = ring_buffer_weak.upgrade() {
                            match rb.push(sample) {
                                Ok(_) => break, // Successfully pushed
                                Err(_) => {
                                    // Ring buffer is full. Drop `rb` so the strong count doesn't include us while sleeping
                                    drop(rb);
                                }
                            }
                        } else {
                            return; // Consumer dropped
                        }
                        thread::sleep(std::time::Duration::from_millis(2));
                    }
                }
            }
        });

        // 2. Spawn disk decoder thread to run file reading and symphonia decoding
        // Decoder thread holds a Weak reference to ShmRingBuffer to accurately detect when the consumer releases its Arc
        let ring_buffer_weak = Arc::downgrade(&ring_buffer);
        let shutdown_decoder = shutdown.clone();
        thread::spawn(move || {
            use symphonia::core::audio::Signal;

            // STAGE 8 High-Performance Disk Streaming
            // Utilizes background threads for lock-free ring-buffer pre-filling.
            if let Ok(file) = std::fs::File::open(&path) {
                let mss = symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());
                let hint = symphonia::core::probe::Hint::new();

                if let Ok(probed_res) = symphonia::default::get_probe().format(&hint, mss, &Default::default(), &Default::default()) {
                    let mut probed = probed_res;
                    if let Some(track) = probed.format.default_track() {
                        if let Ok(mut decoder) = symphonia::default::get_codecs().make(&track.codec_params, &Default::default()) {
                            let mut sample_buf = None;
                            let mut current_block = Vec::with_capacity(1024);

                            while let Ok(packet) = probed.format.next_packet() {
                                if shutdown_decoder.load(Ordering::Relaxed) || ring_buffer_weak.strong_count() <= 1 {
                                    break;
                                }

                                if let Ok(decoded) = decoder.decode(&packet) {
                                    let buf = sample_buf.get_or_insert_with(|| symphonia::core::audio::AudioBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec()));
                                    decoded.convert(buf);

                                    let chan_len = buf.frames();
                                    let num_chans = buf.spec().channels.count();
                                    for i in 0..chan_len {
                                        if shutdown_decoder.load(Ordering::Relaxed) || ring_buffer_weak.strong_count() <= 1 {
                                            break;
                                        }

                                        let mut sample = 0.0;
                                        for c in 0..num_chans {
                                            sample += buf.chan(c)[i];
                                        }
                                        sample /= num_chans as f32;

                                        current_block.push(sample);
                                        if current_block.len() >= 1024 {
                                            let block_to_send = std::mem::replace(&mut current_block, Vec::with_capacity(1024));
                                            if tx.send(block_to_send).is_err() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            // Send any remaining samples in the final partial block
                            if !current_block.is_empty() {
                                let _ = tx.send(current_block);
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn stop_stream(&mut self) {
        self.is_streaming = false;
        self.start_time = None;
        for sig in self.shutdown_signals.values() {
            sig.store(true, Ordering::Relaxed);
        }
        self.streams.clear();
        self.shutdown_signals.clear();
    }
}

impl Drop for StreamingManager {
    fn drop(&mut self) {
        self.stop_stream();
    }
}
