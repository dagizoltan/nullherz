//! Every deck's insert slots must be reachable by name.
//!
//! DNA shaping and key sync are opt-in, which only works if the control that
//! turns them on can find where to install them. The original defect was a
//! DnaMorph node that existed but was never registered in `node_names`, so
//! nothing in the UI could address it.
//!
//! Since roadmap 1.12 the target is a SLOT rather than a permanently-present
//! processor: `deck_x_pitch_slot` and `deck_x_dna_slot` hold identity
//! pass-throughs until something is swapped in. The addressability requirement is
//! unchanged — an unnamed slot is just as unreachable as an unnamed node was.

#[test]
fn test_every_deck_exposes_named_insert_slots() {
    let mut m = nullherz_mixer::MixerManager::new();
    let _ = m.create_4channel_mixer();
    for d in ['a', 'b', 'c', 'd'] {
        for slot in ["pitch_slot", "dna_slot"] {
            let key = format!("deck_{d}_{slot}");
            assert!(
                m.node_names.contains_key(&key),
                "{key} is not registered; the control that installs a processor \
                 into deck {}'s {slot} cannot resolve a target",
                d.to_ascii_uppercase()
            );
        }
    }
}

#[test]
fn test_named_nodes_fit_the_telemetry_map() {
    // The map is the only route from the engine to the UI. Overflowing it drops
    // an arbitrary subset (HashMap iteration order), so controls would break on
    // some runs and not others.
    let mut m = nullherz_mixer::MixerManager::new();
    let _ = m.create_4channel_mixer();
    let slots = nullherz_traits::telemetry::NODE_MAP_SLOTS;
    assert!(
        m.node_names.len() <= slots,
        "{} named nodes exceed the {} telemetry slots",
        m.node_names.len(), slots
    );
}
