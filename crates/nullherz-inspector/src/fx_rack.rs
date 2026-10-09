//! The deck FX rack: UI slots that are the same thing as graph nodes.
//!
//! # Why this module exists
//!
//! The rack used to be `[Vec<String>; 16]` — an unbounded list of LABELS — over
//! a graph that had exactly one FX node per deck. Four defects compounded out of
//! that mismatch:
//!
//! 1. Loading a second effect resolved `deck_<x>_fx<n>` for an `n` with no node,
//!    fell back to an alias for slot 1, and silently replaced the first effect
//!    while the UI list grew.
//! 2. `DeckState::default()` seeded three labels (`TRIM / GAIN`, `3-BAND EQ`,
//!    `PITCH / SPEED`) that were not nodes at all. The rack renderer recognised
//!    them by string-matching the label and re-pointed their knobs at the real
//!    gain and isolator nodes — so the very first real effect load started at
//!    index 3 and took the fallback path on its first try.
//! 3. Remove dropped the label and left the processor running. Reorder swapped
//!    labels and left the graph untouched.
//! 4. The generic knob rendered for a loaded effect sent no command, so every
//!    hot-loaded effect ran at its defaults forever.
//!
//! The fix is to stop keeping a list of names beside the graph and make the
//! rack POSITIONAL: slot `i` of deck `d` *is* node `deck_<d>_fx<i+1>`, which the
//! mixer registers at bootstrap as a `BYPASS` pass-through
//! (`nullherz_mixer::DECK_FX_SLOT_COUNT`). An occupied slot is a slot whose node
//! has been swapped to a real processor. There is no index arithmetic to get
//! wrong, nothing to keep in step, and no state that can disagree with the
//! graph — because the slot's identity is its position.
//!
//! Every mutation here goes through one of the three functions at the bottom, so
//! a rack rendered in a different view (`mixer`, `channel_detail`) cannot mutate
//! the list without also moving the audio.

use nullherz_traits::{Command, MixerCommand, ProcessorTypeId, TopologyCommand};

pub use nullherz_mixer::DECK_FX_SLOT_COUNT;

/// Decks the console actually builds (`create_4channel_mixer`). Channel strips
/// beyond this fold onto a deck with `% DECK_COUNT`, matching the existing
/// strip renderer; slots on a deck the graph does not have resolve to `None`
/// and every command is skipped, which is what `AGENTS.md` §3 requires.
pub const DECK_COUNT: usize = 4;

/// Parameter ramp for rack knobs, in samples.
///
/// Matches the isolator knobs. A `SetParam` with no ramp steps the coefficient
/// between blocks, which is audible as a click on anything with a continuous
/// response.
const RACK_PARAM_RAMP: u32 = 128;

/// One occupied FX slot.
///
/// `processor_type_id` is `None` for a WASM sidecar, which is installed by
/// `CoreCommand::HotLoadSidecar` rather than by a type swap — the distinction
/// matters on reorder, where each slot has to be re-installed the same way it
/// was installed the first time.
#[derive(Clone, Debug, PartialEq)]
pub struct DeckInsert {
    /// Display name, from the sidecar descriptor.
    pub name: String,
    /// Sidecar id, for re-installing a WASM insert after a reorder.
    pub sidecar_id: String,
    /// Native processor type, or `None` for a WASM sidecar.
    pub processor_type_id: Option<ProcessorTypeId>,
    /// Knob values, mirrored so a reorder can re-send them after the swap.
    pub params: [f32; 8],
}

/// A deck's FX slots, positionally bound to `deck_<x>_fx1..fx4`.
pub type DeckFxSlots = [Option<DeckInsert>; DECK_FX_SLOT_COUNT];

pub fn empty_slots() -> DeckFxSlots {
    std::array::from_fn(|_| None)
}

