//! Reachability gate: things that EXIST must also be REACHABLE.
//!
//! The July review found four subsystems that were fully built, unit-tested, and
//! completely unreachable in the running product:
//!
//!   * `StreamingManager` — disk thread, SPSC ring, SHM feeder, all written, bound
//!     to a node type the console never instantiates.
//!   * `PersonalityInheritanceProcessor` — the only implementation of the
//!     donor→recipient transfer model, never instantiated; its source-DNA setter
//!     called only from a unit test, so the donor was permanently all-zero.
//!   * The deck DNA panel — four knobs writing to a metadata field no processor
//!     reads.
//!   * The Breeder view — addressing node 150, which is a `ProcessorTypeId`, not a
//!     graph index.
//!
//! Every one of them had a passing test. That is the point: a unit test proves a
//! thing WORKS, not that anything CALLS it. These tests close that gap, so a
//! `[V]` tag in the docs can mean "a user can do this" rather than "a test
//! passes".

use nullherz_conductor::Conductor;
use nullherz_processors::registry::ProcessorRegistry;
use std::collections::HashSet;

/// Instantiate the real 4-deck console and report which processor type ids it
/// actually puts in the graph.
fn bootstrap_instantiated_type_ids() -> HashSet<u32> {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();
    conductor
        .topology_manager
        .active_node_types
        .values()
        .copied()
        .collect()
}

/// Processor types that are registered but deliberately NOT part of the default
/// console graph. Every entry needs a reason, and the reason should be a plan or
/// a decision — not "it doesn't work yet".
///
/// Shrinking this list is the point. An entry that has sat here for months is a
/// feature nobody can use.
fn known_unreachable() -> Vec<(&'static str, &'static str)> {
    vec![
        ("PersonalityInheritance", "donor→recipient transfer; wiring is Phase 6 of the roadmap"),
        ("StreamingSampler", "disk streaming; wiring is Phase 2 of the roadmap"),
        ("Granular", "reachable only via the transfusion/breeder path, not the console"),
        ("SpectralMorph", "reachable only via the transfusion path"),
        ("Spectral", "generic spectral node, instantiated by sidecars/plugins"),
        ("Wavetable", "synthesis source, not part of a DJ deck chain"),
        ("Modulation", "modulation source, wired by the mod matrix rather than the mixer"),
        ("EnvelopeFollower", "modulation source, wired by the mod matrix"),
        ("Compressor", "available for FX chains; not in the default master chain"),
        ("StereoUtility", "available for FX chains; not in the default master chain"),
        ("Analysis", "measurement tap, inserted on demand"),
        ("Delay", "available for FX chains; not in the default master chain"),
        ("SimdBiquad", "multi-channel biquad used by FX chains, not the deck EQ"),
        ("KeySync", "INSTALLED ON DEMAND, not absent: the KEY latch swaps it into \
          the deck's pitch slot (SetDeckKeySync -> SwapProcessor). It is out of the \
          default chain because its 1024-sample FFT window cost every deck 21.3 ms \
          of latency to serve a latch that defaults OFF (roadmap 1.12)"),
        ("DnaMorph", "INSTALLED ON DEMAND into the deck's DNA slot, same mechanism \
          as KeySync; also still disengaged until real DNA is fed (Phase 6)"),
    ]
}

#[test]
fn test_every_registered_processor_is_reachable_or_declared() {
    let registry = ProcessorRegistry::new();
    let instantiated = bootstrap_instantiated_type_ids();
    let allowed: Vec<&str> = known_unreachable().iter().map(|(n, _)| *n).collect();

    let mut unreachable_and_undeclared = Vec::new();
    for (id, name) in registry.list_available_processors() {
        if instantiated.contains(&id) {
            continue;
        }
        if allowed.contains(&name) {
            continue;
        }
        unreachable_and_undeclared.push(format!("{name} (type id {id})"));
    }

    unreachable_and_undeclared.sort();
    assert!(
        unreachable_and_undeclared.is_empty(),
        "these processors are registered but never instantiated by the console, \
         and are not declared in `known_unreachable()`:\n  {}\n\n\
         Either wire them into the graph, or add them to that list WITH A REASON. \
         A registered processor nobody can reach is a feature that does not exist.",
        unreachable_and_undeclared.join("\n  ")
    );
}

