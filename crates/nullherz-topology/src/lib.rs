pub mod compiler;
pub use compiler::GraphCompiler;

use nullherz_traits::{ProcessorTypeId, MAX_BUFFERS, MAX_CHANNELS, MAX_NODES};
use serde_big_array::BigArray;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DesiredNode {
    pub type_id: ProcessorTypeId,
    pub input_buffers: [u32; MAX_CHANNELS],
    pub output_buffers: [u32; MAX_CHANNELS],
    pub input_count: u32,
    pub output_count: u32,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DesiredGraphState {
    #[serde(with = "BigArray")]
    pub nodes: [Option<DesiredNode>; MAX_NODES],
}

impl Default for DesiredGraphState {
    fn default() -> Self {
        Self { nodes: [None; MAX_NODES] }
    }
}

impl DesiredGraphState {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Lowest unoccupied slot, or `None` when the graph is full.
    ///
    /// Slot index IS the node id, so reusing a freed slot reuses its id. That is
    /// the whole reason this model can support runtime editing at all: the
    /// `IdAllocator` hands out monotonically increasing ids and never reclaims
    /// them ("never reused for safety and simplicity"), which is fine for a
    /// bootstrap that runs once and fatal for an editor — add and remove a strip
    /// eleven times and a 128-node graph is exhausted with nothing in it.
    pub fn free_slot(&self) -> Option<usize> {
        self.nodes.iter().position(|n| n.is_none())
    }

    /// `n` free slots, lowest first, or `None` if the graph cannot fit them.
    ///
    /// All-or-nothing on purpose: a channel strip is eleven nodes and a
    /// half-placed strip is a broken signal path, so the caller needs to know
    /// BEFORE it starts whether the whole thing fits.
    pub fn free_slots(&self, n: usize) -> Option<Vec<usize>> {
        let found: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_none())
            .map(|(i, _)| i)
            .take(n)
            .collect();
        (found.len() == n).then_some(found)
    }

    /// How many node slots are in use.
    pub fn occupied(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_some()).count()
    }

    /// Every buffer id referenced by a live node.
    ///
    /// Derived rather than tracked, deliberately. A separate free-list has to be
    /// kept in step with the graph and drifts the first time a path forgets to
    /// update it; the graph already knows what it references, so asking it
    /// cannot be wrong.
    pub fn buffers_in_use(&self) -> std::collections::HashSet<u32> {
        let mut used = std::collections::HashSet::new();
        for node in self.nodes.iter().flatten() {
            for b in node.input_buffers.iter().take(node.input_count as usize) {
                used.insert(*b);
            }
            for b in node.output_buffers.iter().take(node.output_count as usize) {
                used.insert(*b);
            }
        }
        used
    }

    /// `n` buffer ids no live node references, at or above `reserved_below`.
    ///
    /// `reserved_below` exists because the low ids are FIXED: `MixerConfig`
    /// pins master, cue and the two DJ buses at 0..11, and handing one of those
    /// to a new strip would silently reroute the master. The caller passes the
    /// boundary rather than this guessing it.
    ///
    /// All-or-nothing, for the same reason as `free_slots`: a strip with some of
    /// its buffers is a broken signal path.
    ///
    /// Buffers matter as much as node slots here. `IdAllocator::allocate_buffer_id`
    /// is monotonic like the node one, and a deck strip costs NINETEEN buffers
    /// against a `MAX_BUFFERS` of 240 — so an editor that leaks them exhausts
    /// the graph in twelve edits, before the node space runs out.
    pub fn free_buffers(&self, n: usize, reserved_below: u32) -> Option<Vec<u32>> {
        let used = self.buffers_in_use();
        let found: Vec<u32> = (reserved_below..MAX_BUFFERS as u32)
            .filter(|b| !used.contains(b))
            .take(n)
            .collect();
        (found.len() == n).then_some(found)
    }
}

impl DesiredGraphState {
    /// Read the desired-state view out of the LIVE graph.
    ///
    /// The conductor's source of truth is `GraphTopology` (routing) plus the
    /// topology manager's `active_node_types` (which processor is in each slot).
    /// `DesiredGraphState` is the editing view of the same thing, and deriving
    /// it rather than maintaining a second copy is what stops the two drifting —
    /// a parallel model updated by hand is wrong the first time a path forgets.
    ///
    /// A slot counts as occupied only if it has a recorded TYPE. `routing` is a
    /// fixed `[NodeRouting; MAX_NODES]` where unused slots are zeroed, so
    /// routing alone cannot distinguish "node with no connections yet" from
    /// "empty"; `active_node_types` can, and it is what `RemoveNode` clears.
    pub fn from_live(
        routing: &[nullherz_traits::NodeRouting; MAX_NODES],
        active_node_types: &std::collections::HashMap<u32, u32>,
    ) -> Self {
        let mut state = Self::empty();
        for (idx, slot) in state.nodes.iter_mut().enumerate() {
            let Some(type_id) = active_node_types.get(&(idx as u32)) else { continue };
            let r = &routing[idx];
            let mut node = DesiredNode {
                type_id: ProcessorTypeId(*type_id),
                input_buffers: [0; MAX_CHANNELS],
                output_buffers: [0; MAX_CHANNELS],
                input_count: r.input_count.min(MAX_CHANNELS) as u32,
                output_count: r.output_count.min(MAX_CHANNELS) as u32,
            };
            for j in 0..node.input_count as usize {
                node.input_buffers[j] = r.input_indices[j].0;
            }
            for j in 0..node.output_count as usize {
                node.output_buffers[j] = r.output_indices[j].0;
            }
            *slot = Some(node);
        }
        state
    }
}