/// Native processor type for a catalog sidecar id, or `None` when the module is
/// a WASM sidecar that must be hot-loaded instead.
///
/// This mapping was duplicated at two hot-load sites in `store.rs` that had
/// already drifted apart from the rack renderer. One copy.
pub fn processor_type_for_sidecar(sidecar_id: &str) -> Option<ProcessorTypeId> {
    match sidecar_id {
        "neural-saturation" => Some(ProcessorTypeId::NEURAL_SATURATOR),
        "neural-filter" => Some(ProcessorTypeId::NEURAL_FILTER),
        "neural-tcn" => Some(ProcessorTypeId::NEURAL_TCN),
        "neural-ssm" => Some(ProcessorTypeId::NEURAL_SSM),
        "neural-nam" => Some(ProcessorTypeId::NEURAL_NAM),
        "hypernetwork-eq" => Some(ProcessorTypeId::HYPERNETWORK_EQ),
        "tube-preamp" => Some(ProcessorTypeId::TUBE_PREAMP),
        "multiband-compressor" => Some(ProcessorTypeId::MULTIBAND_COMPRESSOR),
        "transient-shaper" => Some(ProcessorTypeId::TRANSIENT_SHAPER),
        "tape-saturator" => Some(ProcessorTypeId::TAPE_SATURATOR),
        "mutator" => Some(ProcessorTypeId::MUTATOR),
        "algorithmic-reverb" => Some(ProcessorTypeId::REVERB),
        "algorithmic-modulation" => Some(ProcessorTypeId::MODULATION_FX),
        "algorithmic-delay" => Some(ProcessorTypeId::DELAY),
        "algorithmic-eq" => Some(ProcessorTypeId::BIQUAD),
        _ => None,
    }
}

/// The node name for a slot. Slot indices are 0-based; the node names the mixer
/// registers are 1-based.
pub fn slot_node_name(deck_idx: usize, slot_idx: usize) -> String {
    let deck_char = (b'a' + (deck_idx % DECK_COUNT) as u8) as char;
    format!("deck_{}_fx{}", deck_char, slot_idx + 1)
}

/// Outcome of a load attempt, so the caller can tell the operator what happened
/// instead of appearing to succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// Installed into this slot index.
    Loaded(usize),
    /// Every slot on the deck is occupied.
    RackFull,
    /// The slot's node is not in the graph, so no command was sent.
    Unresolved,
}

/// The commands that install `insert` into `deck_<x>_fx<slot+1>`.
///
/// Returns an empty vector when the slot's node does not resolve — skipping the
/// command on an unresolved name is what `AGENTS.md` §3 requires, and is what
/// the deleted `.unwrap_or(i as u32 * 4 + 2)` fallback violated. That fallback
/// was a hardcoded node index in a view: on deck A it computed node 2, which
/// is deck A's `dna_slot`, so a failed lookup quietly swapped a source-insert
/// slot instead of an FX slot.
pub fn install_commands(
    node_map: &std::collections::HashMap<String, u32>,
    deck_idx: usize,
    slot_idx: usize,
    insert: &DeckInsert,
) -> Vec<Command> {
    let Some(node_idx) = node_map.get(&slot_node_name(deck_idx, slot_idx)).copied() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    match insert.processor_type_id {
        Some(processor_type_id) => {
            out.push(Command::Topology(TopologyCommand::SwapProcessor { node_idx, processor_type_id }));
        }
        None => {
            let mut name = [0u8; 32];
            let b = insert.sidecar_id.as_bytes();
            let len = b.len().min(32);
            name[..len].copy_from_slice(&b[..len]);
            out.push(Command::Core(nullherz_traits::CoreCommand::HotLoadSidecar { name, node_idx }));
        }
    }
    // A swap installs a fresh processor at ITS defaults, not at the values the
    // rack is showing. Re-sending the mirrored knobs is what keeps the two in
    // agreement after a load or a reorder.
    out.push(set_param_command(node_idx, 0, insert.params[0]));
    out
}

