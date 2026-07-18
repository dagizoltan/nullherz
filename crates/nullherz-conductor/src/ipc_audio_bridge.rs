use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use ipc_layer::{SharedMemory, ShmRingBuffer, AudioBlock, IpcAudioProducer, IpcAudioConsumer};
use parking_lot::Mutex;

pub struct JitterBuffer {
    pub buffer: VecDeque<AudioBlock>,
    pub target_size: usize,
    pub drift_accumulator: f32,
    pub last_drain_time: std::time::Instant,
    pub clock: Option<Arc<dyn nullherz_traits::ClockProvider>>,
    // STAGE 8: Adaptive Statistics
    pub arrival_times: VecDeque<std::time::Instant>,
    pub stats_update_counter: usize,
}

impl JitterBuffer {
    pub fn new(target_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(target_size * 8),
            target_size,
            drift_accumulator: 0.0,
            last_drain_time: std::time::Instant::now(),
            clock: None,
            arrival_times: VecDeque::with_capacity(32),
            stats_update_counter: 0,
        }
    }

    pub fn push(&mut self, block: AudioBlock) {
        if self.buffer.len() < self.target_size * 8 {
            self.buffer.push_back(block);
        }

        // Track arrival for adaptive jitter calculation
        let now = std::time::Instant::now();
        self.arrival_times.push_back(now);
        if self.arrival_times.len() > 32 { self.arrival_times.pop_front(); }

        self.stats_update_counter += 1;
        if self.stats_update_counter >= 32 {
            self.update_adaptive_target();
            self.stats_update_counter = 0;
        }
    }

    fn update_adaptive_target(&mut self) {
        let n = self.arrival_times.len();
        if n < 2 { return; }

        let mut sum_x = 0.0;
        let mut sum_x2 = 0.0;
        let count = n - 1;

        for i in 1..n {
            let x = self.arrival_times[i].duration_since(self.arrival_times[i-1]).as_secs_f32();
            sum_x += x;
            sum_x2 += x * x;
        }

        let mean = sum_x / count as f32;
        let variance = (sum_x2 / count as f32) - (mean * mean);
        let std_dev = variance.max(0.0).sqrt();

        // Rule: Target size should be enough to cover 3 standard deviations of jitter.
        // Assuming block size is ~5ms (256 samples @ 48k).
        let block_duration = 256.0 / 44100.0;
        let jitter_blocks = (std_dev * 3.0 / block_duration).ceil() as usize;

        self.target_size = jitter_blocks.clamp(2, 16);
    }

    pub fn pop(&mut self) -> Option<AudioBlock> {
        // --- HARDENED CLOCK RECOVERY (Disciplined drift compensation) ---
        let current_len = self.buffer.len();

        let mut drift_adjustment = 0.05;
        if let Some(ref clock) = self.clock {
            let jitter = clock.get_estimated_jitter_ns();
            // If jitter is high, slow down adjustments to avoid oscillation
            if jitter > 10_000 { drift_adjustment = 0.01; }
        }

        // Target: maintain buffer at exactly target_size.
        // If > 2x target_size, we are falling behind.
        if current_len > self.target_size * 2 {
             self.drift_accumulator += drift_adjustment;
             if self.drift_accumulator > 1.0 {
                 self.drift_accumulator -= 1.0;
                 let _ = self.buffer.pop_front(); // Fast-forward one block
             }
        } else if current_len < self.target_size {
             self.drift_accumulator -= drift_adjustment;
             if self.drift_accumulator < -1.0 {
                 self.drift_accumulator += 1.0;
                 return None; // Insert silence by returning nothing
             }
        }

        if current_len >= self.target_size {
            self.buffer.pop_front()
        } else {
            None
        }
    }
}

/// Simulated InfiniBand/RDMA transport layer for high-density multi-machine studio setups.
/// Bypasses CPU processing using direct memory regions (MR) and low-latency queue pairs (QP).
pub struct RdmaMemoryRegion {
    pub addr: u64,
    pub size: usize,
    pub rkey: u32,
}

pub struct RdmaQueuePair {
    pub local_qp_num: u32,
    pub remote_qp_num: u32,
    pub state: String,
}

pub struct RdmaTransport {
    pub memory_regions: Mutex<HashMap<u32, RdmaMemoryRegion>>,
    pub active_qps: Mutex<HashMap<u32, RdmaQueuePair>>,
}

