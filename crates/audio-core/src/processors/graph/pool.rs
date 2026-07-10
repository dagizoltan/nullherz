use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool, AtomicU64};
use std::thread;
use ipc_layer::{AudioBlock, RingBuffer, Producer};
use super::node::ProcessorNode;

#[derive(Clone, Copy)]
pub struct Job {
    pub node_ptr: *const ProcessorNode,
    pub num_samples: usize,
    pub sub_block_offset: usize,
    pub buffers_ptr: *mut AudioBlock,
    pub x_buffers_ptr: *mut AudioBlock,
    pub input_indices: [usize; crate::MAX_CHANNELS],
    pub sidechain_indices: [usize; crate::MAX_CHANNELS],
    pub input_delays: [f32; crate::MAX_CHANNELS],
    pub output_indices: [usize; crate::MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
    pub sidechain_count: usize,
    pub node_idx: usize, // for telemetry
    pub telemetry_ptr: *mut [AtomicU64; crate::MAX_NODES],
    pub transport: Option<crate::Transport>,
    pub host_ptr: Option<*const dyn nullherz_traits::Host>,
    pub is_last_sub_block: bool,
    pub is_bypassed: bool,
    pub pdc_lines_ptr: *mut crate::processors::graph::buffer_pool::PdcLines,
    pub pdc_write_pos: usize,
}

unsafe impl Send for Job {}

impl nullherz_traits::ParallelExecutor for TaskPool {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn num_workers(&self) -> usize {
        self.worker_producers.len()
    }

    unsafe fn push_job_raw(&mut self, worker_idx: usize, data: *const u8, size: usize, _exec_fn: fn(*const u8)) -> bool {
        // Validation: In this hardened implementation, we only support Job type for now
        // but we respect the exec_fn if we were to support multiple job types.
        if size != std::mem::size_of::<Job>() { return false; }
        let job_ptr = data as *const Job;
        // Since Job is Copy (contains only pointers, primitives and Options of Copy types),
        // bitwise copy is safe and won't cause double-frees.
        self.worker_producers[worker_idx].push(unsafe { *job_ptr }).is_ok()
    }

    fn wait_for_completion(&mut self, target_count: usize) {
        while self.completion.load(Ordering::Acquire) < target_count {
            self.completion_fd.wait();
        }
    }
    fn current_completion_count(&self) -> usize {
        self.completion.load(Ordering::Acquire)
    }
    fn notify_workers(&mut self) {
        for fd in &self.worker_wake_fds {
            fd.notify();
        }
    }
}

#[derive(Clone, Copy)]
pub struct StaticAssignment {
    pub node_idx: u32,
    pub worker_idx: u8,
}

pub struct TaskPool {
    workers: Vec<thread::JoinHandle<()>>,
    pub(crate) worker_producers: Vec<Producer<Job>>,
    pub(crate) completion: Arc<AtomicUsize>,
    pub(crate) running: Arc<AtomicBool>,
    pub(crate) worker_wake_fds: Vec<ipc_layer::EventFd>,
    pub(crate) completion_fd: ipc_layer::EventFd,
    /// Caches worker assignments for stable topologies.
    pub assignment_cache: [Option<StaticAssignment>; crate::MAX_NODES],
    /// Per-worker telemetry storage to eliminate atomic contention.
    pub worker_telemetry: Arc<Box<[[AtomicU64; crate::MAX_NODES]]>>,
}

impl TaskPool {
    pub fn new(num_workers: usize) -> Self {
        let mut workers = Vec::new();
        let mut worker_producers = Vec::new();
        let mut worker_wake_fds = Vec::new();
        let completion = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicBool::new(true));
        let completion_fd = ipc_layer::EventFd::create().expect("Failed to create completion EventFd");

        let mut tel_data = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            tel_data.push(std::array::from_fn(|_| AtomicU64::new(0)));
        }
        let worker_telemetry = Arc::new(tel_data.into_boxed_slice());

