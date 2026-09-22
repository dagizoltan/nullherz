//! Block-time scaling beyond the 4-deck console.
//!
//! # Why this exists
//!
//! Every performance fixture in the tree measures ONE graph shape: the
//! bootstrapped 4-deck DJ console (`bench_console_block`, `profile_console_nodes`,
//! and every `probe_*`). `MAX_NODES` is 128 and the product is a studio suite,
//! so the numbers those fixtures produce — "11.7% of budget", "the worker pool
//! normally declines to dispatch" — are statements about a graph roughly a
//! tenth of the size the engine is built to carry. Neither transfers by
//! argument.
//!
//! Three things this answers that the console fixture cannot:
//!
//! 1. **Where does peak block time cross the period?** That is the number
//!    deciding whether a 2-period (10.7 ms) device buffer is reachable, and it
//!    is a function of graph size, not of the mean.
//! 2. **Does the worker pool engage, and does it help?** The cost gate
//!    (`DEFAULT_PARALLEL_THRESHOLD_CYCLES`) is documented against this same
//!    4-deck console, where it concludes the pool should usually decline. Wider
//!    and more expensive stages are exactly what changes that verdict, and the
//!    parallel path is the one `unsafe impl Sync for ProcessorNode` rests on.
//! 3. **Is the render deterministic?** An offline bounce must equal the live
//!    output for a studio suite; a DJ console never needed that. The dispatch
//!    decision depends on MEASURED cycles, so it depends on machine load — which
//!    makes "same input, same output" a claim to test rather than assume.
//!
//! # The graph
//!
//! `n` stereo track strips, each `SAMPLER -> MASTERING_EQ`, summed in groups of
//! up to `MAX_CHANNELS` per side, then a master `MASTERING_EQ -> LIMITER`. Two
//! nodes per strip is deliberately the CHEAP shape: it maximises the node count
//! reachable inside `MAX_BUFFERS` (240), so the sweep explores the engine's
//! structural ceiling rather than a few fat nodes. The sampler is the real one,
//! playing real material through the real resampler.
//!
//! **This shape is not the product's**, and the difference matters for anything
//! but the per-node timings. Every node here gets its own output pair, so a
//! strip costs 2 nodes and 4 buffers. The real DJ deck strip processes IN PLACE:
//! 11 nodes and 19 buffers at the margin, 1.66 buffers per node against this
//! harness's 2.0. So this harness exhausts `MAX_BUFFERS` first while the real
//! console exhausts `MAX_NODES` first. Read the sweep as a statement about NODE
//! COST, and see `nullherz-mixer --example graph_budget` for what the ceilings
//! mean for a console anyone would build.
//!
//! Run:
//!   cargo run --release -p nullherz-conductor --example bench_studio_scale
//!
//! Environment:
//!   NULLHERZ_STUDIO_TRACKS=4,8,16,32,48   tracks per point in the sweep
//!   NULLHERZ_BENCH_BLOCK_SIZE=256         frames per block
//!   NULLHERZ_BENCH_BLOCKS=4000            measured blocks per point
//!   NULLHERZ_PARALLEL_THRESHOLD_CYCLES=0  force pool dispatch; a huge value
//!                                         disables it (see the measurement note
//!                                         on DEFAULT_PARALLEL_THRESHOLD_CYCLES)
//!   NULLHERZ_STUDIO_DETERMINISM=1         two renders in-process, compared
//!                                         sample by sample
//!   NULLHERZ_STUDIO_HASH=1                one render, hashed — compare the hash
//!                                         ACROSS runs with different pool
//!                                         settings, which is the stronger claim
//!
//! Tail statistics on a machine without core isolation need REPEATS, not a
//! single run: the first A/B of serial-vs-pool here read as a 6-11x tail
//! regression that five interleaved repeats did not support. Alternate the
//! configurations and read the spread.
//!
//! Compare runs on the same machine only.

use std::sync::Arc;
use std::time::Instant;

use nullherz_conductor::Conductor;
use nullherz_dna::GeneticLibrary;
use nullherz_traits::{
    Command, CoreCommand, PerformanceCommand, ProcessorTypeId, TopologyCommand, MAX_CHANNELS,
};