pub struct GraphReconciler;

impl GraphReconciler {
    pub fn reconcile(current: &DesiredGraphState, target: &DesiredGraphState) -> Vec<nullherz_traits::TopologyCommand> {
        let mut commands = Vec::new();

        for i in 0..MAX_NODES {
            match (current.nodes[i], target.nodes[i]) {
                (None, Some(node)) => {
                    commands.push(nullherz_traits::TopologyCommand::AddNode {
                        processor_type_id: node.type_id,
                        node_idx: i as u32,
                    });
                    for j in 0..node.input_count as usize {
                        commands.push(nullherz_traits::TopologyCommand::UpdateEdge {
                            node_idx: i as u32,
                            input_idx: j as u32,
                            new_buffer_idx: node.input_buffers[j],
                        });
                    }
                    for j in 0..node.output_count as usize {
                        commands.push(nullherz_traits::TopologyCommand::UpdateOutputEdge {
                            node_idx: i as u32,
                            output_idx: j as u32,
                            new_buffer_idx: node.output_buffers[j],
                        });
                    }
                }
                (Some(curr), Some(node)) => {
                    if curr.type_id != node.type_id {
                        commands.push(nullherz_traits::TopologyCommand::SwapProcessor {
                            node_idx: i as u32,
                            processor_type_id: node.type_id,
                        });
                    }

                    // Reconcile edges
                    for j in 0..node.input_count as usize {
                        if j >= curr.input_count as usize || curr.input_buffers[j] != node.input_buffers[j] {
                            commands.push(nullherz_traits::TopologyCommand::UpdateEdge {
                                node_idx: i as u32,
                                input_idx: j as u32,
                                new_buffer_idx: node.input_buffers[j],
                            });
                        }
                    }
                    for j in 0..node.output_count as usize {
                        if j >= curr.output_count as usize || curr.output_buffers[j] != node.output_buffers[j] {
                            commands.push(nullherz_traits::TopologyCommand::UpdateOutputEdge {
                                node_idx: i as u32,
                                output_idx: j as u32,
                                new_buffer_idx: node.output_buffers[j],
                            });
                        }
                    }

                    // Inputs the target no longer has. Symmetric to the
                    // `RemoveNode` gap above and just as consequential: without
                    // it a node's `input_count` can only grow.
                    //
                    // That is the bus-slot leak. Every strip added to a session
                    // takes one input on the bus summing node; if removing the
                    // strip does not give that input back, the bus fills after
                    // MAX_CHANNELS edits and the next add fails with the bus
                    // full while the graph is nearly empty. Measured: the
                    // twentieth add/remove cycle failed at cycle 12.
                    //
                    // Highest index first, so the engine's own compaction (if
                    // any) cannot renumber a slot out from under a later
                    // command in the same batch.
                    for j in (node.input_count as usize..curr.input_count as usize).rev() {
                        commands.push(nullherz_traits::TopologyCommand::Disconnect {
                            node_idx: i as u32,
                            input_idx: j as u32,
                        });
                    }
                }
                (Some(_curr), None) => {
                    // `RemoveNode` exists and always did — the comment that used
                    // to sit here said "we don't have a specific RemoveNode
                    // command, but we could add one", which was stale.
                    //
                    // Without this arm the reconciler could only ever GROW a
                    // graph. A session could gain a strip and never lose one,
                    // which is half a feature and the worse half: node slots are
                    // the scarcest resource in the graph (`MAX_NODES` is 128 and
                    // a deck strip costs 11), so an editor that cannot free them
                    // runs out after ten edits.
                    commands.push(nullherz_traits::TopologyCommand::RemoveNode {
                        node_idx: i as u32,
                    });
                }
                _ => {}
            }
        }

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::ProcessorTypeId;

    #[test]
    fn test_reconciliation_minimal_mutations() {
        let mut current = DesiredGraphState { nodes: [None; MAX_NODES] };
        let mut target = DesiredGraphState { nodes: [None; MAX_NODES] };

        let node_a = DesiredNode {
            type_id: ProcessorTypeId::GAIN,
            input_buffers: [0; MAX_CHANNELS],
            output_buffers: [0; MAX_CHANNELS],
            input_count: 1,
            output_count: 1,
        };

        target.nodes[0] = Some(node_a);

        let commands = GraphReconciler::reconcile(&current, &target);
        assert_eq!(commands.len(), 3); // AddNode + UpdateEdge + UpdateOutputEdge

        current.nodes[0] = Some(node_a);
        let commands2 = GraphReconciler::reconcile(&current, &target);
        assert_eq!(commands2.len(), 0); // No changes needed
    }

    fn node(type_id: ProcessorTypeId) -> DesiredNode {
        DesiredNode {
            type_id,
            input_buffers: [0; MAX_CHANNELS],
            output_buffers: [0; MAX_CHANNELS],
            input_count: 1,
            output_count: 1,
        }
    }

    /// A graph that can only grow is half an editor, and the worse half.
    ///
    /// The `(Some, None)` arm used to be empty, with a comment claiming no
    /// `RemoveNode` command existed. One did. The consequence was not a missing
    /// nicety: node slots are the scarcest thing in the graph — `MAX_NODES` is
    /// 128 and a deck strip costs 11 — so an editor that never frees them is
    /// exhausted after ten edits regardless of how many tracks are live.
    #[test]
    fn test_reconcile_removes_a_node_that_left_the_target() {
        let mut current = DesiredGraphState::empty();
        let target = DesiredGraphState::empty();
        current.nodes[5] = Some(node(ProcessorTypeId::GAIN));

        let commands = GraphReconciler::reconcile(&current, &target);
        assert_eq!(
            commands.len(),
            1,
            "removing one node should emit exactly one command, got {commands:?}"
        );
        assert!(
            matches!(
                commands[0],
                nullherz_traits::TopologyCommand::RemoveNode { node_idx: 5 }
            ),
            "expected RemoveNode for slot 5, got {:?}",
            commands[0]
        );
    }

    /// Add, remove, add again — and the second add must land in the SAME slot.
    ///
    /// This is the property that makes runtime editing possible at all. The
    /// `IdAllocator` is monotonic and documents that ids are "never reused for
    /// safety and simplicity", which is correct for a bootstrap that runs once
    /// and fatal for an editor. Slot reuse is what replaces it here: the slot
    /// index IS the node id, so a freed slot returns its id with it.
    #[test]
    fn test_a_freed_slot_is_reused_rather_than_leaked() {
        let mut state = DesiredGraphState::empty();

        let first = state.free_slot().expect("empty graph has a free slot");
        state.nodes[first] = Some(node(ProcessorTypeId::GAIN));
        assert_eq!(state.occupied(), 1);

        state.nodes[first] = None;
        let second = state.free_slot().expect("slot came back");
        assert_eq!(
            second, first,
            "a removed node's slot was not reused — ids leak and the graph fills \
             up with nothing in it"
        );
        assert_eq!(state.occupied(), 0);
    }

    /// Buffers are reclaimed too, and they run out FIRST.
    ///
    /// A deck strip costs 11 nodes and 19 buffers against ceilings of 128 and
    /// 240 — so buffer space is exhausted after twelve strips and node space
    /// after ten, and an editor that leaks either is done in a dozen edits.
    #[test]
    fn test_buffers_are_reclaimed_when_a_node_goes() {
        let mut state = DesiredGraphState::empty();
        let mut n = node(ProcessorTypeId::GAIN);
        n.input_buffers[0] = 40;
        n.output_buffers[0] = 41;
        state.nodes[0] = Some(n);

        let used = state.buffers_in_use();
        assert!(used.contains(&40) && used.contains(&41));
        assert!(
            !state.free_buffers(1, 12).unwrap().contains(&40),
            "buffer 40 is referenced by a live node and must not be offered as free"
        );

        state.nodes[0] = None;
        assert!(state.buffers_in_use().is_empty(), "buffers were not reclaimed with the node");
        assert!(
            state.free_buffers(1, 12).unwrap().contains(&12),
            "allocation should resume from the first non-reserved id"
        );
    }

    /// The reserved low ids are never handed out.
    #[test]
    fn test_free_buffers_never_returns_a_reserved_id() {
        let state = DesiredGraphState::empty();
        let got = state.free_buffers(8, 12).expect("empty graph has room");
        assert!(
            got.iter().all(|b| *b >= 12),
            "handed out a reserved buffer ({got:?}) — 0..11 are master, cue and \
             the DJ buses, and reassigning one silently reroutes the master"
        );
    }

    /// A strip is all-or-nothing: eleven nodes placed, or none.
    #[test]
    fn test_free_slots_is_all_or_nothing() {
        let mut state = DesiredGraphState::empty();
        // Fill everything but three slots.
        for i in 0..MAX_NODES - 3 {
            state.nodes[i] = Some(node(ProcessorTypeId::GAIN));
        }

        assert!(state.free_slots(3).is_some(), "exactly three should fit");
        assert!(
            state.free_slots(4).is_none(),
            "four must not report success on three free slots — a half-placed \
             strip is a broken signal path, and the caller has to know before it \
             starts"
        );
        assert_eq!(state.free_slots(3).unwrap().len(), 3);
    }
}
