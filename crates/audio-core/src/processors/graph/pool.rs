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
    pub output_indices: [usize; crate::MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
    pub node_idx: usize, // for telemetry
    pub telemetry_ptr: *const [AtomicU64; crate::MAX_NODES],
    pub transport: Option<crate::Transport>,
    pub is_last_sub_block: bool,
}

unsafe impl Send for Job {}

pub struct TaskPool {
    workers: Vec<thread::JoinHandle<()>>,
    pub(crate) worker_producers: Vec<Producer<Job>>,
    pub(crate) completion: Arc<AtomicUsize>,
    pub(crate) running: Arc<AtomicBool>,
    pub(crate) worker_wake_fds: Vec<ipc_layer::EventFd>,
    pub(crate) completion_fd: ipc_layer::EventFd,
}

impl TaskPool {
    pub fn new(num_workers: usize) -> Self {
        let mut workers = Vec::new();
        let mut worker_producers = Vec::new();
        let mut worker_wake_fds = Vec::new();
        let completion = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicBool::new(true));
        let completion_fd = ipc_layer::EventFd::create().expect("Failed to create completion EventFd");

        for i in 0..num_workers {
            let (prod, mut cons) = RingBuffer::<Job>::new(128).split();
            let running_worker = running.clone();
            let completion_worker = completion.clone();
            let wake_fd = ipc_layer::EventFd::create().expect("Failed to create worker wake EventFd");
            let worker_wake_fd = ipc_layer::EventFd::from_raw(wake_fd.fd());
            let completion_fd_worker = ipc_layer::EventFd::from_raw(completion_fd.fd());

            let handle = thread::spawn(move || {
                crate::setup_rt_thread(85, Some(i + 1)); // Pin workers to cores 1..N
                while running_worker.load(Ordering::Relaxed) {
                    if let Some(job) = cons.pop() {
                        // SAFETY: job.node_ptr is guaranteed to be valid for the duration of the job execution.
                        let node = unsafe { &*job.node_ptr };
                        let num_samples = job.num_samples;
                        let buffers_ptr = job.buffers_ptr;

                        let mut node_inputs_storage = [ &[][..]; 16 ];
                        let input_count = job.input_count.min(16);
                        let offset = job.sub_block_offset;

                        for (i, input_storage) in node_inputs_storage.iter_mut().enumerate().take(input_count) {
                            let p_idx = *job.input_indices.get(i).unwrap_or(&0);
                            if p_idx >= 64 {
                                let x_idx = p_idx - 64;
                                if x_idx < 8 {
                                    // SAFETY: x_buffers_ptr is valid for 8 AudioBlocks as pre-allocated by ProcessorGraph.
                                    unsafe { *input_storage = &(&(*job.x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                                }
                            } else if p_idx < 64 {
                                // SAFETY: buffers_ptr is valid for MAX_NODES AudioBlocks as pre-allocated by ProcessorGraph.
                                unsafe { *input_storage = &(&(*buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
                            }
                        }

                        let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [][..]);
                        let output_count = job.output_count.min(16);
                        for (i, output_storage) in node_outputs_reconstructed.iter_mut().enumerate().take(output_count) {
                            let p_idx = *job.output_indices.get(i).unwrap_or(&0);
                            if p_idx < 64 {
                                // SAFETY: buffers_ptr is valid and unique for each index in the current stage.
                                unsafe {
                                    *output_storage = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
                                }
                            }
                        }

                        let start = crate::get_cycles();

                        let mut inner_context = crate::processors::ProcessContext {
                            pool: None,
                            transport: job.transport.as_ref(),
                            sub_block_offset: offset,
                            is_last_sub_block: job.is_last_sub_block
                        };
                        // SAFETY: node.processor is an UnsafeCell. Access is synchronized via topological stage fencing.
                        unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }

                        let elapsed = crate::get_cycles().wrapping_sub(start);
                        // SAFETY: telemetry_ptr is guaranteed valid for the engine lifetime.
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

        Self { workers, worker_producers, completion, running, worker_wake_fds, completion_fd }
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
