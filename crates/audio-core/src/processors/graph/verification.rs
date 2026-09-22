//! Executor invariants, as tests that run.
//!
//! # What was here before
//!
//! Two Kani harnesses, `prove_execute_stage_no_hazards` and
//! `prove_execute_stage_bounds_safety`, gated on `cfg(kani)` — which only the
//! kani driver sets, so ordinary CI never type-checked them. They had bit-rotted
//! against the types they verified: `ProcessorNode::new_empty()` (no such
//! function), `AudioBlock` where the pool holds `RenderBlock`,
//! `plan.stages[0][0]` where `StageNodes` needs `.0`. `ARCHITECTURE.md` cited
//! them as proof of the scary invariants for months while they could not compile.
//!
//! And even had they compiled, neither proved much. The "no hazards" one built a
//! plan by hand and then asserted a property of that plan without invoking the
//! executor at all — `verify_no_hazards` and its proptests cover the same ground
//! directly. The "bounds safety" one DID call `execute_stage`, but against
//! `GraphTopology::default()`: zero nodes, zero stages, so the stage loop had
//! nothing to iterate and the symbolic `num_samples`/`offset` reached no
//! indexing arithmetic.
//!
//! # What is here now
//!
//! The one property worth keeping — the executor's slicing arithmetic holds for
//! ANY block geometry — as a proptest against a POPULATED graph. That is the
//! class of bug that actually bit: per `buffer_pool.rs`, every device period
//! above 256 frames once indexed `data[offset..offset + 1024]` on a 256-element
//! array and panicked on the audio thread. A randomized sweep over
//! `(num_samples, offset)` with real routing exercises exactly that, runs in
//! normal CI, and cannot rot invisibly.