/// The allowlist must not rot: an entry naming a processor that no longer exists
/// (renamed, deleted) silently stops guarding anything.
#[test]
fn test_unreachable_allowlist_has_no_stale_entries() {
    let registry = ProcessorRegistry::new();
    let registered: HashSet<&str> = registry
        .list_available_processors()
        .into_iter()
        .map(|(_, name)| name)
        .collect();

    let stale: Vec<&str> = known_unreachable()
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| !registered.contains(n))
        .collect();

    assert!(
        stale.is_empty(),
        "`known_unreachable()` names processors that are no longer registered: \
         {stale:?} — remove them so the list keeps meaning something"
    );
}

/// Every well-known node name the UI resolves must exist in the bootstrapped
/// graph.
///
/// `AGENTS.md` requires UI controls to resolve node ids BY NAME and to skip the
/// command when the lookup fails — which is safe, but silent. A control bound to
/// a name the bootstrap never registers simply does nothing, forever, with no
/// error anywhere. This test found exactly that: two controls in the Sampler view
/// resolve `sampler_node`, which the mixer never registers.
#[test]
fn test_ui_node_names_resolve_in_the_bootstrapped_graph() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();
    let names = &conductor.mixer_manager.node_names;

    // Names addressed by the inspector's views. Keep in step with the UI; the
    // point of the list is that adding a control with a typo'd or invented name
    // fails here instead of silently doing nothing.
    let mut required: Vec<String> = vec![
        "capture_node".into(),
        "master_eq".into(),
        "master_sum_l".into(),
        "master_limiter".into(),
        "preview_node".into(),
    ];
    for deck in ['a', 'b', 'c', 'd'] {
        for suffix in ["sampler", "gain", "filter", "isolator", "sequencer"] {
            required.push(format!("deck_{deck}_{suffix}"));
        }
    }

    let missing: Vec<&String> = required.iter().filter(|n| !names.contains_key(*n)).collect();
    assert!(
        missing.is_empty(),
        "the UI resolves these node names but the bootstrap never registers them, \
         so every control bound to them silently does nothing: {missing:?}\n\
         Registered names: {:?}",
        {
            let mut k: Vec<&String> = names.keys().collect();
            k.sort();
            k
        }
    );

    // The list above only guards names someone remembered to add. Scrape the
    // views for LITERAL `get_node_id("...")` arguments too, so a control invented
    // with a name the mixer never registers fails here instead of silently doing
    // nothing. (Dynamic `format!` lookups are covered by the explicit list.)
    let views = concat!(env!("CARGO_MANIFEST_DIR"), "/../nullherz-inspector/src/views");
    let mut unregistered = Vec::new();
    fn scan_names(dir: &std::path::Path, names: &std::collections::HashMap<String, u32>, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_names(&path, names, out);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let mut in_tests = false;
            for (i, line) in text.lines().enumerate() {
                let t = line.trim();
                if t.contains("#[cfg(test)]") {
                    in_tests = true;
                }
                if in_tests || t.starts_with("//") {
                    continue;
                }
                let mut rest = t;
                while let Some(pos) = rest.find("get_node_id(\"") {
                    let after = &rest[pos + "get_node_id(\"".len()..];
                    if let Some(end) = after.find('"') {
                        let name = &after[..end];
                        if !names.contains_key(name) {
                            out.push(format!(
                                "{}:{}: get_node_id(\"{name}\")",
                                path.file_name().unwrap().to_string_lossy(),
                                i + 1
                            ));
                        }
                        rest = &after[end..];
                    } else {
                        break;
                    }
                }
            }
        }
    }
    scan_names(std::path::Path::new(views), names, &mut unregistered);
    unregistered.sort();
    unregistered.dedup();
    assert!(
        unregistered.is_empty(),
        "these views resolve node names the bootstrap never registers, so the \
         controls bound to them do nothing (the lookup fails safe, which is why \
         it goes unnoticed):\n  {}",
        unregistered.join("\n  ")
    );
}