const SR: f32 = 44_100.0;
/// Long enough to outlast install + warmup + measurement at every sweep point.
const TONE_SECONDS: f32 = 45.0;
/// Blocks pumped after the last topology chunk, so the committed plan is live
/// before anything is measured.
const INSTALL_SETTLE_BLOCKS: usize = 256;
const ARM_BLOCKS: usize = 16;
const WARMUP_BLOCKS: usize = 1_000;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn block_size() -> usize {
    env_usize("NULLHERZ_BENCH_BLOCK_SIZE", 256).min(nullherz_traits::MAX_BLOCK_SIZE).max(1)
}

fn sweep() -> Vec<usize> {
    match std::env::var("NULLHERZ_STUDIO_TRACKS") {
        Ok(v) => v.split(',').filter_map(|s| s.trim().parse().ok()).filter(|&n: &usize| n > 0).collect(),
        Err(_) => vec![4, 8, 16, 32, 48],
    }
}

/// Distinct multitone material per track, so no two strips are bit-identical
/// and the summing tree has something real to do.
fn register_tone(conductor: &Conductor, id: u64, base_hz: f32) {
    let frames = (SR * TONE_SECONDS) as usize;
    let mut samples = Vec::with_capacity(frames * 2);
    for chan in 0..2 {
        let detune = if chan == 0 { 1.0 } else { 1.005 };
        for i in 0..frames {
            let t = i as f32 / SR;
            let mut s = 0.0f32;
            for (k, amp) in [(1.0f32, 0.275f32), (3.0, 0.15), (7.0, 0.075)] {
                s += (t * base_hz * detune * k * 2.0 * std::f32::consts::PI).sin() * amp;
            }
            samples.push(s);
        }
    }

    let mut metadata = nullherz_traits::SampleMetadata::new_empty();
    metadata.bpm = 120.0;
    metadata.total_samples = frames as u64;
    metadata.channels = 2;
    let metadata = Arc::new(metadata);
    conductor
        .transfusion_manager
        .sample_registry
        .register_with_metadata(id, Arc::new(samples), metadata.clone());

    let lib = conductor.library.lock();
    lib.save_track(&nullherz_dna::LibraryTrack {
        id,
        path: format!("tone://{}", id),
        title: format!("studio tone {}", id),
        artist: "bench".to_string(),
        album: "bench".to_string(),
        genre: "test tone".to_string(),
        energy_level: 0.5,
        metadata,
    })
    .expect("in-memory library save cannot fail");
}

struct Built {
    sampler_ids: Vec<u32>,
    nodes_used: u32,
    buffers_used: u32,
    commands: Vec<Command>,
}

