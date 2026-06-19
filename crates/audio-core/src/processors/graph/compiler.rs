use crate::processors::graph::{GraphTopology};
use nullherz_traits::error::AudioError;

#[derive(Debug, Clone, Copy)]
pub struct CompiledGraphPlan {
    pub stages: [[usize; crate::MAX_NODES]; crate::MAX_NODES],
    pub stage_counts: [usize; crate::MAX_NODES],
    pub num_stages: usize,
}

impl Default for CompiledGraphPlan {
    fn default() -> Self {
        Self {
            stages: [[0; crate::MAX_NODES]; crate::MAX_NODES],
            stage_counts: [0; crate::MAX_NODES],
            num_stages: 0,
        }
    }
}

pub struct GraphCompiler {}

impl GraphCompiler {
    pub fn compile(topo: &GraphTopology) -> Result<CompiledGraphPlan, AudioError> {
        let mut plan = CompiledGraphPlan::default();
        let n = topo.node_count;
        if n == 0 { return Ok(plan); }

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
        plan.num_stages = 0;

        while processed_count < n {
            let mut stage_nodes = [0usize; crate::MAX_NODES];
            let mut stage_count = 0;
            let mut physical_writes_in_stage = [false; crate::MAX_NODES];
            let mut physical_reads_in_stage = [false; crate::MAX_NODES];

            for i in 0..n {
                if !is_processed[i] && in_degree[i] == 0 {
                    // Check for RAW/WAR/WAW collision with other nodes in the stage
                    let mut collision = false;
                    let routing = &topo.routing[i];

                    for k in 0..routing.output_count {
                        let v_out = *routing.output_indices.get(k).unwrap_or(&0) % 64;
                        let p_out = topo.virtual_to_physical[v_out as usize];
                        if physical_writes_in_stage[p_out] || physical_reads_in_stage[p_out] {
                            collision = true;
                            break;
                        }
                    }
                    if collision { continue; }

                    for k in 0..routing.input_count {
                        let v_in = *routing.input_indices.get(k).unwrap_or(&0) % 64;
                        let p_in = topo.virtual_to_physical[v_in as usize];
                        if physical_writes_in_stage[p_in] {
                            collision = true;
                            break;
                        }
                    }

                    if !collision {
                        stage_nodes[stage_count] = i;
                        stage_count += 1;
                        for k in 0..routing.output_count {
                            let v_out = *routing.output_indices.get(k).unwrap_or(&0) % 64;
                            let p_out = topo.virtual_to_physical[v_out as usize];
                            physical_writes_in_stage[p_out] = true;
                        }
                        for k in 0..routing.input_count {
                            let v_in = *routing.input_indices.get(k).unwrap_or(&0) % 64;
                            let p_in = topo.virtual_to_physical[v_in as usize];
                            physical_reads_in_stage[p_in] = true;
                        }
                    }
                }
            }

            if stage_count == 0 { break; } // Cycle detected or no more progress

            for (i, &node_idx) in stage_nodes.iter().enumerate().take(stage_count) {
                plan.stages[plan.num_stages][i] = node_idx;
                is_processed[node_idx] = true;
                processed_count += 1;
            }
            plan.stage_counts[plan.num_stages] = stage_count;
            plan.num_stages += 1;

            for &node_idx in stage_nodes.iter().take(stage_count) {
                for &dependent in adj[node_idx].iter().take(adj_count[node_idx]) {
                    in_degree[dependent] -= 1;
                }
            }
        }

        if processed_count < n {
            return Err(AudioError::Generic("Cycle detected in graph".into()));
        }

        Self::verify_no_hazards(topo, &plan)?;

        Ok(plan)
    }

