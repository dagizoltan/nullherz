#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool, AtomicU64};
use std::thread;
use ipc_layer::{RingBuffer, Producer};
use super::buffer_pool::RenderBlock;
use super::node::ProcessorNode;

#[derive(Clone, Copy)]
pub struct Job {
    pub node_ptr: *const ProcessorNode,
    pub num_samples: usize,
    pub sub_block_offset: usize,
    pub buffers_ptr: *mut RenderBlock,
    pub x_buffers_ptr: *mut RenderBlock,
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
        // Same combined slot space as the serial executor — see the note there.
        let pdc_slots = (input_count + sidechain_count).min(crate::MAX_CHANNELS);
        for i in 0..pdc_slots {
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
        for i in 0..pdc_slots {
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
            // Debug-only: `eprintln!` is a blocking write(2) under the stderr
            // lock — never on a release worker thread. The fault is recorded
            // lock-free via `bypass_state_ptr` below; this is a dev diagnostic,
            // gated like `assert_finite_block!`.
            #[cfg(debug_assertions)]
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
///
/// # Before you lower this, or conclude the pool is dead weight
///
/// Re-measured 2026-07-29 (4-core @ 3.28 GHz, no `isolcpus`, 8000 blocks per
/// configuration, bootstrapped 4-deck console). The result depends entirely on
/// BLOCK SIZE, so a benchmark at one size answers only for that size:
///
/// * **256 samples (DJ latency): the gate never fires.** The whole 34-node graph
///   costs ~126 µs spread over many stages; one stage must clear this threshold
///   (~46 µs at 3.28 GHz) alone. `NULLHERZ_WORKERS=0` and the default are the
///   same code path here. Forcing dispatch (`NULLHERZ_PARALLEL_THRESHOLD_CYCLES=0`)
///   makes it *worse*: 140 µs vs 117 µs mean.
/// * **1024 samples (offline bounce): it fires and wins.** 350 µs vs 377 µs mean,
///   and it beats FORCED dispatch (412 µs) because it parallelizes only the
///   stages worth it. `bounce.rs` renders at `MAX_BLOCK_SIZE`, so the offline
///   path benefits today.
///
/// The mechanism: forced-parallel p99.9 is roughly CONSTANT in absolute terms
/// across block sizes (2662 / 1900 / 2410 µs at 256 / 512 / 1024) — it is
/// dispatch and scheduler-wakeup jitter, a fixed cost, not proportional to the
/// work. As a fraction of the deadline that is 50% → 18% → 11%. The pool does
/// not improve with larger blocks; the deadline gets more forgiving.
///
/// So lowering this constant to chase the median at live block sizes trades a
/// small mean win for a tail that eats half the budget. The tail is the only
/// number that matters in RT audio. Attack the jitter itself (core isolation)
/// or raise per-stage cost (Phase 5 tap bus, convolution) instead.
///
/// # At studio scale the gate DOES fire at 256 — and the prediction above holds
///
/// Everything above was measured on the bootstrapped 4-deck console: 34 nodes.
/// `MAX_NODES` is 128. Measured 2026-09-22 with
/// `bench_studio_scale` (AMD Ryzen 5 PRO 4650U, 12 threads, no `isolcpus`, no
/// `rtprio` allowance — so pool workers get RTKit's SCHED_RR-20, not the
/// FIFO 85 `setup_rt_thread` asks for), 256-sample blocks throughout:
///
/// ```text
/// SERIAL (gate never fires), 3000 blocks per point:
///   nodes    mean            max
///      12    65.4 us (1.1%)   120.6 us ( 2.1% of budget)
///      20   119.5 us (2.1%)   202.7 us ( 3.5%)
///      36   230.6 us (4.0%)   381.4 us ( 6.6%)
///      72   453.7 us (7.8%)   757.3 us (13.0%)
///     106   683.3 us (11.8%) 1639.3 us (28.2%)
/// ```
///
/// Node cost is essentially linear (5.4 -> 6.4 us/node across a 9x range), so
/// the engine carries its structural ceiling inside ~14% of a 5.8 ms budget
/// with no parallelism at all. **CPU is not what limits graph size.**
///
/// Which address space limits it depends on the STRIP SHAPE, and the harness's
/// shape is not the product's. `bench_studio_scale` gives every node its own
/// output pair (to reach a high node count), so it spends 2 buffers per node
/// and runs out of `MAX_BUFFERS` first. The real DJ deck strip processes IN
/// PLACE and spends 1.66 — `cargo run -p nullherz-mixer --example graph_budget`
/// reports the 4-deck console at 58 nodes / 96 buffers, where `MAX_NODES` binds
/// first (buffers would allow 145 nodes). A deck strip costs 11 nodes and 19
/// buffers at the margin, so the product ceiling is about TEN channel strips,
/// with both ceilings arriving within one strip of each other. Do not carry
/// this table's buffer arithmetic onto a graph built the other way.
///
/// At 106 nodes the stages are finally expensive enough that this gate fires,
/// and what it buys is the mean — at the tail's expense. Five INTERLEAVED
/// repeats per configuration (a single A/B is not enough; see below):
///
/// ```text
///            mean            p99.9              max
///   serial   658-763 us      827-1437 us        940-1763 us (16-30% of budget)
///   default  237-549 us     1099-5988 us       1274-6200 us (22-107%)
/// ```
///
/// The pool halves the mean and makes the tail unbounded: one repeat in five
/// exceeded the deadline outright. That is the mechanism this comment already
/// predicted — dispatch and wakeup jitter is a fixed cost, not proportional to
/// the work — now observed at the size where the gate actually engages. The
/// mean it wins is one nobody needs: serial is at 11.8%.
///
/// So the gate optimises the wrong statistic at this scale. It compares a
/// stage's cost to a fixed cycle count, which is a question about throughput;
/// the question that matters is whether SERIAL would miss the deadline, and at
/// 106 nodes it does not come close. Whether to make it deadline-aware, raise
/// the constant, or require core isolation is a product decision and is NOT
/// made here — but do not lower it on mean-only evidence.
///
/// **Measurement note.** The first A/B run of this comparison read as a 6-11x
/// tail regression from the pool. Five interleaved repeats did not support
/// that: the honest claim is that serial's tail is BOUNDED and stable while the
/// pool's is variable and occasionally over budget. Tail statistics on an
/// unisolated machine need repeats, not a single run — the same discipline this
/// file's earlier numbers were gathered under.
///
/// Full write-up: `docs/system/ARCHITECTURE.md`, "Cost-gated parallelism".
pub const DEFAULT_PARALLEL_THRESHOLD_CYCLES: u64 = 1_000_000;
// Raised from 150_000. The analysis above was right and its PREMISE expired: it
// concluded "256 samples: the gate never fires" from a 34-node console, and the
// graph has since grown past 55 nodes (per-band tap points), so stages now clear
// 150k cycles at live block sizes and dispatch happens where that analysis says
// it must not.
//
// Measured on real ALSA at 48 kHz, 256-sample blocks, 4 minutes each, paired
// back to back on an idle machine (`bin/survival.rs`, which attributes each
// outlier and reports how much of it was NOT in any node):
//
//     threshold 150_000 (dispatch fires)  peak 3745 us  mean 306 us  20 outliers
//     dispatch disabled entirely          peak  528 us  mean 243 us   0 outliers
//
// Seven times the tail, and the MEAN was worse too — dispatch at this node count
// is not a trade, it is pure loss. Every outlier had the same shape: all nodes
// summed to 178-302 us of a 3745 us block, so 93-95% of it was the main thread
// sitting in `wait_for_completion`'s 2000-iteration `spin_loop` waiting for a
// worker that had not been scheduled. That time belongs to no node's timer,
// which is why it went unexplained for so long, and it shows as pure execution
// with zero context switches because spinning is exactly that.
//
// It needs audio: with the decks stopped (`survival --silence`) stages fall below
// the gate, nothing dispatches, and the peak drops to 707 us with no outliers.
// That is also the proof this is the gate and not the audio — the same graph on
// zeroes never dispatches and never spikes.
//
// 1_000_000 cycles is ~400 us at this machine's clock: above the largest stage
// at 256 samples (the deck samplers share one, ~220-300 us) and below the same
// stage at 1024 (~4x that), so the offline bounce path documented above keeps its
// win while live block sizes stay serial. THIS IS A CEILING ON STAGE COST, not a
// tuning dial: lowering it re-admits a tail worth 70% of a 256-sample budget.

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