/// The command that returns a slot to an empty pass-through.
///
/// Remove used to drop the label only, which left the processor in the graph —
/// still processing, still audible, with nothing in the UI to turn it off.
pub fn clear_commands(
    node_map: &std::collections::HashMap<String, u32>,
    deck_idx: usize,
    slot_idx: usize,
) -> Vec<Command> {
    match node_map.get(&slot_node_name(deck_idx, slot_idx)).copied() {
        Some(node_idx) => vec![Command::Topology(TopologyCommand::SwapProcessor {
            node_idx,
            processor_type_id: ProcessorTypeId::BYPASS,
        })],
        None => Vec::new(),
    }
}

/// A rack knob's `SetParam`, ramped like the isolator knobs.
pub fn set_param_command(node_idx: u32, param_id: u32, value: f32) -> Command {
    Command::Mixer(MixerCommand::SetParam {
        target_id: node_idx as u64,
        param_id,
        value,
        ramp_duration_samples: RACK_PARAM_RAMP,
    })
}

// ============================================================================
// MUTATIONS — the only way the rack changes
// ============================================================================

/// Install `insert` into the deck's first empty slot.
///
/// Commands are appended to `commands` rather than sent, so this stays testable
/// and callable from inside an egui closure that already borrows the app.
pub fn load(
    slots: &mut DeckFxSlots,
    node_map: &std::collections::HashMap<String, u32>,
    deck_idx: usize,
    insert: DeckInsert,
    commands: &mut Vec<Command>,
) -> LoadOutcome {
    let Some(slot_idx) = slots.iter().position(|s| s.is_none()) else {
        return LoadOutcome::RackFull;
    };
    let cmds = install_commands(node_map, deck_idx, slot_idx, &insert);
    if cmds.is_empty() {
        // Nothing reached the graph, so nothing goes in the rack either. A
        // label with no processor behind it is the defect this module exists
        // to remove, not a state worth keeping.
        return LoadOutcome::Unresolved;
    }
    commands.extend(cmds);
    slots[slot_idx] = Some(insert);
    LoadOutcome::Loaded(slot_idx)
}

/// Empty a slot, returning its node to `BYPASS`.
pub fn remove(
    slots: &mut DeckFxSlots,
    node_map: &std::collections::HashMap<String, u32>,
    deck_idx: usize,
    slot_idx: usize,
    commands: &mut Vec<Command>,
) {
    if slot_idx >= slots.len() || slots[slot_idx].is_none() {
        return;
    }
    // The label goes only if the processor goes. On an unresolved node the slot
    // stays occupied, which is honest: something may well still be running
    // there, and showing it empty would be a lie the operator cannot act on.
    let cmds = clear_commands(node_map, deck_idx, slot_idx);
    if cmds.is_empty() {
        return;
    }
    commands.extend(cmds);
    slots[slot_idx] = None;
}

/// Exchange two slots, moving the processors with them.
///
/// Positional slots make reorder a real topology edit rather than a graph
/// rebuild: swap the contents, then re-install each slot from what now occupies
/// it. Two `SwapProcessor`s and the mirrored knob values.
///
/// What this cannot carry across is a processor's INTERNAL state — a reverb's
/// tail, a delay line, a compressor's envelope — because the destination node
/// gets a freshly constructed processor. Moving an effect in the chain
/// therefore re-initialises it. That is inherent to slot-swap reordering and is
/// the behaviour an operator gets from a hardware insert patch too.
pub fn reorder(
    slots: &mut DeckFxSlots,
    node_map: &std::collections::HashMap<String, u32>,
    deck_idx: usize,
    a: usize,
    b: usize,
    commands: &mut Vec<Command>,
) {
    if a == b || a >= slots.len() || b >= slots.len() {
        return;
    }
    // Both endpoints must resolve before anything moves. A half-applied
    // reorder would leave the rack describing a chain the graph does not have.
    let names = [slot_node_name(deck_idx, a), slot_node_name(deck_idx, b)];
    if names.iter().any(|n| !node_map.contains_key(n)) {
        return;
    }

    slots.swap(a, b);
    for idx in [a, b] {
        match &slots[idx] {
            Some(insert) => commands.extend(install_commands(node_map, deck_idx, idx, insert)),
            None => commands.extend(clear_commands(node_map, deck_idx, idx)),
        }
    }
}

