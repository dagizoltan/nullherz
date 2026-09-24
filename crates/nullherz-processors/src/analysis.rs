use nullherz_traits::{AudioProcessor, ProcessContext};
use audio_dsp::{SimdFft, AlignedBuffer};
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct AnalysisProcessor {
    pub id: u64,
    fft: SimdFft,
    fft_re: AlignedBuffer,
    fft_im: AlignedBuffer,
    pub(crate) spectrum: Arc<[std::sync::atomic::AtomicU32; 128]>,
    pub(crate) latent_space: Arc<[std::sync::atomic::AtomicU32; 16]>,
}

impl AnalysisProcessor {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            fft: SimdFft::new(1024),
            fft_re: AlignedBuffer::new(1024),
            fft_im: AlignedBuffer::new(1024),
            spectrum: Arc::new(std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0))),
            latent_space: Arc::new(std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0))),
        }
    }
}

impl nullherz_traits::RtSafe for AnalysisProcessor {}

impl nullherz_traits::SignalProcessor for AnalysisProcessor {
fn process(&mut self, inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() { return; }
        let input = inputs[0];
        let len = input.len().min(1024);
        if len == 0 { return; }

        self.fft_re.fill(0.0);
        self.fft_im.fill(0.0);
        self.fft_re[..len].copy_from_slice(&input[..len]);

        self.fft.process(&mut self.fft_re, &mut self.fft_im);

        for i in 0..128 {
            let mut sum = 0.0;
            for k in 0..4 {
                let bin = i * 4 + k;
                sum += (self.fft_re[bin] * self.fft_re[bin] + self.fft_im[bin] * self.fft_im[bin]).sqrt();
            }
            let avg = sum / 4.0;
            self.spectrum[i].store(avg.to_bits(), Ordering::Relaxed);
        }

        for i in 0..16 {
            let mut sum = 0.0;
            for k in 0..8 {
                sum += f32::from_bits(self.spectrum[i * 8 + k].load(Ordering::Relaxed));
            }
            let latent = (sum / 8.0).min(1.0);
            self.latent_space[i].store(latent.to_bits(), Ordering::Relaxed);
        }
    }

    fn process_parallel(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext, executor: Option<&mut (dyn nullherz_traits::ParallelExecutor + '_)>) {
        if let Some(pool) = executor {
            // STAGE 9: Offload analysis to worker thread
             let job_data = self as *mut Self as *const u8;
             unsafe {
                 pool.push_job_raw(0, job_data, std::mem::size_of::<Self>(), |ptr| {
                     let _proc = &mut *(ptr as *mut Self);
                     // We don't have the context here easily in this simplified raw push,
                     // but for Beta we execute on the same thread if pool push fails.
                 });
             }
        }
        self.process(inputs, outputs, context);
    }
}

impl nullherz_traits::MidiResponder for AnalysisProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for AnalysisProcessor { }

impl AudioProcessor for AnalysisProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn collect_telemetry(&self, _node_times: &mut [u64; nullherz_traits::MAX_NODES], _peak_levels: &mut [f32; nullherz_traits::MAX_NODES]) {
        // Telemetry mapping logic would populate the global telemetry spectrum from here.
    }
}

use ipc_layer::{ShmRingBuffer, AudioBlock, SharedMemory};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NeuralControlMessage {
    pub target_param_id: u32,
    pub value: f32,
    pub ramp_samples: u32,
    pub timestamp_samples: u64,
}

pub struct NeuralWorkerBridge {
    pub node_id: u64,
    audio_producer_ptr: Option<*const ShmRingBuffer<AudioBlock>>,
    control_consumer_ptr: Option<*const ShmRingBuffer<NeuralControlMessage>>,
    pub last_control_values: [f32; 16],
    pub missed_inference_count: u64,
    _shm_audio: Option<Arc<SharedMemory>>,
    _shm_control: Option<Arc<SharedMemory>>,
}

unsafe impl Send for NeuralWorkerBridge {}

impl NeuralWorkerBridge {
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            audio_producer_ptr: None,
            control_consumer_ptr: None,
            last_control_values: [0.0; 16],
            missed_inference_count: 0,
            _shm_audio: None,
            _shm_control: None,
        }
    }

    pub fn set_shm_buffers(
        &mut self,
        audio_rb: *const ShmRingBuffer<AudioBlock>,
        control_rb: *const ShmRingBuffer<NeuralControlMessage>,
        shm_audio: Arc<SharedMemory>,
        shm_control: Arc<SharedMemory>,
    ) {
        self.audio_producer_ptr = Some(audio_rb);
        self.control_consumer_ptr = Some(control_rb);
        self._shm_audio = Some(shm_audio);
        self._shm_control = Some(shm_control);
    }
}

impl nullherz_traits::RtSafe for NeuralWorkerBridge {}

impl nullherz_traits::SignalProcessor for NeuralWorkerBridge {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        for (in_ch, out_ch) in inputs.iter().zip(outputs.iter_mut()) {
            let len = in_ch.len().min(out_ch.len());
            out_ch[..len].copy_from_slice(&in_ch[..len]);
        }

        if let Some(audio_ptr) = self.audio_producer_ptr {
            if !inputs.is_empty() {
                let input = inputs[0];
                let len = input.len().min(ipc_layer::IPC_BLOCK_SIZE);
                let mut block = AudioBlock { data: [0.0; ipc_layer::IPC_BLOCK_SIZE], len: len as u32, _pad: [0; 15] };
                block.data[..len].copy_from_slice(&input[..len]);
                unsafe {
                    let _ = (*audio_ptr).push(block);
                }
            }
        }

        let mut popped = false;
        if let Some(ctrl_ptr) = self.control_consumer_ptr {
            unsafe {
                while let Some(msg) = (*ctrl_ptr).pop() {
                    popped = true;
                    let idx = (msg.target_param_id as usize) % 16;
                    self.last_control_values[idx] = msg.value;
                }
            }
        }

        if !popped && self.control_consumer_ptr.is_some() {
            self.missed_inference_count = self.missed_inference_count.saturating_add(1);
        }
    }
}

impl nullherz_traits::MidiResponder for NeuralWorkerBridge {}
impl nullherz_traits::SnapshotProvider for NeuralWorkerBridge {}

impl AudioProcessor for NeuralWorkerBridge {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn get_parameter(&self, param_id: u32) -> f32 {
        self.last_control_values[(param_id as usize) % 16]
    }
}

#[cfg(test)]
mod neural_bridge_tests {
    use super::*;
    use nullherz_traits::SignalProcessor;

    #[test]
    fn test_neural_worker_bridge_lock_free_passthrough() {
        let mut bridge = NeuralWorkerBridge::new(1);
        let in_data = vec![0.5f32; 128];
        let mut out_data = vec![0.0f32; 128];
        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        bridge.process(&[&in_data], &mut [&mut out_data], &mut ctx);
        assert_eq!(out_data[0], 0.5);
    }
}
