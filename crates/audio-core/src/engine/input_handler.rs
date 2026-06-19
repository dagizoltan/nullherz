use std::sync::Arc;
use ipc_layer::{Consumer, MidiEvent};
use nullherz_traits::{Command, TopologyMutation, AudioProcessor};
use crate::engine::command_dispatcher::CommandDispatcher;
use crate::engine::resource_recycler::ResourceRecycler;
use crate::engine::metrics::EngineMetrics;
use crate::engine::sample_registry::SampleRegistry;
use std::sync::atomic::AtomicBool;

pub struct EngineInputHandler {}

impl EngineInputHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn handle_async_inputs(
        graph: &mut dyn AudioProcessor,
        transport: &mut nullherz_traits::Transport,
        bundle_consumer: &mut Option<Consumer<Vec<Command>>>,
        topology_consumer: &mut Option<Consumer<TopologyMutation>>,
        midi_consumer: &mut Option<Consumer<MidiEvent>>,
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
            Command::RegisterCapture { capture_node_idx, sample_id } => {
                if let Some(processor_graph) = graph.as_any_mut().downcast_mut::<crate::processors::ProcessorGraph>() {
                    let idx = *capture_node_idx as usize;
                    if idx < processor_graph.node_count {
                        let node = &processor_graph.nodes[idx];
                        if let Some(snapshot) = unsafe { (*node.processor.get()).pull_snapshot() } {
                             // Registration happens here. Although it uses a RwLock,
                             // if it's called from RT it's bad.
                             // However, if we move handle_async_inputs to non-RT path
                             // of the engine (e.g. before the process loop begins in
                             // a separate thread or at the start of the block with a
                             // trylock), it would be safer.
                             // For now, we follow the theory's "finish what you started".
                            sample_registry.register(*sample_id, snapshot);
                        }
                    }
                }
            }
            Command::AddSourceFromRegistry { granular_node_idx, sample_id } => {
                if let Some(sample) = sample_registry.get(*sample_id) {
                    graph.apply_topology_mutation(TopologyMutation::AddSource {
                        node_idx: *granular_node_idx,
                        buffer: sample,
                    });
                }
            }
            _ => CommandDispatcher::handle_single_command(transport, graph, cmd),
        }
    }
}