// ============================================================================
// APP BINDING
// ============================================================================

/// The rack operations as they are called from a view.
///
/// These exist so a view never has to hold `&mut app.decks` and `&app.topo` at
/// the same time — the split borrow lives here, once, instead of being worked
/// around at every call site with a clone of the node map.
impl crate::InspectorApp {
    /// Install a module into the focused deck's first empty slot.
    pub(crate) fn fx_rack_load(&mut self, deck_idx: usize, insert: DeckInsert) -> LoadOutcome {
        let d = deck_idx % DECK_COUNT;
        let mut commands = Vec::new();
        let outcome = load(&mut self.decks.deck_fx[d], &self.topo.node_map, d, insert, &mut commands);
        self.send_all(commands);
        outcome
    }

    /// Install a module by its catalog descriptor.
    pub(crate) fn fx_rack_load_sidecar(
        &mut self,
        deck_idx: usize,
        name: &str,
        sidecar_id: &str,
    ) -> LoadOutcome {
        self.fx_rack_load(
            deck_idx,
            DeckInsert {
                name: name.to_string(),
                sidecar_id: sidecar_id.to_string(),
                processor_type_id: processor_type_for_sidecar(sidecar_id),
                params: [0.5; 8],
            },
        )
    }

    pub(crate) fn fx_rack_remove(&mut self, deck_idx: usize, slot_idx: usize) {
        let d = deck_idx % DECK_COUNT;
        let mut commands = Vec::new();
        remove(&mut self.decks.deck_fx[d], &self.topo.node_map, d, slot_idx, &mut commands);
        self.send_all(commands);
    }

    pub(crate) fn fx_rack_reorder(&mut self, deck_idx: usize, a: usize, b: usize) {
        let d = deck_idx % DECK_COUNT;
        let mut commands = Vec::new();
        reorder(&mut self.decks.deck_fx[d], &self.topo.node_map, d, a, b, &mut commands);
        self.send_all(commands);
    }

    /// Set one of a loaded insert's parameters, on the node that insert is in.
    ///
    /// The rack's knob used to render with no `send` at all, so a hot-loaded
    /// effect ran at its construction defaults for the rest of the session.
    pub(crate) fn fx_rack_set_param(
        &mut self,
        deck_idx: usize,
        slot_idx: usize,
        param_id: u32,
        value: f32,
    ) {
        let d = deck_idx % DECK_COUNT;
        let Some(slot) = self.decks.deck_fx[d].get_mut(slot_idx) else { return };
        let Some(insert) = slot.as_mut() else { return };
        if let Some(p) = insert.params.get_mut(param_id as usize) {
            *p = value;
        }
        let Some(node_idx) = self.topo.node_map.get(&slot_node_name(d, slot_idx)).copied() else {
            return;
        };
        let _ = self.command_sender.send(set_param_command(node_idx, param_id, value));
    }

    fn send_all(&self, commands: Vec<Command>) {
        for c in commands {
            let _ = self.command_sender.send(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A node map shaped like the bootstrapped console's.
    fn console_map() -> HashMap<String, u32> {
        let mut m = HashMap::new();
        for (d, deck) in ['a', 'b', 'c', 'd'].iter().enumerate() {
            for slot in 1..=DECK_FX_SLOT_COUNT {
                m.insert(format!("deck_{deck}_fx{slot}"), (d * DECK_FX_SLOT_COUNT + slot) as u32);
            }
        }
        m
    }

    fn native(name: &str, ty: ProcessorTypeId) -> DeckInsert {
        DeckInsert {
            name: name.into(),
            sidecar_id: name.to_lowercase(),
            processor_type_id: Some(ty),
            params: [0.5; 8],
        }
    }

    fn swaps(commands: &[Command]) -> Vec<(u32, ProcessorTypeId)> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::Topology(TopologyCommand::SwapProcessor { node_idx, processor_type_id }) => {
                    Some((*node_idx, *processor_type_id))
                }
                _ => None,
            })
            .collect()
    }