/// Build `n` track strips plus the summing tree and master chain, as topology
/// commands on the conductor's own allocator.
fn build_studio(conductor: &mut Conductor, n: usize) -> Built {
    let alloc = conductor.mixer_manager.id_allocator.clone();
    let master_l = conductor.mixer_manager.config.master_l as u32;
    let master_r = conductor.mixer_manager.config.master_r as u32;

    let mut cmds: Vec<Command> = Vec::new();
    let mut sampler_ids = Vec::with_capacity(n);
    // Per-strip post-EQ outputs, which the summing tree consumes.
    let mut strip_out: Vec<(u32, u32)> = Vec::with_capacity(n);

    let add = |cmds: &mut Vec<Command>, id: u32, ty: ProcessorTypeId| {
        cmds.push(Command::Topology(TopologyCommand::AddNode { node_idx: id, processor_type_id: ty }));
    };
    let wire_in = |cmds: &mut Vec<Command>, id: u32, slot: u32, buf: u32| {
        cmds.push(Command::Topology(TopologyCommand::UpdateEdge { node_idx: id, input_idx: slot, new_buffer_idx: buf }));
    };
    let wire_out = |cmds: &mut Vec<Command>, id: u32, slot: u32, buf: u32| {
        cmds.push(Command::Topology(TopologyCommand::UpdateOutputEdge { node_idx: id, output_idx: slot, new_buffer_idx: buf }));
    };

    for _ in 0..n {
        // SAMPLER -> stereo pair
        let samp = alloc.allocate_node_id();
        let s_l = alloc.allocate_buffer_id(2);
        let s_r = s_l + 1;
        add(&mut cmds, samp, ProcessorTypeId::SAMPLER);
        wire_out(&mut cmds, samp, 0, s_l);
        wire_out(&mut cmds, samp, 1, s_r);

        // MASTERING_EQ -> stereo pair (identity at unity gain, so it costs real
        // biquad time without moving the signal)
        let eq = alloc.allocate_node_id();
        let e_l = alloc.allocate_buffer_id(2);
        let e_r = e_l + 1;
        add(&mut cmds, eq, ProcessorTypeId::MASTERING_EQ);
        wire_in(&mut cmds, eq, 0, s_l);
        wire_in(&mut cmds, eq, 1, s_r);
        wire_out(&mut cmds, eq, 0, e_l);
        wire_out(&mut cmds, eq, 1, e_r);

        sampler_ids.push(samp);
        strip_out.push((e_l, e_r));
    }

    // Summing tree: SummingProcessor mixes ALL its inputs into outputs[0], so
    // one node per side per group. Groups of MAX_CHANNELS keep every node
    // inside its routing arrays.
    let mut level: Vec<(u32, u32)> = strip_out;
    while level.len() > 1 {
        let mut next: Vec<(u32, u32)> = Vec::new();
        for group in level.chunks(MAX_CHANNELS) {
            if group.len() == 1 {
                next.push(group[0]);
                continue;
            }
            let sum_l = alloc.allocate_node_id();
            let sum_r = alloc.allocate_node_id();
            let out_l = alloc.allocate_buffer_id(2);
            let out_r = out_l + 1;
            add(&mut cmds, sum_l, ProcessorTypeId::SUMMING);
            add(&mut cmds, sum_r, ProcessorTypeId::SUMMING);
            for (i, (l, r)) in group.iter().enumerate() {
                wire_in(&mut cmds, sum_l, i as u32, *l);
                wire_in(&mut cmds, sum_r, i as u32, *r);
            }
            wire_out(&mut cmds, sum_l, 0, out_l);
            wire_out(&mut cmds, sum_r, 0, out_r);
            next.push((out_l, out_r));
        }
        level = next;
    }
    let (mix_l, mix_r) = level[0];

    // Master chain, same shape the console uses: MASTERING_EQ -> LIMITER.
    let m_eq = alloc.allocate_node_id();
    let me_l = alloc.allocate_buffer_id(2);
    let me_r = me_l + 1;
    add(&mut cmds, m_eq, ProcessorTypeId::MASTERING_EQ);
    wire_in(&mut cmds, m_eq, 0, mix_l);
    wire_in(&mut cmds, m_eq, 1, mix_r);
    wire_out(&mut cmds, m_eq, 0, me_l);
    wire_out(&mut cmds, m_eq, 1, me_r);

    let lim = alloc.allocate_node_id();
    add(&mut cmds, lim, ProcessorTypeId::LIMITER);
    wire_in(&mut cmds, lim, 0, me_l);
    wire_in(&mut cmds, lim, 1, me_r);
    wire_out(&mut cmds, lim, 0, master_l);
    wire_out(&mut cmds, lim, 1, master_r);

    let nodes_used = alloc.current_node_id();
    let buffers_used = alloc.current_buffer_id();
    assert!(
        nodes_used as usize <= nullherz_traits::MAX_NODES,
        "{} tracks needs {} nodes, over MAX_NODES ({})",
        n, nodes_used, nullherz_traits::MAX_NODES
    );
    assert!(
        buffers_used as usize <= nullherz_traits::MAX_BUFFERS,
        "{} tracks needs {} buffers, over MAX_BUFFERS ({})",
        n, buffers_used, nullherz_traits::MAX_BUFFERS
    );

    cmds.push(Command::Core(CoreCommand::CommitTopology));

    Built { sampler_ids, nodes_used, buffers_used, commands: cmds }
}

/// Apply a topology in chunks, running the engine between them.
///
/// The mutation ring is 256 deep and the engine drains a BOUNDED 16 per block,
/// so a 32-track graph (~700 mutations) does not fit in one batch. In
/// production the backend thread is draining continuously while the conductor
/// pushes, and `push_mutation` backpressures for up to 1 s against it. This
/// harness has no backend thread — nothing drains unless we pump — so pushing
/// the whole batch first is what made 32 tracks report DROPPED mutations and
/// render silence. That was the harness, not the engine.
///
/// Returns the number of blocks the install took, which is the honest
/// "how long until a loaded session is audible" figure.
fn install_chunked(conductor: &mut Conductor, cmds: Vec<Command>, left: &mut [f32], right: &mut [f32]) -> usize {
    // 96 commands is comfortably under the 256-deep ring even if every one of
    // them lowers to a mutation.
    const CHUNK: usize = 96;
    // 96 commands need ceil(96/16) = 6 blocks to drain; 12 leaves margin for
    // the mutations a single command can expand into.
    const BLOCKS_PER_CHUNK: usize = 12;

    let mut blocks = 0usize;
    for chunk in cmds.chunks(CHUNK) {
        conductor.apply_mixer_commands(chunk.to_vec());
        for _ in 0..BLOCKS_PER_CHUNK {
            pump(conductor, left, right);
            blocks += 1;
        }
    }
    // Let the committed plan settle before anything is measured.
    for _ in 0..INSTALL_SETTLE_BLOCKS {
        pump(conductor, left, right);
        blocks += 1;
    }
    blocks
}

