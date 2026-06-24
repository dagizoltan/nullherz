use std::sync::Arc;
use nullherz_traits::{Command, TopologyMutation, AudioProcessor, MidiConsumer, CommandBundleConsumer, TopologyMutationConsumer};
use crate::engine::command_dispatcher::CommandDispatcher;
use crate::engine::resource_recycler::ResourceRecycler;
use crate::engine::metrics::EngineMetrics;
use nullherz_dna::SampleRegistry;
use std::sync::atomic::AtomicBool;

pub struct EngineInputHandler {}

impl EngineInputHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn handle_async_inputs(
        graph: &mut dyn AudioProcessor,
        transport: &mut nullherz_traits::Transport,
        bundle_consumer: &mut Option<Box<dyn CommandBundleConsumer>>,
        topology_consumer: &mut Option<Box<dyn TopologyMutationConsumer>>,
        midi_consumer: &mut Option<Box<dyn MidiConsumer>>,
        resource_recycler: &mut ResourceRecycler,
        sample_registry: &SampleRegistry,
        metrics: &EngineMetrics,
        health_signal: &Arc<AtomicBool>,
    ) {
        if let Some(cons) = bundle_consumer {
            while let Some(bundle) = cons.pop() {
                for cmd in &bundle {
                    Self::handle_command(graph, transport, sample_registry, cmd);
                }
                resource_recycler.recycle_bundle(bundle, metrics, health_signal);
            }
        }

        if let Some(cons) = topology_consumer {
            let mut topo_processed = 0;
            while let Some(topo_mut) = cons.pop() {
                graph.apply_topology_mutation(topo_mut);
                topo_processed += 1;
                if topo_processed >= 16 { break; }
            }
        }

        if let Some(cons) = midi_consumer {
            while let Some(event) = cons.pop() {
                graph.apply_midi(event);
            }
        }
    }

    fn handle_command(
        graph: &mut dyn AudioProcessor,
        transport: &mut nullherz_traits::Transport,
        sample_registry: &SampleRegistry,
        cmd: &Command,
    ) {
        match cmd {
            Command::RegisterCapture { .. } => {
                graph.apply_command(cmd);
            }
            Command::AddSourceFromRegistry { granular_node_idx, sample_id } => {
                if let Some(sample) = sample_registry.get(*sample_id) {
                    graph.apply_topology_mutation(TopologyMutation::AddSource {
                        node_idx: *granular_node_idx,
                        buffer: sample.buffer,
                        sample_id: *sample_id,
                        metadata: Some(Arc::new(sample.metadata)),
                    });
                }
            }
            _ => CommandDispatcher::handle_single_command(transport, graph, cmd),
        }
    }
}