        for i in 0..num_workers {
            let (prod, mut cons) = RingBuffer::<Job>::new(128).split();
            let running_worker = running.clone();
            let completion_worker = completion.clone();
            let wake_fd = ipc_layer::EventFd::create().expect("Failed to create worker wake EventFd");
            let worker_wake_fd = ipc_layer::EventFd::from_raw(wake_fd.fd());
            let completion_fd_worker = ipc_layer::EventFd::from_raw(completion_fd.fd());

            let handle = thread::spawn(move || {
                ipc_layer::setup_rt_thread(85, Some(i + 1)); // Pin workers to cores 1..N
                while running_worker.load(Ordering::Relaxed) {
                    // RT-9: Hybrid spin-wait for reduced context-switch overhead
                    let mut job_opt = None;
                    for _ in 0..1000 {
                         if let Some(j) = cons.pop() {
                             job_opt = Some(j);
                             break;
                         }
                         std::hint::spin_loop();
                    }

                    if let Some(job) = job_opt {
                        // SAFETY: job.node_ptr is guaranteed to be valid for the duration of the job execution.
                        let node = unsafe { &*job.node_ptr };
                        let num_samples = job.num_samples;
                        let buffers_ptr = job.buffers_ptr;

                        let mut node_inputs_storage = [ &[][..]; crate::MAX_CHANNELS * 2 ];
                        let input_count = job.input_count.min(crate::MAX_CHANNELS);
                        let sidechain_count = job.sidechain_count.min(crate::MAX_CHANNELS);
                        let offset = job.sub_block_offset;

                        for i in 0..input_count + sidechain_count {
                            let p_idx = if i < input_count {
                                *job.input_indices.get(i).unwrap_or(&0)
                            } else {
                                *job.sidechain_indices.get(i - input_count).unwrap_or(&0)
                            };

                            if p_idx >= crate::MAX_NODES {
                                let x_idx = p_idx - crate::MAX_NODES;
                                if x_idx < crate::MAX_CROSSFADE_BUFFERS {
                                    // SAFETY: x_buffers_ptr is valid for MAX_CROSSFADE_BUFFERS AudioBlocks as pre-allocated by ProcessorGraph.
                                    unsafe { node_inputs_storage[i] = &(&(*job.x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                                }
                            } else if p_idx < crate::MAX_NODES {
                                // SAFETY: buffers_ptr is valid for MAX_NODES AudioBlocks as pre-allocated by ProcessorGraph.
                                unsafe { node_inputs_storage[i] = &(&(*job.buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
                            }
                        }

                        let mut node_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
                        let output_count = job.output_count.min(crate::MAX_CHANNELS);
                        for (i, output_storage) in node_outputs_reconstructed.iter_mut().enumerate().take(output_count) {
                            let p_idx = *job.output_indices.get(i).unwrap_or(&0);
                            if p_idx < crate::MAX_NODES {
                                // SAFETY: buffers_ptr is valid and unique for each index in the current stage.
                                unsafe {
                                    *output_storage = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
                                }
                            }
                        }

                        let mut pdc_storage = [[0.0f32; ipc_layer::MAX_BLOCK_SIZE]; crate::MAX_CHANNELS];
                        if !job.pdc_lines_ptr.is_null() {
                            let pdc_lines = unsafe { &mut *job.pdc_lines_ptr };
                            for i in 0..input_count {
                                let delay_f = job.input_delays[i];
                                if delay_f > 0.0 && delay_f < (crate::processors::graph::buffer_pool::MAX_PDC_SAMPLES as f32 - 4.0) {
                                    let input = node_inputs_storage[i];
                                    let max_len = crate::processors::graph::buffer_pool::MAX_PDC_SAMPLES;
                                    let mut w_pos = (job.pdc_write_pos.wrapping_sub(num_samples)) % max_len;
                                    for &sample in input {
                                        pdc_lines.set_sample(job.node_idx, i, w_pos, sample);
                                        w_pos = (w_pos + 1) % max_len;
                                    }

                                    let delay_int = delay_f.floor() as usize;
                                    let delay_frac = delay_f - delay_f.floor();

                                    let mut r_pos = (job.pdc_write_pos.wrapping_sub(num_samples).wrapping_sub(delay_int)) % max_len;
                                    for j in 0..num_samples {
                                        pdc_storage[i][j] = pdc_lines.get_sample_interpolated(job.node_idx, i, r_pos, delay_frac);
                                        r_pos = (r_pos + 1) % max_len;
                                    }
                                }
                            }
                            for i in 0..input_count {
                                let delay = job.input_delays[i] as usize;
                                if delay > 0 && delay < crate::processors::graph::buffer_pool::MAX_PDC_SAMPLES {
                                    node_inputs_storage[i] = &pdc_storage[i][..num_samples];
                                }
                            }
                        }

                        let start = crate::get_cycles();

                        let mut inner_context = nullherz_traits::ProcessContext {

                            transport: job.transport.as_ref(),
                            host: job.host_ptr.map(|ptr| unsafe { &*ptr }),
                            sub_block_offset: offset,
                            is_last_sub_block: job.is_last_sub_block,
                        };
                        // SAFETY: node.processor is an UnsafeCell. Access is synchronized via topological stage fencing.
                        if job.is_bypassed {
                            if input_count > 0 {
                                let input = node_inputs_storage[0];
                                for output in node_outputs_reconstructed.iter_mut().take(output_count) {
                                    output.copy_from_slice(input);
                                }
                            } else {
                                for output in node_outputs_reconstructed.iter_mut().take(output_count) {
                                    output.fill(0.0);
                                }
                            }
                        } else {
                            unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count + sidechain_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }
                            for output in node_outputs_reconstructed.iter().take(output_count) {
                                crate::assert_finite_block!(output, job.node_idx);
                            }
                        }

                        let elapsed = crate::get_cycles().wrapping_sub(start);
                        // SAFETY: telemetry_ptr is guaranteed valid for the engine lifetime.
                        // Optimization: report to local worker accumulator first if we had one,
                        // for now we use store with Relaxed ordering which is sufficient for telemetry.
                        unsafe { (*job.telemetry_ptr)[job.node_idx].store(elapsed, Ordering::Relaxed); }

                        completion_worker.fetch_add(1, Ordering::Release);
                        completion_fd_worker.notify();
                    } else {
                        let _ = worker_wake_fd.wait();
                    }
                }
            });

            workers.push(handle);
            worker_producers.push(prod);
            worker_wake_fds.push(wake_fd);
        }

        Self {
            workers,
            worker_producers,
            completion,
            running,
            worker_wake_fds,
            completion_fd,
            assignment_cache: [None; crate::MAX_NODES],
            worker_telemetry,
        }
    }

    pub fn clear_cache(&mut self) {
        self.assignment_cache = [None; crate::MAX_NODES];
    }
}

impl Drop for TaskPool {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        for fd in &self.worker_wake_fds {
            fd.notify();
        }
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}