fn pump(conductor: &mut Conductor, left: &mut [f32], right: &mut [f32]) {
    let block = left.len().min(right.len());
    let inputs: Vec<&[f32]> = vec![];
    let mut outputs = vec![left, right];
    let mut engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    let engine_arc = engine_lock.as_mut().expect("setup_engine must install an engine");
    if let Some(engine) = Arc::get_mut(engine_arc) {
        engine.process_block(&inputs, &mut outputs, block);
    } else {
        // No other thread runs in this harness; exclusive access holds by
        // construction (same pattern as bench_console_block and the golden render).
        let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn nullherz_traits::RenderingEngine;
        unsafe { (*engine_ptr).process_block(&inputs, &mut outputs, block); }
    }
}

/// Install the graph, give every sampler material, and start it playing.
fn spin_up(n: usize, block: usize) -> (Conductor, Vec<f32>, Vec<f32>) {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    // A small pool of distinct tones, cycled across the strips.
    const TONES: usize = 8;
    for k in 0..TONES {
        register_tone(&conductor, 9_800 + k as u64, 110.0 * (1.0 + k as f32 * 0.17));
    }

    let built = build_studio(&mut conductor, n);
    let n_cmds = built.commands.len();

    let mut left = vec![0.0f32; block];
    let mut right = vec![0.0f32; block];
    let install_blocks = install_chunked(&mut conductor, built.commands, &mut left, &mut right);

    // Hand each sampler its source directly: LoadTrackToDeck is deck-addressed
    // and there are no decks here.
    for (i, &node) in built.sampler_ids.iter().enumerate() {
        let sample_id = 9_800 + (i % TONES) as u64;
        let sample = conductor
            .transfusion_manager
            .sample_registry
            .get(sample_id)
            .expect("registered above");
        if let Some(ref mut prod) = conductor.topology_manager.topo_producer {
            prod.push(nullherz_traits::TopologyMutation::AddSource {
                node_idx: node,
                buffer: sample.buffer,
                sample_id,
                metadata: Some(sample.metadata.clone()),
            })
            .unwrap_or_else(|_| panic!("topology ring refused a source for node {node}"));
        }
        // Drain as we go: the ring is bounded and 48 sources would overrun it.
        pump(&mut conductor, &mut left, &mut right);
    }

    let mut play: Vec<Command> = vec![Command::Core(CoreCommand::Play)];
    for &node in &built.sampler_ids {
        play.push(Command::Performance(PerformanceCommand::PlayNode { node_idx: node }));
    }
    conductor.apply_mixer_commands(play);
    for _ in 0..ARM_BLOCKS {
        pump(&mut conductor, &mut left, &mut right);
    }

    println!(
        "  graph: {} tracks, {} nodes, {} buffers (ceilings {} / {}) | {} commands installed over {} blocks",
        n, built.nodes_used, built.buffers_used,
        nullherz_traits::MAX_NODES, nullherz_traits::MAX_BUFFERS,
        n_cmds, install_blocks
    );

    (conductor, left, right)
}

fn measure_point(n: usize, block: usize, blocks: usize) {
    let (mut conductor, mut left, mut right) = spin_up(n, block);

    for _ in 0..WARMUP_BLOCKS {
        pump(&mut conductor, &mut left, &mut right);
    }

    let mut peak = 0.0f32;
    let mut ns: Vec<u64> = Vec::with_capacity(blocks);
    for _ in 0..blocks {
        let t0 = Instant::now();
        pump(&mut conductor, &mut left, &mut right);
        ns.push(t0.elapsed().as_nanos() as u64);
        for i in 0..block {
            peak = peak.max(left[i].abs()).max(right[i].abs());
        }
    }

    ns.sort_unstable();
    let mean = ns.iter().sum::<u64>() as f64 / ns.len() as f64;
    let pct = |p: f64| ns[((ns.len() as f64 * p) as usize).min(ns.len() - 1)] as f64;
    let budget = block as f64 / SR as f64 * 1e9;

    assert!(peak > 0.0, "{} tracks rendered silence — the graph never played", n);

    println!(
        "  {:>3} tracks | mean {:>8.1} us ({:>5.1}%) | p99 {:>8.1} | p99.9 {:>8.1} | max {:>9.1} us ({:>6.1}% of budget) | peak {:.4}",
        n,
        mean / 1e3,
        mean / budget * 100.0,
        pct(0.99) / 1e3,
        pct(0.999) / 1e3,
        pct(1.0) / 1e3,
        pct(1.0) / budget * 100.0,
        peak,
    );
}

