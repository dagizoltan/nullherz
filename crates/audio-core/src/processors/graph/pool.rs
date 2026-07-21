#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
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
    pub bypass_state_ptr: *const std::sync::atomic::AtomicBool,
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
        // Typical stages complete in a few microseconds; parking on the
        // eventfd immediately costs a syscall plus a context-switch round
        // trip per stage. Spin briefly first, then block.
        for _ in 0..2000 {
            if self.completion.load(Ordering::Acquire) >= target_count { return; }
            std::hint::spin_loop();
        }
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

/// Execute one graph job on a worker thread. `pdc_scratch` is the worker's
/// persistent interpolation scratch; rows are fully overwritten for the
/// `[..num_samples]` range before they are read.
fn run_job(job: &Job, pdc_scratch: &mut [[f32; ipc_layer::MAX_BLOCK_SIZE]; crate::MAX_CHANNELS]) {
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

        // BufferSlot is the single interpreter of the
        // crossfade-sentinel encoding — this used to split
        // at MAX_NODES and misread every buffer id >= 64.
        match nullherz_traits::BufferSlot::from_raw(p_idx) {
            nullherz_traits::BufferSlot::Crossfade(x_idx) => {
                if x_idx < crate::MAX_CROSSFADE_BUFFERS {
                    // SAFETY: x_buffers_ptr is valid for MAX_CROSSFADE_BUFFERS AudioBlocks as pre-allocated by ProcessorGraph.
                    unsafe { node_inputs_storage[i] = &(&(*job.x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                }
            }
            nullherz_traits::BufferSlot::Pool(p_idx) => {
                // SAFETY: buffers_ptr is valid for MAX_BUFFERS AudioBlocks as pre-allocated by ProcessorGraph.
                unsafe { node_inputs_storage[i] = &(&(*job.buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
            }
        }
    }

    let mut node_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
    let output_count = job.output_count.min(crate::MAX_CHANNELS);
    for (i, output_storage) in node_outputs_reconstructed.iter_mut().enumerate().take(output_count) {
        let p_idx = *job.output_indices.get(i).unwrap_or(&0);
        if p_idx < crate::MAX_BUFFERS {
            // SAFETY: buffers_ptr is valid and unique for each index in the current stage.
            unsafe {
                *output_storage = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
            }
        }
    }

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
                    pdc_scratch[i][j] = pdc_lines.get_sample_interpolated(job.node_idx, i, r_pos, delay_frac);
                    r_pos = (r_pos + 1) % max_len;
                }
            }
        }
        for i in 0..input_count {
            // Same condition as the write pass and the serial executor: a
            // purely FRACTIONAL delay (0 < d < 1) must swap the input too —
            // `as usize` truncation used to drop it on this path only.
            let delay_f = job.input_delays[i];
            if delay_f > 0.0 && delay_f < (crate::processors::graph::buffer_pool::MAX_PDC_SAMPLES as f32 - 4.0) {
                node_inputs_storage[i] = &pdc_scratch[i][..num_samples];
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
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            unsafe {
                (*node.processor.get()).process(
                    &node_inputs_storage[..input_count + sidechain_count],
                    &mut node_outputs_reconstructed[..output_count],
                    &mut inner_context
                );
            }
        }));

        if result.is_err() {
            eprintln!(
                "Audio Engine: caught panic in process() of node_idx {} (processor type: '{}')",
                job.node_idx,
                unsafe { (*node.processor.get()).processor_type() }
            );

            // Zero-fill reconstructed outputs
            for output in node_outputs_reconstructed.iter_mut().take(output_count) {
                output.fill(0.0);
            }

            // Permanently bypass the node
            if !job.bypass_state_ptr.is_null() {
                unsafe { (*job.bypass_state_ptr).store(true, Ordering::Relaxed); }
            }
        } else {
            for output in node_outputs_reconstructed.iter().take(output_count) {
                crate::assert_finite_block!(output, job.node_idx);
            }
        }
    }

    let elapsed = crate::get_cycles().wrapping_sub(start);
    // SAFETY: telemetry_ptr is guaranteed valid for the engine lifetime.
    unsafe { (*job.telemetry_ptr)[job.node_idx].store(elapsed, Ordering::Relaxed); }
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
    /// Per-stage cost gate (cycles): the executor dispatches a stage to the
    /// pool only if the stage's telemetry-measured node-time sum meets or
    /// exceeds this. Below it, the stage runs inline on the RT thread — pool
    /// dispatch (push + eventfd wake + completion wait) costs more than the
    /// stage's own work, so parallelizing it is a net loss (see the
    /// 2026-07-21 worker experiment: a 2× loss on ≤ 2 physical cores).
    /// Default derived from that machine's measured ~150k-cycle dispatch
    /// overhead; override with NULLHERZ_PARALLEL_THRESHOLD_CYCLES.
    pub parallel_threshold_cycles: u64,
}