impl RdmaTransport {
    pub fn new() -> Self {
        Self {
            memory_regions: Mutex::new(HashMap::new()),
            active_qps: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_memory_region(&self, node_idx: u32, addr: u64, size: usize) -> u32 {
        let rkey = node_idx.wrapping_mul(777) + 1234;
        let mut mrs = self.memory_regions.lock();
        mrs.insert(node_idx, RdmaMemoryRegion { addr, size, rkey });
        rkey
    }

    pub fn connect_qp(&self, node_idx: u32, remote_qp: u32) {
        let mut qps = self.active_qps.lock();
        qps.insert(node_idx, RdmaQueuePair {
            local_qp_num: node_idx * 10 + 1,
            remote_qp_num: remote_qp,
            state: "RTS".to_string(), // Ready to Send
        });
    }

    /// Performs zero-copy bypass-CPU direct transfer of raw audio blocks into local memory region.
    ///
    /// # Safety
    /// The registered memory region for `node_idx` must outlive this call and
    /// must not be concurrently written by the remote peer; the caller must
    /// ensure the queue pair was established for this region (RDMA writes
    /// bypass all CPU-side bounds and aliasing checks).
    pub unsafe fn rdma_write_block(&self, node_idx: u32, block: &AudioBlock) -> Result<(), &'static str> {
        let mrs = self.memory_regions.lock();
        let qps = self.active_qps.lock();

        if !qps.contains_key(&node_idx) {
            return Err("RDMA Error: Queue Pair not connected");
        }

        if let Some(mr) = mrs.get(&node_idx) {
            if mr.size < std::mem::size_of::<AudioBlock>() {
                return Err("RDMA Error: Target memory region too small");
            }
            let dest_ptr = mr.addr as *mut AudioBlock;
            unsafe {
                std::ptr::copy_nonoverlapping(block as *const AudioBlock, dest_ptr, 1);
            }
            Ok(())
        } else {
            Err("RDMA Error: Memory Region not registered")
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
    /// High-performance RDMA transport layer.
    pub rdma_transport: RdmaTransport,
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
            rdma_transport: RdmaTransport::new(),
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

        self.return_queues.lock().insert(node_idx, producer);
        self.shm_segments.lock().insert(node_idx, shm);

        Ok(consumer)
    }

    pub fn push_block(&self, node_idx: u32, block: AudioBlock) -> Result<(), AudioBlock> {
        // Apply jitter buffering for remote blocks (identified by being pushed via discovery listener)
        let mut jitters = self.jitter_buffers.lock();
        let buffer = jitters.entry(node_idx).or_insert_with(|| JitterBuffer::new(4));
        buffer.push(block);
        Ok(())
    }

    pub fn set_clock(&self, node_idx: u32, clock: Arc<dyn nullherz_traits::ClockProvider>) {
        let mut jitters = self.jitter_buffers.lock();
        if let Some(buffer) = jitters.get_mut(&node_idx) {
            buffer.clock = Some(clock);
        }
    }

    pub fn pop_block(&self, node_idx: u32) -> Option<AudioBlock> {
        let mut queues = self.send_queues.lock();
        queues.get_mut(&node_idx).and_then(|consumer| consumer.pop())
    }

    pub fn process_return_queues(&self) {
        let mut jitters = self.jitter_buffers.lock();
        let queues = self.return_queues.lock();

        for (&node_idx, buffer) in jitters.iter_mut() {
            if let Some(producer) = queues.get(&node_idx) {
                while let Some(block) = buffer.pop() {
                    if producer.push(block).is_err() { break; }
                }
            }
        }
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

        self.send_queues.lock().insert(node_idx, consumer);
        self.shm_segments.lock().insert(node_idx, shm);

        Ok(producer)
    }

    pub fn unregister_return_node(&self, node_idx: u32) {
        self.return_queues.lock().remove(&node_idx);
        self.shm_segments.lock().remove(&node_idx);
    }
}

#[cfg(all(feature = "kani-verify", kani))]
mod verification {
    use super::*;
    use ipc_layer::AudioBlock;

    #[kani::proof]
    #[kani::unwind(10)]
    pub fn prove_jitter_buffer_no_panic() {
        let mut jb = JitterBuffer::new(2);
        let block = AudioBlock { data: [0.0; 256], len: 256, _pad: [0; 15] };

        // Symbols for symbolic execution
        let num_pushes: usize = kani::any_where(|&n: &usize| n < 10);
        let num_pops: usize = kani::any_where(|&n: &usize| n < 10);

        for _ in 0..num_pushes {
            jb.push(block);
        }

        for _ in 0..num_pops {
            jb.pop();
        }

        kani::assert(jb.target_size == 2, "Target size must remain constant");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jitter_buffer_flow() {
        let mut jb = JitterBuffer::new(2);
        let block = AudioBlock { data: [0.0; 256], len: 256, _pad: [0; 15] };

        jb.push(block);
        assert!(jb.pop().is_none()); // Should wait for target_size=2

        jb.push(block);
        let popped = jb.pop();
        assert!(popped.is_some());
        assert_eq!(popped.unwrap().len, 256);
    }

    #[test]
    fn test_jitter_buffer_drift_compensation() {
        let mut jb = JitterBuffer::new(2);
        let block = AudioBlock { data: [0.0; 256], len: 256, _pad: [0; 15] };

        // Overfill buffer to trigger drift compensation
        for _ in 0..10 {
            jb.push(block);
        }

        let initial_drift = jb.drift_accumulator;
        let _ = jb.pop();
        assert!(jb.drift_accumulator > initial_drift);
    }
}
