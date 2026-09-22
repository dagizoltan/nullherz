use nullherz_traits::{MAX_NODES, MAX_CHANNELS};
pub use nullherz_traits::{CompiledGraphPlan, GraphTopology};
use nullherz_traits::error::AudioError;

pub struct GraphCompiler {}

impl GraphCompiler {
    /// Every buffer a node READS in one `process()` call: its inputs and its
    /// sidechains.
    ///
    /// Sidechains were treated as reads in the executor (which resolves and
    /// passes them) but not by the scheduler, which is how a sidechain could
    /// be both read and written inside one parallel stage. One helper, used by
    /// the dependency build, the stage packer and the hazard verifier, is what
    /// keeps the three from drifting apart again.
    fn read_edges(routing: &nullherz_traits::NodeRouting) -> impl Iterator<Item = nullherz_traits::BufferId> + '_ {
        routing.input_indices.iter().take(routing.input_count.min(nullherz_traits::MAX_CHANNELS))
            .chain(routing.sidechain_indices.iter().take(routing.sidechain_count.min(nullherz_traits::MAX_CHANNELS)))
            .copied()
    }

    pub fn compile(topo: &GraphTopology) -> Result<CompiledGraphPlan, AudioError> {
        let mut plan = CompiledGraphPlan::default();
        let n = topo.node_count;
        if n == 0 { return Ok(plan); }

        // A node's inputs and sidechains share one slot space: they occupy
        // consecutive entries of `node_inputs_storage`, of `input_delays`, and
        // of the PDC ring's per-node rows, all of which are `MAX_CHANNELS` wide.
        // Exceeding it would silently drop the delay compensation for the
        // overflowing slots (and, in the executor, read past the scratch rows).
        // Fail the compile instead; it runs off-thread, so refusing is free.
        for i in 0..n.min(MAX_NODES) {
            let r = &topo.routing[i];
            let combined = r.input_count + r.sidechain_count;
            if combined > MAX_CHANNELS {
                return Err(AudioError::ConfigurationError(format!(
                    "node {} has {} inputs + {} sidechains = {} slots, over MAX_CHANNELS ({})",
                    i, r.input_count, r.sidechain_count, combined, MAX_CHANNELS
                )));
            }
        }

        // Refuse out-of-range buffer indices instead of %-wrapping them below:
        // a wrapped index aliases another edge's buffer and corrupts audio
        // with no error. This runs off-thread, so failing the compile is free.
        for i in 0..n.min(MAX_NODES) {
            let r = &topo.routing[i];
            let bad = r.input_indices.iter().take(r.input_count.min(nullherz_traits::MAX_CHANNELS))
                .chain(r.output_indices.iter().take(r.output_count.min(nullherz_traits::MAX_CHANNELS)))
                .chain(r.sidechain_indices.iter().take(r.sidechain_count.min(nullherz_traits::MAX_CHANNELS)))
                .find(|v| !v.is_valid());
            if let Some(&v) = bad {
                return Err(AudioError::ConfigurationError(format!(
                    "node {} routes buffer {} out of range (MAX_BUFFERS = {})", i, v.0, nullherz_traits::MAX_BUFFERS
                )));
            }
        }

        // PERF-08: Use Boxed arrays to avoid massive stack pressure (~96KB previously)
        let mut in_degree = Box::new([0usize; MAX_NODES]);
        let mut adj = Box::new([[0usize; MAX_NODES]; MAX_NODES]);
        let mut adj_count = Box::new([0usize; MAX_NODES]);

        // 1. Build adjacency list and in-degrees efficiently
        let mut v_to_producers = Box::new([[0usize; MAX_NODES]; nullherz_traits::MAX_BUFFERS]);
        let mut v_producer_counts = Box::new([0usize; nullherz_traits::MAX_BUFFERS]);
        for j in 0..n {
            let routing_j = &topo.routing[j];
            // `.min(MAX_CHANNELS)`: `output_count` is data, and indexing
            // `output_indices[k]` past 16 is a panic. Every other read of these
            // arrays clamps; this one did not.
            for k in 0..routing_j.output_count.min(nullherz_traits::MAX_CHANNELS) {
                let v_out = routing_j.output_indices[k].index();
                if v_out < nullherz_traits::MAX_BUFFERS
                    && v_producer_counts[v_out] < MAX_NODES {
                    v_to_producers[v_out][v_producer_counts[v_out]] = j;
                    v_producer_counts[v_out] += 1;
                }
            }
        }

        for (i, in_degree_val) in in_degree.iter_mut().enumerate().take(n) {
            let routing_i = &topo.routing[i];
            // Side-chain dependency resolution.
            //
            // Sidechains are read by the node exactly like inputs are, so their
            // producers must be scheduled EARLIER — the chain is what this loop
            // builds. It previously walked `input_indices` only, while the
            // comment above it claimed to cover sidechains. That is not a
            // cosmetic gap: `unsafe impl Sync for ProcessorNode` justifies
            // itself with "the topological scheduler guarantees no RAW, WAR, or
            // WAW hazards exist within a parallel stage". Without a sidechain
            // edge, a sidechained compressor can be packed into the same stage
            // as its sidechain producer, and two workers then touch the same
            // buffer — the exact data race that safety comment rules out.
            //
            // Latent rather than live only because every production write site
            // sets `sidechain_count = 0` today. It would have become real with
            // the first sidechain compressor.
            let inputs = routing_i.input_indices.iter().take(routing_i.input_count.min(nullherz_traits::MAX_CHANNELS));
            let sidechains = routing_i.sidechain_indices.iter().take(routing_i.sidechain_count.min(nullherz_traits::MAX_CHANNELS));
            for v_in_id in inputs.chain(sidechains) {
                let v_in = v_in_id.index();
                if v_in < nullherz_traits::MAX_BUFFERS {
                    for &j in v_to_producers[v_in].iter().take(v_producer_counts[v_in]) {
                        if i == j { continue; }
                        let mut exists = false;
                        for &adj_val in adj[j].iter().take(adj_count[j]) {
                            if adj_val == i {
                                exists = true;
                                break;
                            }
                        }
                        if !exists {
                            adj[j][adj_count[j]] = i;
                            adj_count[j] += 1;
                            *in_degree_val += 1;
                        }
                    }
                }
            }
        }

        // 2. Kahn's algorithm with Write-After-Write (WAW) tracking
        let mut processed_count = 0;
        let mut is_processed = Box::new([false; MAX_NODES]);
        plan.num_stages = 0;

        while processed_count < n {
            // Sized by the NODE address space, not the buffer one. It held
            // `MAX_BUFFERS` entries, which was harmless only because a stage
            // cannot contain more nodes than exist — but it is exactly the
            // node-vs-buffer index confusion `BufferId` was introduced to kill,
            // and `plan.stages[..].0` it is copied into is `[u32; MAX_NODES]`.
            let mut stage_nodes = [0u32; MAX_NODES];
            let mut stage_count = 0;
            let mut physical_writes_in_stage = [false; nullherz_traits::MAX_BUFFERS];
            let mut physical_reads_in_stage = [false; nullherz_traits::MAX_BUFFERS];

            for i in 0..n {
                if !is_processed[i] && in_degree[i] == 0 {
                    // PERF-09: Static Graph Pruning
                    // If a node is bypassed, we still need to "process" it for Kahn's
                    // to satisfy dependencies, but we omit it from the execution plan
                    // to reclaim CPU cycles.
                    if topo.bypass_states[i] {
                        is_processed[i] = true;
                        processed_count += 1;
                        for &dependent in adj[i].iter().take(adj_count[i]) {
                            in_degree[dependent] -= 1;
                        }
                        continue;
                    }

                    // Check for RAW/WAR/WAW collision with other nodes in the stage
                    let mut collision = false;
                    let routing = &topo.routing[i];

                    for k in 0..routing.output_count {
                        let v_out = routing.output_indices.get(k).copied().unwrap_or_default().index();
                        let p_out = topo.virtual_to_physical[v_out].index();
                        if physical_writes_in_stage[p_out] || physical_reads_in_stage[p_out] {
                            collision = true;
                            break;
                        }
                    }
                    if collision { continue; }

                    // Reads cover sidechains too — the node consumes them in
                    // the same `process()` call as its inputs.
                    for v_in_id in Self::read_edges(routing) {
                        let p_in = topo.virtual_to_physical[v_in_id.index()].index();
                        if physical_writes_in_stage[p_in] {
                            collision = true;
                            break;
                        }
                    }

                    if !collision {
                        stage_nodes[stage_count] = i as u32;
                        stage_count += 1;
                        for k in 0..routing.output_count.min(nullherz_traits::MAX_CHANNELS) {
                            let v_out = routing.output_indices.get(k).copied().unwrap_or_default().index();
                            let p_out = topo.virtual_to_physical[v_out].index();
                            physical_writes_in_stage[p_out] = true;
                        }
                        for v_in_id in Self::read_edges(routing) {
                            let p_in = topo.virtual_to_physical[v_in_id.index()].index();
                            physical_reads_in_stage[p_in] = true;
                        }
                    }
                }
            }

            if stage_count == 0 { break; } // Cycle detected or no more progress

            for (i, &node_idx) in stage_nodes.iter().enumerate().take(stage_count) {
                plan.stages[plan.num_stages].0[i] = node_idx;
                is_processed[node_idx as usize] = true;
                processed_count += 1;
            }
            plan.stage_counts[plan.num_stages] = stage_count as u32;
            plan.num_stages += 1;

            for &node_idx_u32 in stage_nodes.iter().take(stage_count) {
                let node_idx = node_idx_u32 as usize;
                for &dependent in adj[node_idx].iter().take(adj_count[node_idx]) {
                    in_degree[dependent] -= 1;
                }
            }
        }

        if processed_count < n {
            return Err(AudioError::Generic("Cycle detected in graph".into()));
        }

        // --- NETWORK PROXY INSERTION: REMOVED ---
        //
        // This used to append two stages per cross-machine edge, each holding a
        // synthetic "proxy" id counted up from `MAX_NODES` so it could not
        // collide with a real node. It crashed the audio thread.
        //
        // `plan.stages` is not an annotation, it is the executor's work list:
        // `execute_stage` does `&nodes[n_idx]` against a `[ProcessorNode;
        // MAX_NODES]`. An id of exactly MAX_NODES is an out-of-bounds index, and
        // it lands OUTSIDE the `catch_unwind` that guards `process()`, so the
        // per-node fault isolation does not catch it — on a SCHED_FIFO callback
        // it is a hard stop. `verify_no_hazards` skipped these ids
        // (`if n_idx >= MAX_NODES { continue }`), so compilation reported
        // success and the plan shipped to the RT thread inside `SetTopology`.
        // `TopologyCommand::MigrateNode` is all it took to trigger: it writes
        // `node_assignments` and commits.
        //
        // Nothing consumed the proxies. Distributed audio is routed by
        // `Conductor::process_distributed_audio`, which walks `node_assignments`
        // and moves blocks through `IpcAudioBridge` from the ORCHESTRATION
        // thread; it never reads `plan.stages`. So this was a crash with no
        // feature attached to it.
        //
        // A real implementation needs proxies to be allocated graph nodes with
        // actual processors (like `SidecarProcessor`), inside the node address
        // space, with routing and latency of their own — not ids invented by the
        // compiler. Until then the boundary lives where it is actually used,
        // in `node_assignments`.

        Self::identify_islands(n, &adj, &adj_count, &mut plan);

        Self::calculate_pdc(n, &adj, &adj_count, &mut plan, topo);

        Self::verify_no_hazards(topo, &plan)?;
        Self::verify_stage_ids_in_range(&plan)?;

        Ok(plan)
    }

    /// Every id in the plan must be a legal index into the engine's node array.
    ///
    /// The executor indexes `nodes[n_idx]` directly, so this is the boundary
    /// where an out-of-range id has to stop. Failing the compile costs nothing
    /// — it runs off the audio thread — and the alternative is an index panic
    /// on the RT callback. Checked as its own pass rather than inside the
    /// producer so that ANY future stage-writing code is covered by it.
    pub fn verify_stage_ids_in_range(plan: &CompiledGraphPlan) -> Result<(), AudioError> {
        if plan.num_stages > MAX_NODES {
            return Err(AudioError::ConfigurationError(format!(
                "plan declares {} stages, more than MAX_NODES ({})", plan.num_stages, MAX_NODES
            )));
        }
        for s_idx in 0..plan.num_stages {
            let count = plan.stage_counts[s_idx] as usize;
            if count > MAX_NODES {
                return Err(AudioError::ConfigurationError(format!(
                    "stage {} declares {} nodes, more than MAX_NODES ({})", s_idx, count, MAX_NODES
                )));
            }
            for &id in &plan.stages[s_idx].0[..count] {
                if id as usize >= MAX_NODES {
                    return Err(AudioError::ConfigurationError(format!(
                        "stage {} carries node id {}, out of range for the engine's \
                         node array (MAX_NODES = {}) — the executor would index past it",
                        s_idx, id, MAX_NODES
                    )));
                }
            }
        }
        Ok(())
    }

    fn calculate_pdc(n: usize, adj: &[[usize; MAX_NODES]; MAX_NODES], adj_count: &[usize; MAX_NODES], plan: &mut CompiledGraphPlan, topo: &GraphTopology) {
        let mut path_latencies = [0u32; nullherz_traits::MAX_BUFFERS];

        // 1. Initial pass: Get intrinsic latencies from topo (populated by GraphManager)
        for i in 0..n {
            plan.node_latencies[i] = topo.plan.node_latencies[i];
        }

        // 2. Compute path latencies using topological order
        for s_idx in 0..plan.num_stages {
            for &u_u32 in &plan.stages[s_idx].0[..plan.stage_counts[s_idx] as usize] {
                let u = u_u32 as usize;
                if u >= MAX_NODES { continue; }

                let current_path_lat = path_latencies[u] + plan.node_latencies[u];

                for &v in adj[u].iter().take(adj_count[u]) {
                    path_latencies[v] = path_latencies[v].max(current_path_lat);
                }
            }
        }

        // 2. Determine required delay for each node input to align summing
        // We use a modified topo because we need to know which input comes from which path.
        // Since topo.routing[v].input_indices[i] tells us the virtual buffer,
        // and we can find which node writes to that virtual buffer.

        let mut v_to_producer = [None; nullherz_traits::MAX_BUFFERS];
        for j in 0..n {
            let routing_j = &topo.routing[j];
            for k in 0..routing_j.output_count {
                let v_out = routing_j.output_indices[k].index();
                if v_out < nullherz_traits::MAX_BUFFERS {
                    v_to_producer[v_out] = Some(j);
                }
            }
        }

        // `input_delays` is indexed by COMBINED SLOT: inputs first, then
        // sidechains — the same order `node_inputs_storage` uses in the executor
        // and the worker pool, so slot `k` there is slot `k` here.
        //
        // Sidechains were previously left uncompensated: `read_edges` made them
        // scheduling dependencies (so the producer runs first), but a sidechain
        // arriving from a deeper path still arrived EARLY relative to the node's
        // audio input. For a ducking compressor that means the gain reduction
        // leads the signal it is supposed to be keyed to, by the path-latency
        // difference. Indexing by combined slot gets them compensated without a
        // parallel array, a second `PdcLines` bank, or a change to the
        // serialized plan — the executor's PDC ring already has `MAX_CHANNELS`
        // rows per node, which is exactly the combined budget asserted above.
        for v in 0..n {
            let routing_v = &topo.routing[v];
            let max_v_path_lat = path_latencies[v];
            let inputs = routing_v.input_indices.iter().take(routing_v.input_count.min(MAX_CHANNELS));
            let sidechains = routing_v.sidechain_indices.iter().take(routing_v.sidechain_count.min(MAX_CHANNELS));
            for (slot, v_buf_id) in inputs.chain(sidechains).enumerate() {
                if slot >= MAX_CHANNELS { break; }
                let v_buf = v_buf_id.index();
                if let Some(u) = v_to_producer[v_buf] {
                    let u_path_lat = path_latencies[u] + plan.node_latencies[u];
                    if max_v_path_lat > u_path_lat {
                        plan.input_delays[v].0[slot] = (max_v_path_lat - u_path_lat) as f32;
                    }
                }
            }
        }
    }

    fn identify_islands(n: usize, adj: &[[usize; MAX_NODES]; MAX_NODES], adj_count: &[usize; MAX_NODES], plan: &mut CompiledGraphPlan) {
        let mut visited = [false; MAX_NODES];
        let mut island_id = 0u8;

        for i in 0..n {
            if !visited[i] {
                island_id += 1;
                let mut queue = [0usize; MAX_NODES];
                let mut head = 0;
                let mut tail = 0;

                queue[tail] = i;
                tail += 1;
                visited[i] = true;

                while head < tail {
                    let u = queue[head];
                    head += 1;
                    plan.node_islands[u] = island_id;

                    for &v in adj[u].iter().take(adj_count[u]) {
                        if !visited[v] {
                            visited[v] = true;
                            queue[tail] = v;
                            tail += 1;
                        }
                    }

                    // Also check reverse adjacency to handle undirected islands
                    for v in 0..n {
                        if !visited[v] {
                            for &neighbor in adj[v].iter().take(adj_count[v]) {
                                if neighbor == u {
                                    visited[v] = true;
                                    queue[tail] = v;
                                    tail += 1;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn verify_no_hazards(topo: &GraphTopology, plan: &CompiledGraphPlan) -> Result<(), AudioError> {
        for s_idx in 0..plan.num_stages {
            let stage = &plan.stages[s_idx].0[..plan.stage_counts[s_idx] as usize];
            let mut physical_writes = [false; nullherz_traits::MAX_BUFFERS];
            let mut physical_reads = [false; nullherz_traits::MAX_BUFFERS];

            for &n_idx_u32 in stage {
                let n_idx = n_idx_u32 as usize;
                // An id past the node array is not something to skip over: the
                // executor would index `nodes[n_idx]` with it. Report it.
                if n_idx >= MAX_NODES {
                    return Err(AudioError::ConfigurationError(format!(
                        "stage {} carries node id {}, out of range (MAX_NODES = {})", s_idx, n_idx, MAX_NODES
                    )));
                }
                let routing = &topo.routing[n_idx];

                // Check for RAW/WAR/WAW hazards with OTHER nodes in the same stage.
                // Intra-node reuse is permitted for in-place processing.
                // "Reads" includes sidechains — see `read_edges`.

                for k in 0..routing.output_count.min(nullherz_traits::MAX_CHANNELS) {
                    let v_out = routing.output_indices.get(k).copied().unwrap_or_default().index();
                    let p_out = topo.virtual_to_physical[v_out].index();

                    if physical_writes[p_out] || physical_reads[p_out] {
                        return Err(AudioError::IpcError(format!("Hazard at stage {}. Node {} output collides with physical buffer {} already in use.", s_idx, n_idx, p_out)));
                    }
                }

                for v_in_id in Self::read_edges(routing) {
                    let p_in = topo.virtual_to_physical[v_in_id.index()].index();

                    if physical_writes[p_in] {
                        return Err(AudioError::IpcError(format!("RAW Hazard at stage {}. Node {} input collides with physical buffer {} being written to.", s_idx, n_idx, p_in)));
                    }
                }

                // After checking, MARK them as used by this node for the rest of the stage
                for k in 0..routing.output_count.min(nullherz_traits::MAX_CHANNELS) {
                    let v_out = routing.output_indices.get(k).copied().unwrap_or_default().index();
                    let p_out = topo.virtual_to_physical[v_out].index();
                    physical_writes[p_out] = true;
                }
                for v_in_id in Self::read_edges(routing) {
                    physical_reads[topo.virtual_to_physical[v_in_id.index()].index()] = true;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::{BufferId, GraphTopology, NodeRouting};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_compiler_hazard_detection_robustness(
            v2p in prop::collection::vec(0..MAX_NODES, MAX_NODES),
            writes in prop::collection::vec((0..MAX_NODES, 0..16usize), 1..10),
            reads in prop::collection::vec((0..MAX_NODES, 0..16usize), 1..10)
        ) {
            let mut v2p_arr = [BufferId(0); nullherz_traits::MAX_BUFFERS];
            for (i, &v) in v2p.iter().enumerate() { v2p_arr[i] = BufferId(v as u32); }

            let mut topo = GraphTopology {
                routing: [NodeRouting {
                    input_indices: [BufferId(0); 16],
                    output_indices: [BufferId(0); 16],
                    sidechain_indices: [BufferId(0); 16],
                    input_count: 0,
                    output_count: 0,
                    sidechain_count: 0,
                    input_delays: [0.0; 16],
                }; MAX_NODES],
                virtual_to_physical: v2p_arr,
                plan: CompiledGraphPlan::default(),
                crossfades: [None; 8],
                node_count: 1,
                node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
                node_positions: [None; MAX_NODES],
                bypass_states: [false; MAX_NODES],
            };

            for &(v_out_usize, _) in writes.iter() {
                let v_out = BufferId(v_out_usize as u32);
                if topo.routing[0].output_count < 16 {
                    topo.routing[0].output_indices[topo.routing[0].output_count] = v_out;
                    topo.routing[0].output_count += 1;
                }
            }

            for &(v_in_usize, _) in reads.iter() {
                let v_in = BufferId(v_in_usize as u32);
                if topo.routing[0].input_count < 16 {
                    topo.routing[0].input_indices[topo.routing[0].input_count] = v_in;
                    topo.routing[0].input_count += 1;
                }
            }

            let result = GraphCompiler::compile(&topo);
            assert!(result.is_ok());
        }

        #[test]
        fn test_random_graph_compilation(
            num_nodes in 1..20usize,
            edges in prop::collection::vec((0..20usize, 0..20usize), 0..40)
        ) {
            let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
            for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }

            let mut topo = GraphTopology {
                routing: [NodeRouting {
                    input_indices: [BufferId(0); 16],
                    output_indices: [BufferId(0); 16],
                    sidechain_indices: [BufferId(0); 16],
                    input_count: 0,
                    output_count: 0,
                    sidechain_count: 0,
                    input_delays: [0.0; 16],
                }; MAX_NODES],
                virtual_to_physical: v2p,
                plan: CompiledGraphPlan::default(),
                crossfades: [None; 8],
                node_count: num_nodes,
                node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
                node_positions: [None; MAX_NODES],
                bypass_states: [false; MAX_NODES],
            };

            for (src, dst) in edges {
                let src = src % num_nodes;
                let dst = dst % num_nodes;
                if src == dst { continue; }

                // Create an edge src -> dst using a virtual buffer
                let v_buf = BufferId((src + 10) as u32);
                if topo.routing[src].output_count < 16 && topo.routing[dst].input_count < 16 {
                    topo.routing[src].output_indices[topo.routing[src].output_count] = v_buf;
                    topo.routing[src].output_count += 1;
                    topo.routing[dst].input_indices[topo.routing[dst].input_count] = v_buf;
                    topo.routing[dst].input_count += 1;
                }
            }

            let result = GraphCompiler::compile(&topo);
            // It's either Ok or Err(CycleDetected)
            if let Err(e) = result {
                assert!(e.to_string().contains("Cycle detected") || e.to_string().contains("Hazard"));
            } else if let Ok(plan) = result {
                GraphCompiler::verify_no_hazards(&topo, &plan).expect("Compiled plan has hazards");
            }
        }
    }

    #[test]
    fn test_hazard_detection_raw() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };

        // Node 0 writes to buffer 10
        topo.routing[0].output_indices[0] = BufferId(10);
        topo.routing[0].output_count = 1;

        // Node 1 reads from buffer 10
        topo.routing[1].input_indices[0] = BufferId(10);
        topo.routing[1].input_count = 1;

        // Force them into the same stage in a manually constructed plan
        let mut plan = CompiledGraphPlan {
            num_stages: 1,
            ..Default::default()
        };
        plan.stage_counts[0] = 2;
        plan.stages[0].0[0] = 0;
        plan.stages[0].0[1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("RAW Hazard"));
    }

    #[test]
    fn test_hazard_detection_war() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };

        // Node 0 reads from buffer 10
        topo.routing[0].input_indices[0] = BufferId(10);
        topo.routing[0].input_count = 1;

        // Node 1 writes to buffer 10
        topo.routing[1].output_indices[0] = BufferId(10);
        topo.routing[1].output_count = 1;

        let mut plan = CompiledGraphPlan {
            num_stages: 1,
            ..Default::default()
        };
        plan.stage_counts[0] = 2;
        plan.stages[0].0[0] = 0;
        plan.stages[0].0[1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides with physical buffer 10 already in use"));
    }

    /// A cross-machine edge must never put an unrunnable id in the plan.
    ///
    /// Replaces `test_proxy_injection_on_boundary_cross`, which asserted the
    /// OPPOSITE — that the compiler emits a stage id `>= MAX_NODES`. That id is
    /// what `execute_stage` indexes `nodes[n_idx]` with, so the old test was
    /// pinning an out-of-bounds index on the audio thread as correct behaviour.
    /// `TopologyCommand::MigrateNode` is the reachable path to it.
    #[test]
    fn test_cross_boundary_plan_stays_in_node_range() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut node_assignments = [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES];
        node_assignments[0] = nullherz_traits::NodeAssignment([0; 32]);
        let mut remote_id = [0u8; 32]; remote_id[0] = 1; node_assignments[1] = nullherz_traits::NodeAssignment(remote_id);

        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
            node_assignments,
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };

        // Node 0 (Local) -> Node 1 (Remote) via Buffer 10
        topo.routing[0].output_indices[0] = BufferId(10);
        topo.routing[0].output_count = 1;
        topo.routing[1].input_indices[0] = BufferId(10);
        topo.routing[1].input_count = 1;

        let plan = GraphCompiler::compile(&topo).expect("Compilation failed");

        // The dependency is still honoured: node 0 before node 1.
        assert!(plan.num_stages >= 2, "producer and consumer must be in different stages");

        for s in 0..plan.num_stages {
            for &id in &plan.stages[s].0[..plan.stage_counts[s] as usize] {
                assert!(
                    (id as usize) < MAX_NODES,
                    "stage {s} carries node id {id}; the executor indexes nodes[{id}] \
                     against a [ProcessorNode; {MAX_NODES}] and would panic on the audio thread"
                );
            }
        }
        // And the standalone pass agrees.
        GraphCompiler::verify_stage_ids_in_range(&plan).expect("plan must pass the range gate");
    }

    /// The range gate must reject a hand-built plan, not just a compiled one —
    /// it is the last boundary before the executor.
    #[test]
    fn test_range_gate_rejects_out_of_range_stage_id() {
        let mut plan = CompiledGraphPlan { num_stages: 1, ..Default::default() };
        plan.stage_counts[0] = 1;
        plan.stages[0].0[0] = MAX_NODES as u32;
        let err = GraphCompiler::verify_stage_ids_in_range(&plan)
            .expect_err("an id of exactly MAX_NODES is out of range");
        assert!(err.to_string().contains("out of range"), "got: {err}");
    }

    /// A sidechain from a DEEPER path must be delay-compensated like an input.
    ///
    /// Scheduling it after its producer (the test above) is only half the job: a
    /// sidechain arriving from a shorter path still arrives EARLY relative to the
    /// node's audio input, by the path-latency difference. For a ducking
    /// compressor that means the gain reduction leads the signal it is keyed to.
    ///
    /// `input_delays` is indexed by COMBINED slot — inputs, then sidechains — so
    /// the sidechain's compensation lands at slot `input_count + k`.
    #[test]
    fn test_sidechain_gets_delay_compensation() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 4,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };

        // Node 0: a LATENT source (an FFT insert, say) -> buffer 10.
        topo.routing[0].output_indices[0] = BufferId(10);
        topo.routing[0].output_count = 1;
        topo.plan.node_latencies[0] = 512;

        // Node 1: zero-latency source -> buffer 11. This is the key signal.
        topo.routing[1].output_indices[0] = BufferId(11);
        topo.routing[1].output_count = 1;

        // Node 2: the compressor. Audio input from the LATENT node 0, sidechain
        // key from the instant node 1. The key therefore arrives 512 samples
        // early and must be delayed to match.
        topo.routing[2].input_indices[0] = BufferId(10);
        topo.routing[2].input_count = 1;
        topo.routing[2].sidechain_indices[0] = BufferId(11);
        topo.routing[2].sidechain_count = 1;
        topo.routing[2].output_indices[0] = BufferId(12);
        topo.routing[2].output_count = 1;

        // Node 3 consumes it, so node 2 is not a leaf.
        topo.routing[3].input_indices[0] = BufferId(12);
        topo.routing[3].input_count = 1;
        topo.routing[3].output_indices[0] = BufferId(13);
        topo.routing[3].output_count = 1;

        let plan = GraphCompiler::compile(&topo).expect("compile");

        // Slot 0 is the audio input (from the latent path — already aligned, so
        // no delay). Slot 1 is the sidechain, which must be pushed back by the
        // 512 samples node 0 introduces.
        let sidechain_slot = topo.routing[2].input_count; // == 1
        assert_eq!(
            plan.input_delays[2].0[sidechain_slot], 512.0,
            "the sidechain key was not delay-compensated: slot delays are {:?}",
            &plan.input_delays[2].0[..4]
        );
    }

    /// Inputs and sidechains share one `MAX_CHANNELS`-wide slot space. Going
    /// over it would silently drop compensation for the overflow, so the compile
    /// must refuse rather than truncate.
    #[test]
    fn test_combined_slot_overflow_is_rejected() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 1,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };
        topo.routing[0].input_count = 10;
        topo.routing[0].sidechain_count = 10; // 20 > MAX_CHANNELS

        let err = GraphCompiler::compile(&topo).expect_err("20 slots must be refused");
        assert!(err.to_string().contains("MAX_CHANNELS"), "got: {err}");
    }

    /// A sidechain is a read, so its producer must be scheduled earlier.
    ///
    /// Without a dependency edge the two land in one stage, and the parallel
    /// executor then has one worker writing the buffer while another reads it —
    /// the data race `unsafe impl Sync for ProcessorNode` claims cannot happen.
    #[test]
    fn test_sidechain_producer_is_scheduled_before_consumer() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };

        // Node 0 produces buffer 10. Node 1 takes buffer 10 as a SIDECHAIN only
        // (its audio input is an unrelated buffer) — a ducking compressor.
        topo.routing[0].output_indices[0] = BufferId(10);
        topo.routing[0].output_count = 1;
        topo.routing[1].input_indices[0] = BufferId(20);
        topo.routing[1].input_count = 1;
        topo.routing[1].sidechain_indices[0] = BufferId(10);
        topo.routing[1].sidechain_count = 1;
        topo.routing[1].output_indices[0] = BufferId(21);
        topo.routing[1].output_count = 1;

        let plan = GraphCompiler::compile(&topo).expect("Compilation failed");

        let stage_of = |target: u32| -> usize {
            (0..plan.num_stages)
                .find(|&s| plan.stages[s].0[..plan.stage_counts[s] as usize].contains(&target))
                .unwrap_or_else(|| panic!("node {target} missing from the plan"))
        };
        assert!(
            stage_of(0) < stage_of(1),
            "the sidechain producer (node 0, stage {}) must run before its consumer \
             (node 1, stage {}) — same stage means a concurrent read/write of buffer 10",
            stage_of(0), stage_of(1)
        );
    }


    #[test]
    fn test_static_graph_pruning() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };

        // Node 0 (active) -> Node 1 (bypassed)
        topo.routing[0].output_indices[0] = BufferId(10);
        topo.routing[0].output_count = 1;
        topo.routing[1].input_indices[0] = BufferId(10);
        topo.routing[1].input_count = 1;
        topo.bypass_states[1] = true;

        let plan = GraphCompiler::compile(&topo).expect("Compilation failed");

        // Node 0 should be in the plan, Node 1 should be pruned
        let mut node_0_found = false;
        let mut node_1_found = false;

        for s in 0..plan.num_stages {
            for &node_idx in &plan.stages[s].0[..plan.stage_counts[s] as usize] {
                if node_idx == 0 { node_0_found = true; }
                if node_idx == 1 { node_1_found = true; }
            }
        }

        assert!(node_0_found);
        assert!(!node_1_found, "Bypassed node was not pruned from the execution plan");
    }

    #[test]
    fn test_hazard_detection_waw() {
        let mut v2p = [BufferId(0); nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = BufferId(i as u32); }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); 16],
                output_indices: [BufferId(0); 16],
                sidechain_indices: [BufferId(0); 16],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; 16],
            }; MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES],
            node_positions: [None; MAX_NODES],
            bypass_states: [false; MAX_NODES],
        };

        // Both nodes write to buffer 10
        topo.routing[0].output_indices[0] = BufferId(10);
        topo.routing[0].output_count = 1;
        topo.routing[1].output_indices[0] = BufferId(10);
        topo.routing[1].output_count = 1;

        let mut plan = CompiledGraphPlan {
            num_stages: 1,
            ..Default::default()
        };
        plan.stage_counts[0] = 2;
        plan.stages[0].0[0] = 0;
        plan.stages[0].0[1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides with physical buffer 10 already in use"));
    }
}