/// Render the same graph twice and compare every sample.
///
/// The dispatch decision is cost-gated on MEASURED cycles, so it varies with
/// machine load. If that changed summing order the two renders would differ,
/// and an offline bounce could not be trusted to match the live output.
fn determinism(n: usize, block: usize, blocks: usize) {
    let render = || {
        let (mut conductor, mut left, mut right) = spin_up(n, block);
        let mut out: Vec<f32> = Vec::with_capacity(blocks * block * 2);
        for _ in 0..blocks {
            pump(&mut conductor, &mut left, &mut right);
            out.extend_from_slice(&left);
            out.extend_from_slice(&right);
        }
        out
    };

    println!("  render A ...");
    let a = render();
    println!("  render B ...");
    let b = render();

    assert_eq!(a.len(), b.len(), "renders differ in length");
    let mut first_diff = None;
    let mut max_abs = 0.0f32;
    let mut n_diff = 0usize;
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x.to_bits() != y.to_bits() {
            n_diff += 1;
            max_abs = max_abs.max((x - y).abs());
            if first_diff.is_none() {
                first_diff = Some((i, *x, *y));
            }
        }
    }

    let energy = a.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    println!("  samples compared : {}", a.len());
    println!("  peak level       : {:.4}", energy);
    match first_diff {
        None => println!("  RESULT           : bit-identical across both renders"),
        Some((i, x, y)) => {
            let db = if max_abs > 0.0 { 20.0 * max_abs.log10() } else { f32::NEG_INFINITY };
            println!("  RESULT           : {} of {} samples differ", n_diff, a.len());
            println!("  first difference : sample {} — {:e} vs {:e}", i, x, y);
            println!("  largest delta    : {:e} ({:.1} dBFS)", max_abs, db);
        }
    }
}

/// FNV-1a over the raw bit patterns — deterministic, and comparable ACROSS
/// processes, which two renders inside one process cannot be. The stronger
/// determinism claim is not "the same settings give the same output" but "the
/// scheduling decision cannot reach the output at all", and that needs two
/// runs with different pool settings.
fn render_hash(n: usize, block: usize, blocks: usize) -> u64 {
    let (mut conductor, mut left, mut right) = spin_up(n, block);
    let mut h: u64 = 0xcbf29ce484222325;
    let mut peak = 0.0f32;
    for _ in 0..blocks {
        pump(&mut conductor, &mut left, &mut right);
        for v in left.iter().chain(right.iter()) {
            peak = peak.max(v.abs());
            for b in v.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    }
    assert!(peak > 0.0, "hash run rendered silence");
    println!("  peak level : {:.4}", peak);
    h
}

fn main() {
    let block = block_size();
    let blocks = env_usize("NULLHERZ_BENCH_BLOCKS", 4_000);
    let budget = block as f64 / SR as f64 * 1e6;

    println!("block size : {} @ {} Hz (budget {:.0} us)", block, SR, budget);
    println!(
        "pool gate  : {}",
        std::env::var("NULLHERZ_PARALLEL_THRESHOLD_CYCLES")
            .unwrap_or_else(|_| "150000 (DEFAULT_PARALLEL_THRESHOLD_CYCLES)".to_string())
    );
    println!();

    if env_usize("NULLHERZ_STUDIO_HASH", 0) > 0 {
        let n = sweep().last().copied().unwrap_or(32);
        let b = blocks.min(200);
        println!("render hash — {} tracks, {} blocks", n, b);
        println!("  HASH       : {:016x}", render_hash(n, block, b));
        return;
    }

    if env_usize("NULLHERZ_STUDIO_DETERMINISM", 0) > 0 {
        let n = sweep().last().copied().unwrap_or(32);
        println!("determinism check — {} tracks, {} blocks, rendered twice", n, blocks.min(200));
        determinism(n, block, blocks.min(200));
        return;
    }

    for n in sweep() {
        measure_point(n, block, blocks);
        println!();
    }
}