/// Default per-stage parallel cost gate, in TSC cycles. On the 2026-07-21
/// reference box (~2.6 GHz) pool dispatch added ~150k cycles (~55 µs) per
/// stage; a stage must out-cost that to be worth parallelizing. Conservative
/// by design — it never regresses a cheap stage, and on faster/many-core
/// hardware with cheaper dispatch it can be lowered via the env override.
pub const DEFAULT_PARALLEL_THRESHOLD_CYCLES: u64 = 150_000;

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
                // Per-thread PDC interpolation scratch, allocated once at
                // spawn (off the RT path). Rows are fully overwritten for
                // the [..num_samples] range before they are read, so reuse
                // across jobs is safe. A fresh 16 KB zero-init per job used
                // to run here whether or not any input delay was active.
                let mut pdc_scratch = Box::new([[0.0f32; ipc_layer::MAX_BLOCK_SIZE]; crate::MAX_CHANNELS]);
                while running_worker.load(Ordering::Relaxed) {
                    // Drain everything queued, then signal completion ONCE
                    // for the batch — an eventfd write per job was a syscall
                    // per node per stage. The waiter re-checks the counter on
                    // every wake, so batching cannot under-notify: each
                    // worker's final fetch_add is always followed by a notify.
                    let mut batch_completed = 0usize;
                    while let Some(job) = cons.pop() {
                        run_job(&job, &mut pdc_scratch);
                        completion_worker.fetch_add(1, Ordering::Release);
                        batch_completed += 1;
                    }
                    if batch_completed > 0 {
                        completion_fd_worker.notify();
                        continue; // the queue may have refilled while notifying
                    }

                    // RT-9: Hybrid spin-wait for reduced context-switch overhead
                    let mut saw_job = false;
                    for _ in 0..1000 {
                        if cons.peek().is_some() { saw_job = true; break; }
                        std::hint::spin_loop();
                    }
                    if !saw_job {
                        let _ = worker_wake_fd.wait();
                    }
                }
            });

            workers.push(handle);
            worker_producers.push(prod);
            worker_wake_fds.push(wake_fd);
        }

        // Env read happens here (pool construction, setup thread), never on
        // the RT path — the executor just reads the resolved field.
        let parallel_threshold_cycles = std::env::var("NULLHERZ_PARALLEL_THRESHOLD_CYCLES")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_PARALLEL_THRESHOLD_CYCLES);

        Self {
            workers,
            worker_producers,
            completion,
            running,
            worker_wake_fds,
            completion_fd,
            assignment_cache: [None; crate::MAX_NODES],
            worker_telemetry,
            parallel_threshold_cycles,
        }
    }

    pub fn clear_cache(&mut self) {
        self.assignment_cache = [None; crate::MAX_NODES];
    }

    /// Wake only the workers named in `mask` (bit w = worker w). The stage
    /// scheduler builds the mask from actual job placement — waking all N
    /// workers per stage cost N eventfd syscalls from the RT thread, and
    /// jobless workers burned a full spin window before sleeping again.
    pub fn notify_workers_masked(&self, mask: u64) {
        for (w, fd) in self.worker_wake_fds.iter().enumerate() {
            if w < 64 && (mask & (1u64 << w)) != 0 {
                fd.notify();
            }
        }
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