#[cfg(test)]
mod tests {
    use crate::processors::graph::{
        buffer_pool::{PdcLines, RenderBlock},
        GraphExecutor, GraphTopology, ProcessorNode,
    };
    use crate::processors::AudioProcessor;
    use crate::processors::graph::DummyProcessor;
    use nullherz_traits::{BufferId, CompiledGraphPlan, NodeAssignment, NodeRouting, SignalProcessor};
    use proptest::prelude::*;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    /// Writes its input to its output, touching every sample of the slices it is
    /// handed — so a wrong slice length shows up as a panic rather than as
    /// silence.
    struct TouchEverything;
    impl SignalProcessor for TouchEverything {
        fn process(
            &mut self,
            inputs: &[&[f32]],
            outputs: &mut [&mut [f32]],
            _c: &mut nullherz_traits::ProcessContext,
        ) {
            for (o, i) in outputs.iter_mut().zip(inputs.iter()) {
                let n = o.len().min(i.len());
                for k in 0..n {
                    o[k] = i[k] * 0.5;
                }
            }
        }
    }
    impl nullherz_traits::MidiResponder for TouchEverything {}
    impl nullherz_traits::SnapshotProvider for TouchEverything {}
    impl AudioProcessor for TouchEverything {
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    /// A real chain: `n` stereo nodes, node i reading buffers (2i, 2i+1) and
    /// writing (2i+2, 2i+3), compiled into a genuine multi-stage plan.
    fn populated(n: usize) -> GraphTopology {
        let mut v2p = [BufferId(0); crate::MAX_BUFFERS];
        for (i, v) in v2p.iter_mut().enumerate() {
            *v = BufferId(i as u32);
        }
        let mut topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [BufferId(0); crate::MAX_CHANNELS],
                output_indices: [BufferId(0); crate::MAX_CHANNELS],
                sidechain_indices: [BufferId(0); crate::MAX_CHANNELS],
                input_count: 0,
                output_count: 0,
                sidechain_count: 0,
                input_delays: [0.0; crate::MAX_CHANNELS],
            }; crate::MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; crate::MAX_CROSSFADE_BUFFERS],
            node_count: n,
            node_assignments: [NodeAssignment([0; 32]); crate::MAX_NODES],
            node_positions: [None; crate::MAX_NODES],
            bypass_states: [false; crate::MAX_NODES],
        };
        for i in 0..n {
            topo.routing[i].input_indices[0] = BufferId((2 * i) as u32);
            topo.routing[i].input_indices[1] = BufferId((2 * i + 1) as u32);
            topo.routing[i].input_count = 2;
            topo.routing[i].output_indices[0] = BufferId((2 * i + 2) as u32);
            topo.routing[i].output_indices[1] = BufferId((2 * i + 3) as u32);
            topo.routing[i].output_count = 2;
        }
        topo.plan = nullherz_topology::GraphCompiler::compile(&topo).expect("compile");
        topo
    }

    fn nodes_with(n: usize) -> Box<[ProcessorNode; crate::MAX_NODES]> {
        let nodes: Box<[ProcessorNode; crate::MAX_NODES]> =
            Box::new(std::array::from_fn(|i| ProcessorNode {
                processor: std::cell::UnsafeCell::new(if i < n {
                    Box::new(TouchEverything) as Box<dyn AudioProcessor>
                } else {
                    Box::new(DummyProcessor) as Box<dyn AudioProcessor>
                }),
            }));
        nodes
    }

    fn boxed<const N: usize>() -> Box<[RenderBlock; N]> {
        vec![RenderBlock::silent(); N]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!())
    }

    proptest! {
        // Deliberately few cases: each one builds a graph and runs every stage.
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// `execute_stage` must hold for ANY block geometry the backends can
        /// produce. Both backends chunk the device period against
        /// `MAX_BLOCK_SIZE` and hand the engine whatever chunk that yields, so
        /// `num_samples` and `offset` are data, not constants — and the only
        /// guarantee is `offset + num_samples <= MAX_BLOCK_SIZE`.
        ///
        /// This is the invariant whose violation panicked the audio thread on
        /// every device period above 256 frames (see `buffer_pool.rs`). A panic
        /// here is a panic there.
        #[test]
        fn execute_stage_holds_for_any_block_geometry(
            n_nodes in 1usize..12,
            num_samples in 1usize..=ipc_layer::MAX_BLOCK_SIZE,
            offset_frac in 0.0f64..1.0,
        ) {
            let slack = ipc_layer::MAX_BLOCK_SIZE - num_samples;
            let offset = (offset_frac * slack as f64) as usize;
            prop_assert!(offset + num_samples <= ipc_layer::MAX_BLOCK_SIZE);

            let topo = populated(n_nodes);
            let nodes = nodes_with(n_nodes);
            let mut buffers = boxed::<{ crate::MAX_BUFFERS }>();
            let mut xfade = boxed::<{ crate::MAX_CROSSFADE_BUFFERS }>();
            let block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
            let telemetry: [AtomicU64; crate::MAX_NODES] =
                std::array::from_fn(|_| AtomicU64::new(0));
            let faulted: [AtomicBool; crate::MAX_NODES] =
                std::array::from_fn(|_| AtomicBool::new(false));
            let mut pdc = PdcLines::new();

            // Seed the input edges so the nodes have something to move.
            for b in buffers.iter_mut() {
                let n = num_samples.min(b.data.len());
                b.data[..n].fill(0.25);
            }

            for s_idx in 0..topo.plan.num_stages {
                GraphExecutor::execute_stage(
                    &nodes, &mut buffers, &mut xfade, &topo, s_idx,
                    num_samples, offset, &block_x_map,
                    &mut None, None, None, true,
                    &telemetry, &mut pdc, 0, &faulted,
                );
            }

            // No node may have been permanently bypassed — that only happens
            // when `catch_unwind` caught a panic inside `process()`.
            for (i, f) in faulted.iter().enumerate().take(n_nodes) {
                prop_assert!(
                    !f.load(std::sync::atomic::Ordering::Relaxed),
                    "node {i} panicked at num_samples={num_samples} offset={offset}"
                );
            }
            // And the output is finite — a wrong slice can alias rather than panic.
            for b in buffers.iter() {
                prop_assert!(b.data.iter().all(|v| v.is_finite()));
            }
        }

        /// The same, with plugin-delay compensation engaged on every input, since
        /// PDC adds its own ring arithmetic keyed on `num_samples`.
        #[test]
        fn execute_stage_holds_with_pdc_engaged(
            n_nodes in 2usize..8,
            num_samples in 1usize..=512,
            delay in 1u32..2000,
        ) {
            let mut topo = populated(n_nodes);
            for i in 0..n_nodes {
                topo.plan.input_delays[i].0[0] = delay as f32;
                topo.plan.input_delays[i].0[1] = delay as f32 + 0.5; // fractional too
            }
            let nodes = nodes_with(n_nodes);
            let mut buffers = boxed::<{ crate::MAX_BUFFERS }>();
            let mut xfade = boxed::<{ crate::MAX_CROSSFADE_BUFFERS }>();
            let block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
            let telemetry: [AtomicU64; crate::MAX_NODES] =
                std::array::from_fn(|_| AtomicU64::new(0));
            let faulted: [AtomicBool; crate::MAX_NODES] =
                std::array::from_fn(|_| AtomicBool::new(false));
            let mut pdc = PdcLines::new();

            for write_pos in [0usize, 7, 1023, 4095] {
                for s_idx in 0..topo.plan.num_stages {
                    GraphExecutor::execute_stage(
                        &nodes, &mut buffers, &mut xfade, &topo, s_idx,
                        num_samples, 0, &block_x_map,
                        &mut None, None, None, true,
                        &telemetry, &mut pdc, write_pos, &faulted,
                    );
                }
            }
            for (i, f) in faulted.iter().enumerate().take(n_nodes) {
                prop_assert!(
                    !f.load(std::sync::atomic::Ordering::Relaxed),
                    "node {i} panicked with PDC delay {delay} at num_samples={num_samples}"
                );
            }
        }
    }
}
