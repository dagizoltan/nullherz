use std::collections::HashMap;
use std::sync::Arc;
use ipc_layer::{SharedMemory, ShmRingBuffer, AudioBlock, IpcAudioProducer, IpcAudioConsumer};
use std::sync::Mutex;

pub struct JitterBuffer {
    pub buffer: Vec<AudioBlock>,
    pub target_size: usize,
    pub drift_accumulator: f32,
    pub last_drain_time: std::time::Instant,
}

impl JitterBuffer {
    pub fn new(target_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(target_size * 8),
            target_size,
            drift_accumulator: 0.0,
            last_drain_time: std::time::Instant::now(),
        }
    }

    pub fn push(&mut self, block: AudioBlock) {
        if self.buffer.len() < self.target_size * 8 {
            self.buffer.push(block);
        }
    }

    pub fn pop(&mut self) -> Option<AudioBlock> {
        // --- BASIC CLOCK RECOVERY ---
        // If the buffer is getting too full, we decrease target size slightly (speed up drain)
        // If the buffer is too empty, we increase target size (slow down drain)
        let current_len = self.buffer.len();

        if current_len > self.target_size * 3 {
             self.drift_accumulator += 0.1; // Speed up
        } else if current_len < self.target_size / 2 {
             self.drift_accumulator -= 0.1; // Slow down
        }

        if current_len >= self.target_size {
            Some(self.buffer.remove(0))
        } else {
            None
        }
    }
}

pub struct IpcAudioBridge {
    /// Maps node_idx to its audio return queue.
    pub return_queues: Arc<Mutex<HashMap<u32, IpcAudioProducer>>>,
    /// Maps node_idx to its audio send queue (local -> remote).
    pub send_queues: Arc<Mutex<HashMap<u32, IpcAudioConsumer>>>,
    /// Jitter buffers for remote return paths.
    pub jitter_buffers: Arc<Mutex<HashMap<u32, JitterBuffer>>>,
    /// SHM segments owned by the bridge.
    pub shm_segments: Arc<Mutex<HashMap<u32, Arc<SharedMemory>>>>,
}

impl Default for IpcAudioBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcAudioBridge {
    pub fn new() -> Self {
        Self {
            return_queues: Arc::new(Mutex::new(HashMap::new())),
            send_queues: Arc::new(Mutex::new(HashMap::new())),
            jitter_buffers: Arc::new(Mutex::new(HashMap::new())),
            shm_segments: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_return_node(&self, node_idx: u32) -> Result<IpcAudioConsumer, Box<dyn std::error::Error>> {
        let shm_name = format!("nullherz_audio_return_{}", node_idx);
        let (layout, _) = ShmRingBuffer::<AudioBlock>::layout(32);

        let shm = Arc::new(SharedMemory::create(&shm_name, layout.size())?);
        let rb_ptr = unsafe { ShmRingBuffer::<AudioBlock>::init(shm.ptr(), 32) };

        let producer = IpcAudioProducer {
            buffer: shm.clone(),
            rb: rb_ptr,
        };

        let consumer = IpcAudioConsumer {
            buffer: shm.clone(),
            rb: rb_ptr,
        };

        self.return_queues.lock().unwrap().insert(node_idx, producer);
        self.shm_segments.lock().unwrap().insert(node_idx, shm);

        Ok(consumer)
    }

    pub fn push_block(&self, node_idx: u32, block: AudioBlock) -> Result<(), AudioBlock> {
        // Apply jitter buffering for remote blocks (identified by being pushed via discovery listener)
        let mut jitters = self.jitter_buffers.lock().unwrap();
        let buffer = jitters.entry(node_idx).or_insert_with(|| JitterBuffer::new(4));
        buffer.push(block);

        // Periodically drain from jitter buffer to SHM return queue
        if let Some(buffered_block) = buffer.pop() {
            let queues = self.return_queues.lock().unwrap();
            if let Some(producer) = queues.get(&node_idx) {
                producer.push(buffered_block)
            } else {
                Err(buffered_block)
            }
        } else {
            Ok(())
        }
    }

    pub fn pop_block(&self, node_idx: u32) -> Option<AudioBlock> {
        let mut queues = self.send_queues.lock().unwrap();
        queues.get_mut(&node_idx).and_then(|consumer| consumer.pop())
    }

    pub fn register_send_node(&self, node_idx: u32) -> Result<IpcAudioProducer, Box<dyn std::error::Error>> {
        let shm_name = format!("nullherz_audio_send_{}", node_idx);
        let (layout, _) = ShmRingBuffer::<AudioBlock>::layout(32);

        let shm = Arc::new(SharedMemory::create(&shm_name, layout.size())?);
        let rb_ptr = unsafe { ShmRingBuffer::<AudioBlock>::init(shm.ptr(), 32) };

        let producer = IpcAudioProducer {
            buffer: shm.clone(),
            rb: rb_ptr,
        };

        let consumer = IpcAudioConsumer {
            buffer: shm.clone(),
            rb: rb_ptr,
        };

        self.send_queues.lock().unwrap().insert(node_idx, consumer);
        self.shm_segments.lock().unwrap().insert(node_idx, shm);

        Ok(producer)
    }

    pub fn unregister_return_node(&self, node_idx: u32) {
        self.return_queues.lock().unwrap().remove(&node_idx);
        self.shm_segments.lock().unwrap().remove(&node_idx);
    }
}
