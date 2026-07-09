use nullherz_traits::MAX_NODES;
pub use nullherz_traits::{CompiledGraphPlan, GraphTopology};
use nullherz_traits::error::AudioError;

pub struct GraphCompiler {}

impl GraphCompiler {
    pub fn compile(topo: &GraphTopology) -> Result<CompiledGraphPlan, AudioError> {
        let mut plan = CompiledGraphPlan::default();
        let n = topo.node_count;
        if n == 0 { return Ok(plan); }

        // PERF-08: Use Boxed arrays to avoid massive stack pressure (~96KB previously)
        let mut in_degree = Box::new([0usize; MAX_NODES]);
        let mut adj = Box::new([[0usize; MAX_NODES]; MAX_NODES]);
        let mut adj_count = Box::new([0usize; MAX_NODES]);

        // 1. Build adjacency list and in-degrees efficiently
        let mut v_to_producers = Box::new([[0usize; MAX_NODES]; MAX_NODES]);
        let mut v_producer_counts = Box::new([0usize; MAX_NODES]);
        for j in 0..n {
            let routing_j = &topo.routing[j];
            for k in 0..routing_j.output_count {
                let v_out = routing_j.output_indices[k] as usize;
                if v_out < MAX_NODES {
                    v_to_producers[v_out][v_producer_counts[v_out]] = j;
                    v_producer_counts[v_out] += 1;
                }
            }
        }

        for (i, in_degree_val) in in_degree.iter_mut().enumerate().take(n) {
            let routing_i = &topo.routing[i];
            for l in 0..routing_i.input_count {
                let v_in = routing_i.input_indices[l] as usize;
                if v_in < MAX_NODES {
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
            let mut stage_nodes = [0u32; MAX_NODES];
            let mut stage_count = 0;
            let mut physical_writes_in_stage = [false; MAX_NODES];
            let mut physical_reads_in_stage = [false; MAX_NODES];

            for i in 0..n {
                if !is_processed[i] && in_degree[i] == 0 {
                    // Check for RAW/WAR/WAW collision with other nodes in the stage
                    let mut collision = false;
                    let routing = &topo.routing[i];

                    for k in 0..routing.output_count {
                        let v_out = (*routing.output_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                        let p_out = topo.virtual_to_physical[v_out] as usize;
                        if physical_writes_in_stage[p_out] || physical_reads_in_stage[p_out] {
                            collision = true;
                            break;
                        }
                    }
                    if collision { continue; }

                    for k in 0..routing.input_count {
                        let v_in = (*routing.input_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                        let p_in = topo.virtual_to_physical[v_in] as usize;
                        if physical_writes_in_stage[p_in] {
                            collision = true;
                            break;
                        }
                    }

                    if !collision {
                        stage_nodes[stage_count] = i as u32;
                        stage_count += 1;
                        for k in 0..routing.output_count {
                            let v_out = (*routing.output_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                            let p_out = topo.virtual_to_physical[v_out] as usize;
                            physical_writes_in_stage[p_out] = true;
                        }
                        for k in 0..routing.input_count {
                            let v_in = (*routing.input_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                            let p_in = topo.virtual_to_physical[v_in] as usize;
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

        // --- STAGE 3: NETWORK PROXY INSERTION ---
        // Synthetic Proxy IDs start above MAX_NODES to avoid collision
        let mut next_proxy_id = MAX_NODES as u32;

        for node_idx in 0..n {
            let local_assignment = &topo.node_assignments[node_idx];
            let routing = &topo.routing[node_idx];

            for &v_out_u32 in routing.output_indices.iter().take(routing.output_count) {
                let v_out = v_out_u32 as usize;
                for consumer_idx in 0..n {
                    if consumer_idx == node_idx { continue; }
                    let consumer_assignment = &topo.node_assignments[consumer_idx];

                    if local_assignment != consumer_assignment {
                        let consumer_routing = &topo.routing[consumer_idx];
                        if consumer_routing.input_indices.iter().take(consumer_routing.input_count).any(|&v_in| v_in as usize == v_out) {
                            // Boundary crossed! In a real graph, we insert a proxy node into the plan.
                            // Since CompiledGraphPlan uses fixed-size arrays, we'd need to expand it or use reserved slots.
                            // For now, we inject a virtual stage for the transfer.
                            if plan.num_stages < MAX_NODES - 1 {
                                let p_idx = plan.num_stages;
                                plan.stages[p_idx].0[0] = next_proxy_id as u32; // PROXY_SEND or PROXY_RECV
                                plan.stage_counts[p_idx] = 1;
                                plan.num_stages += 1;
                                next_proxy_id += 1;
                            }
                        }
                    }
                }
            }
        }

        Self::identify_islands(n, &adj, &adj_count, &mut plan);

        Self::calculate_pdc(n, &adj, &adj_count, &mut plan, topo);

        Self::verify_no_hazards(topo, &plan)?;

        Ok(plan)
    }

    fn calculate_pdc(n: usize, adj: &[[usize; MAX_NODES]; MAX_NODES], adj_count: &[usize; MAX_NODES], plan: &mut CompiledGraphPlan, topo: &GraphTopology) {
        let mut path_latencies = [0u32; MAX_NODES];

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

        let mut v_to_producer = [None; MAX_NODES];
        for j in 0..n {
            let routing_j = &topo.routing[j];
            for k in 0..routing_j.output_count {
                let v_out = routing_j.output_indices[k] as usize;
                if v_out < MAX_NODES {
                    v_to_producer[v_out] = Some(j);
                }
            }
        }

        for v in 0..n {
            let routing_v = &topo.routing[v];
            let max_v_path_lat = path_latencies[v];
            for i in 0..routing_v.input_count {
                let v_buf = routing_v.input_indices[i] as usize;
                if let Some(u) = v_to_producer[v_buf] {
                    let u_path_lat = path_latencies[u] + plan.node_latencies[u];
                    if max_v_path_lat > u_path_lat {
                        plan.input_delays[v].0[i] = max_v_path_lat - u_path_lat;
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
            let mut physical_writes = [false; MAX_NODES];
            let mut physical_reads = [false; MAX_NODES];

            for &n_idx_u32 in stage {
                let n_idx = n_idx_u32 as usize;
                if n_idx >= MAX_NODES { continue; } // Skip synthetic proxy nodes for hazard check
                let routing = &topo.routing[n_idx];

                // Check for RAW/WAR/WAW hazards with OTHER nodes in the same stage.
                // Intra-node reuse is permitted for in-place processing.

                for k in 0..routing.output_count {
                    let v_out = (*routing.output_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                    let p_out = topo.virtual_to_physical[v_out] as usize;

                    if physical_writes[p_out] || physical_reads[p_out] {
                        return Err(AudioError::IpcError(format!("Hazard at stage {}. Node {} output collides with physical buffer {} already in use.", s_idx, n_idx, p_out)));
                    }
                }

                for k in 0..routing.input_count {
                    let v_in = (*routing.input_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                    let p_in = topo.virtual_to_physical[v_in] as usize;

                    if physical_writes[p_in] {
                        return Err(AudioError::IpcError(format!("RAW Hazard at stage {}. Node {} input collides with physical buffer {} being written to.", s_idx, n_idx, p_in)));
                    }
                }

                // After checking, MARK them as used by this node for the rest of the stage
                for k in 0..routing.output_count {
                    let v_out = (*routing.output_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                    let p_out = topo.virtual_to_physical[v_out] as usize;
                    physical_writes[p_out] = true;
                }
                for k in 0..routing.input_count {
                    let v_in = (*routing.input_indices.get(k).unwrap_or(&0) % MAX_NODES as u32) as usize;
                    let p_in = topo.virtual_to_physical[v_in] as usize;
                    physical_reads[p_in] = true;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::{GraphTopology, NodeRouting};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_compiler_hazard_detection_robustness(
            v2p in prop::collection::vec(0..MAX_NODES, MAX_NODES),
            writes in prop::collection::vec((0..MAX_NODES, 0..16usize), 1..10),
            reads in prop::collection::vec((0..MAX_NODES, 0..16usize), 1..10)
        ) {
            let mut v2p_arr = [0u32; MAX_NODES];
            for (i, &v) in v2p.iter().enumerate() { v2p_arr[i] = v as u32; }

            let mut topo = GraphTopology {
                routing: [NodeRouting {
                    input_indices: [0; 16],
                    output_indices: [0; 16],
                    input_count: 0,
                    output_count: 0,
                    input_delays: [0; 16],
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
                let v_out = v_out_usize as u32;
                if topo.routing[0].output_count < 16 {
                    topo.routing[0].output_indices[topo.routing[0].output_count] = v_out;
                    topo.routing[0].output_count += 1;
                }
            }

            for &(v_in_usize, _) in reads.iter() {
                let v_in = v_in_usize as u32;
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
            let mut v2p = [0u32; MAX_NODES];
            for (i, val) in v2p.iter_mut().enumerate() { *val = i as u32; }

            let mut topo = GraphTopology {
                routing: [NodeRouting {
                    input_indices: [0; 16],
                    output_indices: [0; 16],
                    input_count: 0,
                    output_count: 0,
                    input_delays: [0; 16],
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
                let v_buf = (src + 10) as u32;
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
        let mut v2p = [0u32; MAX_NODES];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i as u32; }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [0; 16],
                output_indices: [0; 16],
                input_count: 0,
                output_count: 0,
                input_delays: [0; 16],
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
        topo.routing[0].output_indices[0] = 10;
        topo.routing[0].output_count = 1;

        // Node 1 reads from buffer 10
        topo.routing[1].input_indices[0] = 10;
        topo.routing[1].input_count = 1;

        // Force them into the same stage in a manually constructed plan
        let mut plan = CompiledGraphPlan::default();
        plan.num_stages = 1;
        plan.stage_counts[0] = 2;
        plan.stages[0].0[0] = 0;
        plan.stages[0].0[1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("RAW Hazard"));
    }

    #[test]
    fn test_hazard_detection_war() {
        let mut v2p = [0u32; MAX_NODES];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i as u32; }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [0; 16],
                output_indices: [0; 16],
                input_count: 0,
                output_count: 0,
                input_delays: [0; 16],
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
        topo.routing[0].input_indices[0] = 10;
        topo.routing[0].input_count = 1;

        // Node 1 writes to buffer 10
        topo.routing[1].output_indices[0] = 10;
        topo.routing[1].output_count = 1;

        let mut plan = CompiledGraphPlan::default();
        plan.num_stages = 1;
        plan.stage_counts[0] = 2;
        plan.stages[0].0[0] = 0;
        plan.stages[0].0[1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides with physical buffer 10 already in use"));
    }

    #[test]
    fn test_proxy_injection_on_boundary_cross() {
        let mut v2p = [0u32; MAX_NODES];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i as u32; }
        let mut node_assignments = [nullherz_traits::NodeAssignment([0; 32]); MAX_NODES];
        node_assignments[0] = nullherz_traits::NodeAssignment([0; 32]);
        let mut remote_id = [0u8; 32]; remote_id[0] = 1; node_assignments[1] = nullherz_traits::NodeAssignment(remote_id);

        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [0; 16],
                output_indices: [0; 16],
                input_count: 0,
                output_count: 0,
                input_delays: [0; 16],
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
        topo.routing[0].output_indices[0] = 10;
        topo.routing[0].output_count = 1;
        topo.routing[1].input_indices[0] = 10;
        topo.routing[1].input_count = 1;

        let plan = GraphCompiler::compile(&topo).expect("Compilation failed");

        // Stage 0: Node 0
        // Stage 1: Proxy Injection (since cross-boundary)
        // Stage 2: Node 1
        assert!(plan.num_stages >= 2);
        let mut proxy_detected = false;
        for s in 0..plan.num_stages {
            if plan.stages[s].0[0] >= MAX_NODES as u32 {
                proxy_detected = true;
            }
        }
        assert!(proxy_detected, "Proxy node was not injected for cross-boundary edge");
    }

    #[test]
    fn test_hazard_detection_waw() {
        let mut v2p = [0u32; MAX_NODES];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i as u32; }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [0; 16],
                output_indices: [0; 16],
                input_count: 0,
                output_count: 0,
                input_delays: [0; 16],
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
        topo.routing[0].output_indices[0] = 10;
        topo.routing[0].output_count = 1;
        topo.routing[1].output_indices[0] = 10;
        topo.routing[1].output_count = 1;

        let mut plan = CompiledGraphPlan::default();
        plan.num_stages = 1;
        plan.stage_counts[0] = 2;
        plan.stages[0].0[0] = 0;
        plan.stages[0].0[1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides with physical buffer 10 already in use"));
    }
}
