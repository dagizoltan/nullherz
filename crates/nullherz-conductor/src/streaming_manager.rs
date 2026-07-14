use std::sync::Arc;
use ipc_layer::ShmRingBuffer;
use std::thread;
use std::collections::HashMap;

pub struct StreamingManager {
    streams: HashMap<u64, Arc<ShmRingBuffer<f32>>>,
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
            is_streaming: false,
            start_time: None,
            bitrate: 256.0,
            dropped_frames: 0,
            viewer_count: 42,
        }
    }

    pub fn start_stream(&mut self, id: u64, path: String, ring_buffer: Arc<ShmRingBuffer<f32>>) {
        self.streams.insert(id, ring_buffer.clone());
        self.is_streaming = true;
        self.start_time = Some(std::time::Instant::now());

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

                            while let Ok(packet) = probed.format.next_packet() {
                                if Arc::strong_count(&ring_buffer) == 1 {
                                    break;
                                }

                                if let Ok(decoded) = decoder.decode(&packet) {
                                    if sample_buf.is_none() {
                                        sample_buf = Some(symphonia::core::audio::AudioBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec()));
                                    }
                                    let buf = sample_buf.as_mut().unwrap();
                                    decoded.convert(buf);

                                    let chan_len = buf.frames();
                                    let num_chans = buf.spec().channels.count();
                                    for i in 0..chan_len {
                                        if Arc::strong_count(&ring_buffer) == 1 {
                                            break;
                                        }

                                        let mut sample = 0.0;
                                        for c in 0..num_chans {
                                            sample += buf.chan(c)[i];
                                        }
                                        sample /= num_chans as f32;

                                        // Poll ring-buffer capacity and write next chunk
                                        while let Err(_failed_sample) = ring_buffer.push(sample) {
                                            if Arc::strong_count(&ring_buffer) == 1 {
                                                break;
                                            }
                                            thread::sleep(std::time::Duration::from_millis(5));
                                        }
                                    }
                                }
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
        self.streams.clear();
    }
}
