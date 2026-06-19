use std::sync::Arc;
use ipc_layer::{Consumer, MidiEvent};
use nullherz_traits::{Command, TopologyMutation, AudioProcessor};
use crate::engine::command_dispatcher::CommandDispatcher;
use crate::engine::resource_recycler::ResourceRecycler;
use crate::engine::metrics::EngineMetrics;
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
        metrics: &EngineMetrics,
        health_signal: &Arc<AtomicBool>,
    ) {
        if let Some(cons) = bundle_consumer {
            while let Some(bundle) = cons.pop() {
                for cmd in &bundle {
                    CommandDispatcher::handle_single_command(transport, graph, cmd);
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
}