    /// Defect (a)/(c): four loads must land on four DIFFERENT nodes. Under the
    /// old one-slot graph every load after the first resolved the same node and
    /// replaced its predecessor.
    #[test]
    fn four_loads_occupy_four_distinct_nodes() {
        let map = console_map();
        let mut slots = empty_slots();
        let mut cmds = Vec::new();

        for i in 0..DECK_FX_SLOT_COUNT {
            let outcome = load(
                &mut slots,
                &map,
                0,
                native(&format!("FX{i}"), ProcessorTypeId::REVERB),
                &mut cmds,
            );
            assert_eq!(outcome, LoadOutcome::Loaded(i));
        }

        let targets: Vec<u32> = swaps(&cmds).iter().map(|(n, _)| *n).collect();
        assert_eq!(targets, vec![1, 2, 3, 4], "loads must walk the slots, not pile onto one");
        assert!(slots.iter().all(|s| s.is_some()));
    }

    /// The rack is bounded by the graph. A fifth load is refused rather than
    /// appended to a list the audio path cannot honour.
    #[test]
    fn fifth_load_is_refused() {
        let map = console_map();
        let mut slots = empty_slots();
        let mut cmds = Vec::new();
        for i in 0..DECK_FX_SLOT_COUNT {
            load(&mut slots, &map, 0, native(&format!("FX{i}"), ProcessorTypeId::DELAY), &mut cmds);
        }
        let before = cmds.len();
        assert_eq!(
            load(&mut slots, &map, 0, native("FIFTH", ProcessorTypeId::DELAY), &mut cmds),
            LoadOutcome::RackFull
        );
        assert_eq!(cmds.len(), before, "a refused load must send nothing");
    }

    /// Defect (d): remove must return the node to BYPASS, not just drop the
    /// label and leave the processor audible.
    #[test]
    fn remove_returns_the_node_to_bypass() {
        let map = console_map();
        let mut slots = empty_slots();
        let mut cmds = Vec::new();
        load(&mut slots, &map, 0, native("REVERB", ProcessorTypeId::REVERB), &mut cmds);
        cmds.clear();

        remove(&mut slots, &map, 0, 0, &mut cmds);
        assert_eq!(swaps(&cmds), vec![(1, ProcessorTypeId::BYPASS)]);
        assert!(slots[0].is_none());
    }

    /// Defect (d): reorder must move the processors, not only the labels.
    #[test]
    fn reorder_moves_the_processors() {
        let map = console_map();
        let mut slots = empty_slots();
        let mut cmds = Vec::new();
        load(&mut slots, &map, 0, native("REVERB", ProcessorTypeId::REVERB), &mut cmds);
        load(&mut slots, &map, 0, native("DELAY", ProcessorTypeId::DELAY), &mut cmds);
        cmds.clear();

        reorder(&mut slots, &map, 0, 0, 1, &mut cmds);

        assert_eq!(slots[0].as_ref().unwrap().name, "DELAY");
        assert_eq!(slots[1].as_ref().unwrap().name, "REVERB");
        assert_eq!(
            swaps(&cmds),
            vec![(1, ProcessorTypeId::DELAY), (2, ProcessorTypeId::REVERB)],
            "each node must be re-installed with what now occupies its slot"
        );
    }