/// Literal node indices that are known, declared, and tracked. Same discipline as
/// `known_unreachable()`: an entry needs a reason, and the list should shrink.
fn declared_literal_node_indices() -> Vec<(&'static str, &'static str)> {
    vec![(
        "breeder.rs",
        "target_node_idx: 150 is ProcessorTypeId::PERSONALITY_INHERITANCE used as a \
         graph index. It cannot be resolved by name yet because that processor is \
         not in the console graph at all (see `known_unreachable`); both are fixed \
         together when transfer is wired in Phase 6. Out of range, so commands are \
         dropped rather than misdirected.",
    )]
}

/// Node indices must come from a name lookup, never from a literal.
///
/// The Breeder view shipped `target_node_idx: 150` — that is
/// `ProcessorTypeId::PERSONALITY_INHERITANCE`, used as a GRAPH index. It was out
/// of range so the engine dropped every command it sent. The notifications view
/// shipped `granular_node_idx: 4`, which is WORSE: 4 is a real node in deck A's
/// chain, so its "LOAD" button aimed at whatever happened to sit there.
///
/// A range check cannot catch this in general — type ids and node indices
/// genuinely overlap (`ProcessorTypeId::DELAY` is 0, which is also deck A's
/// sampler), so a low literal addresses a real, wrong node instead of failing.
#[test]
fn test_ui_views_do_not_hardcode_node_indices() {
    let views = concat!(env!("CARGO_MANIFEST_DIR"), "/../nullherz-inspector/src/views");
    // Fields that carry a GRAPH INDEX to the engine. Matched on the whole field
    // name, not a suffix: `selected_hotload_node_idx` is UI selection state and
    // ends in `node_idx`, but never reaches the engine.
    const TARGET_FIELDS: [&str; 3] = ["node_idx", "target_node_idx", "granular_node_idx"];
    let mut offenders: Vec<String> = Vec::new();

    fn scan(dir: &std::path::Path, fields: &[&str], offenders: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan(&path, fields, offenders);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let mut in_tests = false;
            for (i, line) in text.lines().enumerate() {
                let t = line.trim();
                if t.contains("#[cfg(test)]") {
                    in_tests = true;
                }
                if in_tests || t.starts_with("//") {
                    continue;
                }
                for field in fields {
                    // Whole-field match: the character before the field name must
                    // not be part of an identifier.
                    let Some(pos) = t.find(&format!("{field}:")) else { continue };
                    let preceded_by_ident = pos > 0
                        && t[..pos]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_');
                    if preceded_by_ident {
                        continue;
                    }
                    let rest = t[pos + field.len() + 1..].trim_start();
                    if rest.starts_with(|c: char| c.is_ascii_digit()) {
                        offenders.push((
                            path.file_name().unwrap().to_string_lossy().into_owned(),
                            format!("{}:{}: {}", path.file_name().unwrap().to_string_lossy(), i + 1, t),
                        ));
                        break; // one report per line
                    }
                }
            }
        }
    }
    let mut found: Vec<(String, String)> = Vec::new();
    scan(std::path::Path::new(views), &TARGET_FIELDS, &mut found);

    let declared: Vec<&str> = declared_literal_node_indices().iter().map(|(f, _)| *f).collect();
    for (file, report) in found {
        if !declared.contains(&file.as_str()) {
            offenders.push(report);
        }
    }

    offenders.sort();
    offenders.dedup();
    assert!(
        offenders.is_empty(),
        "UI views must resolve node indices BY NAME (`app.get_node_id(..)`), never \
         hardcode them.\n\n\
         A literal is worse than it looks: type ids and node indices OVERLAP, so a \
         low literal addresses a real but WRONG node rather than being dropped. \
         (`ProcessorTypeId::DELAY` is 0, which is also deck A's sampler.)\n\n\
         Offenders:\n  {}\n\n\
         Fix by resolving the name, or add the file to \
         `declared_literal_node_indices()` with a reason.",
        offenders.join("\n  ")
    );
}
