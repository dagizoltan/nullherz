//! Every deck's DNA morph node must be reachable by name.
//!
//! DNA shaping is opt-in now, which only works if the control that turns it on
//! can find the node. The node existed from the start but was never registered
//! in `node_names`, so nothing in the UI could address it — the feature would
//! have been unreachable in exactly the way scroll-to-scrub was.

#[test]
fn test_every_deck_exposes_a_named_dna_morph_node() {
    let mut m = nullherz_mixer::MixerManager::new();
    let _ = m.create_4channel_mixer();
    for d in ['a', 'b', 'c', 'd'] {
        let key = format!("deck_{d}_dna_morph");
        assert!(
            m.node_names.contains_key(&key),
            "{key} is not registered; the SHAPE toggle for deck {} cannot resolve a target",
            d.to_ascii_uppercase()
        );
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