    pub fn verify_no_hazards(topo: &GraphTopology, plan: &CompiledGraphPlan) -> Result<(), AudioError> {
        for s_idx in 0..plan.num_stages {
            let stage = &plan.stages[s_idx][..plan.stage_counts[s_idx]];
            let mut physical_writes = [false; crate::MAX_NODES];
            let mut physical_reads = [false; crate::MAX_NODES];

            for &n_idx in stage {
                let routing = &topo.routing[n_idx];

                // Check for RAW/WAR/WAW hazards with OTHER nodes in the same stage.
                // Intra-node reuse is permitted for in-place processing.

                for k in 0..routing.output_count {
                    let v_out = *routing.output_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_out = *topo.virtual_to_physical.get(v_out).unwrap_or(&0).min(&(crate::MAX_NODES - 1));

                    if physical_writes[p_out] || physical_reads[p_out] {
                        return Err(AudioError::IpcError(format!("Hazard at stage {}. Node {} output collides with physical buffer {} already in use.", s_idx, n_idx, p_out)));
                    }
                }

                for k in 0..routing.input_count {
                    let v_in = *routing.input_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_in = *topo.virtual_to_physical.get(v_in).unwrap_or(&0).min(&(crate::MAX_NODES - 1));

                    if physical_writes[p_in] {
                        return Err(AudioError::IpcError(format!("RAW Hazard at stage {}. Node {} input collides with physical buffer {} being written to.", s_idx, n_idx, p_in)));
                    }
                }

                // After checking, MARK them as used by this node for the rest of the stage
                for k in 0..routing.output_count {
                    let v_out = *routing.output_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_out = *topo.virtual_to_physical.get(v_out).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    physical_writes[p_out] = true;
                }
                for k in 0..routing.input_count {
                    let v_in = *routing.input_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_in = *topo.virtual_to_physical.get(v_in).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
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
    use crate::processors::graph::{GraphTopology, NodeRouting};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_compiler_hazard_detection_robustness(
            v2p in prop::collection::vec(0..64usize, 64),
            writes in prop::collection::vec((0..64usize, 0..16usize), 1..10),
            reads in prop::collection::vec((0..64usize, 0..16usize), 1..10)
        ) {
            let mut v2p_arr = [0usize; 64];
            v2p_arr.copy_from_slice(&v2p);

            let mut topo = GraphTopology {
                routing: [NodeRouting { input_indices: [0; 16], output_indices: [0; 16], input_count: 0, output_count: 0 }; 64],
                virtual_to_physical: v2p_arr,
                plan: CompiledGraphPlan::default(),
                crossfades: [None; 8],
                node_count: 1,
            };

            for (v_out, _) in writes {
                if topo.routing[0].output_count < 16 {
                    topo.routing[0].output_indices[topo.routing[0].output_count] = v_out;
                    topo.routing[0].output_count += 1;
                }
            }

            for (v_in, _) in reads {
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
            let mut v2p = [0usize; 64];
            for (i, val) in v2p.iter_mut().enumerate() { *val = i; }

            let mut topo = GraphTopology {
                routing: [NodeRouting { input_indices: [0; 16], output_indices: [0; 16], input_count: 0, output_count: 0 }; 64],
                virtual_to_physical: v2p,
                plan: CompiledGraphPlan::default(),
                crossfades: [None; 8],
                node_count: num_nodes,
            };

            for (src, dst) in edges {
                let src = src % num_nodes;
                let dst = dst % num_nodes;
                if src == dst { continue; }

                // Create an edge src -> dst using a virtual buffer
                let v_buf = src + 10;
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
        let mut v2p = [0usize; 64];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i; }
        let mut topo = GraphTopology {
            routing: [NodeRouting { input_indices: [0; 16], output_indices: [0; 16], input_count: 0, output_count: 0 }; 64],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
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
        plan.stages[0][0] = 0;
        plan.stages[0][1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("RAW Hazard"));
    }

    #[test]
    fn test_hazard_detection_war() {
        let mut v2p = [0usize; 64];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i; }
        let mut topo = GraphTopology {
            routing: [NodeRouting { input_indices: [0; 16], output_indices: [0; 16], input_count: 0, output_count: 0 }; 64],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
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
        plan.stages[0][0] = 0;
        plan.stages[0][1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides with physical buffer 10 already in use"));
    }

    #[test]
    fn test_hazard_detection_waw() {
        let mut v2p = [0usize; 64];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i; }
        let mut topo = GraphTopology {
            routing: [NodeRouting { input_indices: [0; 16], output_indices: [0; 16], input_count: 0, output_count: 0 }; 64],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; 8],
            node_count: 2,
        };

        // Both nodes write to buffer 10
        topo.routing[0].output_indices[0] = 10;
        topo.routing[0].output_count = 1;
        topo.routing[1].output_indices[0] = 10;
        topo.routing[1].output_count = 1;

        let mut plan = CompiledGraphPlan::default();
        plan.num_stages = 1;
        plan.stage_counts[0] = 2;
        plan.stages[0][0] = 0;
        plan.stages[0][1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides with physical buffer 10 already in use"));
    }
}
