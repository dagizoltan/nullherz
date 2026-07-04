use std::collections::HashMap;
use std::sync::Arc;
use ipc_layer::{SharedMemory, ShmRingBuffer, AudioBlock, IpcAudioProducer, IpcAudioConsumer};
use std::sync::Mutex;

pub struct IpcAudioBridge {
    /// Maps node_idx to its audio return queue.
    pub return_queues: Arc<Mutex<HashMap<u32, IpcAudioProducer>>>,
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
        let queues = self.return_queues.lock().unwrap();
        if let Some(producer) = queues.get(&node_idx) {
            producer.push(block)
        } else {
            Err(block)
        }
    }

    pub fn unregister_return_node(&self, node_idx: u32) {
        self.return_queues.lock().unwrap().remove(&node_idx);
        self.shm_segments.lock().unwrap().remove(&node_idx);
    }
}
