use crate::processors::graph::{GraphTopology};
use crate::error::AudioError;

pub struct GraphCompiler {}

impl GraphCompiler {
    pub fn calculate_stages(topo: &mut GraphTopology) {
        let n = topo.node_count;
        if n == 0 { return; }

        let mut in_degree = [0usize; crate::MAX_NODES];
        let mut adj = [[0usize; crate::MAX_NODES]; crate::MAX_NODES];
        let mut adj_count = [0usize; crate::MAX_NODES];

        // 1. Build adjacency list and in-degrees efficiently
        let mut v_to_producers = [[0usize; crate::MAX_NODES]; crate::MAX_NODES];
        let mut v_producer_counts = [0usize; crate::MAX_NODES];
        for j in 0..n {
            let routing_j = &topo.routing[j];
            for k in 0..routing_j.output_count {
                let v_out = routing_j.output_indices[k];
                if v_out < crate::MAX_NODES {
                    v_to_producers[v_out][v_producer_counts[v_out]] = j;
                    v_producer_counts[v_out] += 1;
                }
            }
        }

        for (i, in_degree_val) in in_degree.iter_mut().enumerate().take(n) {
            let routing_i = &topo.routing[i];
            for l in 0..routing_i.input_count {
                let v_in = routing_i.input_indices[l];
                if v_in < crate::MAX_NODES {
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
        let mut is_processed = [false; crate::MAX_NODES];
        topo.num_stages = 0;

        while processed_count < n {
            let mut stage_nodes = [0usize; crate::MAX_NODES];
            let mut stage_count = 0;
            let mut physical_buffers_in_stage = [false; crate::MAX_NODES];

            for i in 0..n {
                if !is_processed[i] && in_degree[i] == 0 {
                    // Check for WAW collision on physical buffers
                    let mut collision = false;
                    let routing = &topo.routing[i];
                    for k in 0..routing.output_count {
                        let v_out = routing.output_indices[k].min(63);
                        let p_out = topo.virtual_to_physical[v_out].min(63);
                        if physical_buffers_in_stage[p_out] {
                            collision = true;
                            break;
                        }
                    }

                    if !collision {
                        stage_nodes[stage_count] = i;
                        stage_count += 1;
                        for k in 0..routing.output_count {
                            let v_out = routing.output_indices[k].min(63);
                            let p_out = topo.virtual_to_physical[v_out].min(63);
                            physical_buffers_in_stage[p_out] = true;
                        }
                    }
                }
            }

            if stage_count == 0 { break; } // Cycle detected or no more progress

            for (i, &node_idx) in stage_nodes.iter().enumerate().take(stage_count) {
                topo.stages[topo.num_stages][i] = node_idx;
                is_processed[node_idx] = true;
                processed_count += 1;
            }
            topo.stage_counts[topo.num_stages] = stage_count;
            topo.num_stages += 1;

            for &node_idx in stage_nodes.iter().take(stage_count) {
                for &dependent in adj[node_idx].iter().take(adj_count[node_idx]) {
                    in_degree[dependent] -= 1;
                }
            }
        }
    }

    pub fn verify_no_hazards(topo: &GraphTopology) -> Result<(), AudioError> {
        for s_idx in 0..topo.num_stages {
            let stage = &topo.stages[s_idx][..topo.stage_counts[s_idx]];
            let mut physical_writes = [false; crate::MAX_NODES];
            let mut physical_reads = [false; crate::MAX_NODES];

            for &n_idx in stage {
                let routing = &topo.routing[n_idx];

                // 1. Check for RAW (Read-After-Write) and WAR (Write-After-Read) Hazards
                for k in 0..routing.output_count {
                    let v_out = *routing.output_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_out = *topo.virtual_to_physical.get(v_out).unwrap_or(&0).min(&(crate::MAX_NODES - 1));

                    if physical_writes[p_out] {
                        return Err(AudioError::IpcError(format!("WAW Hazard at stage {}. Node {} attempts to write to physical buffer {} which is already used for writing in this stage.", s_idx, n_idx, p_out)));
                    }
                    if physical_reads[p_out] {
                        return Err(AudioError::IpcError(format!("WAR Hazard at stage {}. Node {} attempts to write to physical buffer {} which is already being read in this stage.", s_idx, n_idx, p_out)));
                    }
                    physical_writes[p_out] = true;
                }

                for k in 0..routing.input_count {
                    let v_in = *routing.input_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_in = *topo.virtual_to_physical.get(v_in).unwrap_or(&0).min(&(crate::MAX_NODES - 1));

                    if physical_writes[p_in] {
                        return Err(AudioError::IpcError(format!("RAW Hazard at stage {}. Node {} attempts to read from physical buffer {} which is being written to in this stage.", s_idx, n_idx, p_in)));
                    }
                    physical_reads[p_in] = true;
                }
            }
        }
        Ok(())
    }
}