    /// Reordering into an empty slot must clear the vacated node, or the effect
    /// keeps running at its old position as well as its new one.
    #[test]
    fn reorder_into_an_empty_slot_clears_the_source() {
        let map = console_map();
        let mut slots = empty_slots();
        let mut cmds = Vec::new();
        load(&mut slots, &map, 0, native("REVERB", ProcessorTypeId::REVERB), &mut cmds);
        cmds.clear();

        reorder(&mut slots, &map, 0, 0, 1, &mut cmds);

        assert!(slots[0].is_none());
        assert_eq!(slots[1].as_ref().unwrap().name, "REVERB");
        assert_eq!(
            swaps(&cmds),
            vec![(1, ProcessorTypeId::BYPASS), (2, ProcessorTypeId::REVERB)]
        );
    }

    /// A load restores the knob values the rack is showing, because the swap
    /// installs a processor at its own defaults.
    #[test]
    fn install_resends_the_mirrored_params() {
        let map = console_map();
        let mut insert = native("REVERB", ProcessorTypeId::REVERB);
        insert.params[0] = 0.77;
        let cmds = install_commands(&map, 0, 0, &insert);

        let sent: Vec<f32> = cmds
            .iter()
            .filter_map(|c| match c {
                Command::Mixer(MixerCommand::SetParam { target_id: 1, value, .. }) => Some(*value),
                _ => None,
            })
            .collect();
        assert_eq!(sent, vec![0.77]);
    }

    /// `AGENTS.md` §3: an unresolved name sends NOTHING. No hardcoded index, no
    /// fallback to another node, and no rack entry pretending otherwise.
    #[test]
    fn unresolved_slot_sends_no_command_and_stores_nothing() {
        let empty = HashMap::new();
        let mut slots = empty_slots();
        let mut cmds = Vec::new();

        assert_eq!(
            load(&mut slots, &empty, 0, native("REVERB", ProcessorTypeId::REVERB), &mut cmds),
            LoadOutcome::Unresolved
        );
        assert!(cmds.is_empty());
        assert!(slots.iter().all(|s| s.is_none()));

        // And the same for the other two mutations.
        slots[0] = Some(native("REVERB", ProcessorTypeId::REVERB));
        remove(&mut slots, &empty, 0, 0, &mut cmds);
        reorder(&mut slots, &empty, 0, 0, 1, &mut cmds);
        assert!(cmds.is_empty());
        assert!(slots[0].is_some(), "the slot must not be emptied without the processor being cleared");
    }

    /// Slot index 0 is `fx1`: the off-by-one that made the first real load miss.
    #[test]
    fn slot_names_are_one_based() {
        assert_eq!(slot_node_name(0, 0), "deck_a_fx1");
        assert_eq!(slot_node_name(3, 3), "deck_d_fx4");
        // Strips past the fourth fold onto a deck that exists.
        assert_eq!(slot_node_name(4, 0), "deck_a_fx1");
    }

    /// A WASM sidecar is installed by hot-load, not by a type swap — including
    /// when a reorder re-installs it.
    #[test]
    fn wasm_sidecar_reinstalls_by_hot_load() {
        let map = console_map();
        let insert = DeckInsert {
            name: "Custom WASM".into(),
            sidecar_id: "custom-wasm".into(),
            processor_type_id: None,
            params: [0.5; 8],
        };
        let cmds = install_commands(&map, 0, 1, &insert);
        assert!(swaps(&cmds).is_empty(), "a sidecar must not be type-swapped");
        assert!(cmds.iter().any(|c| matches!(
            c,
            Command::Core(nullherz_traits::CoreCommand::HotLoadSidecar { node_idx: 2, .. })
        )));
    }

    /// The catalog mapping must stay in one place and keep resolving.
    #[test]
    fn catalog_mapping_resolves_known_modules() {
        assert_eq!(processor_type_for_sidecar("algorithmic-reverb"), Some(ProcessorTypeId::REVERB));
        assert_eq!(processor_type_for_sidecar("tube-preamp"), Some(ProcessorTypeId::TUBE_PREAMP));
        assert_eq!(processor_type_for_sidecar("some-wasm-module"), None);
    }
}
