//! A declared parameter range must be the range the processor actually honours.
//!
//! `ParameterMetadata` carries `id, name, min, max, default` — everything a
//! generic editor needs. It is declared by hand, in parallel arrays, entirely
//! separately from the `value.clamp(lo, hi)` in `set_parameter`:
//!
//!     let mins = [0.001, 1.0, 0.1, 0.01];      // metadata()
//!     let maxs = [1.0, 1000.0, 5.0, 1.0];
//!     ...
//!     0 => self.threshold = value.clamp(0.001, 1.0),   // set_parameter
//!
//! Two independent statements of one fact. Nothing has connected them, so a
//! declaration can describe a range the processor will not accept — and a UI
//! built on the declaration would then offer a control whose ends do nothing.
//!
//! This test connects them at runtime: for every registered processor that
//! declares parameters, push each one past both declared ends and assert the
//! value that comes back is inside the declared range.

use nullherz_processors::registry::ProcessorRegistry;

const SAMPLE_RATE: f32 = 48_000.0;

/// `(processor_type, param_id)` pairs whose declaration already disagrees with
/// the clamp. **This list may only SHRINK.**
///
/// 31 pairs, 55 individual disagreements, found when this test was written —
/// so the drift it guards against is not hypothetical, it is the current state
/// of two thirds of the declaring processors. They are allowlisted rather than
/// fixed because each one is a judgement nobody has made yet, and the two ways
/// to resolve it are not equivalent:
///
///   * `Sampler.PlaybackRate` declares `0..2` and clamps nothing, accepting 22.
///     Tightening the clamp to 2 would cap a performance control that may
///     legitimately go higher; widening the declaration admits that 2 was never
///     the limit. Someone has to decide which was intended.
///   * `DeckStemMatrix.Stem*Gain` declares `-96..12` dB and clamps to `-120..24`.
///     Here the clamp is almost certainly right and the declaration stale.
///   * `Modulation.TargetID` declares a range at all, for what is an id.
///
/// Fixing one means editing either its `metadata()` or its `set_parameter`, and
/// removing its line here. Adding a line needs a reason in the same commit: the
/// next reader will take this list as permission.
const KNOWN_DISAGREEMENTS: &[(&str, u32)] = &[
    ("Capture", 0),
    ("Capture", 1),
    ("Compressor", 3),
    ("Compressor", 4),
    ("DeckStemMatrix", 2),
    ("DeckStemMatrix", 12),
    ("DeckStemMatrix", 22),
    ("DeckStemMatrix", 32),
    ("DeckStemMatrix", 42),
    ("DeckStemMatrix", 52),
    ("DeckStemMatrix", 62),
    ("DeckStemMatrix", 72),
    ("DeckStemMatrix", 82),
    ("DeckStemMatrix", 92),
    ("DeckStemMatrix", 102),
    ("DeckStemMatrix", 112),
    ("EnvelopeFollower", 0),
    ("EnvelopeFollower", 1),
    ("Gain", 2),
    ("Granular", 0),
    ("Granular", 1),
    ("KeySync", 0),
    ("Modulation", 0),
    ("Modulation", 1),
    ("Modulation", 2),
    ("Modulation", 3),
    ("Modulation", 4),
    ("Sampler", 1),
    ("Sampler", 4),
    ("Sampler", 5),
    ("Summing", 2),
];

fn is_known(type_name: &str, param_id: u32) -> bool {
    KNOWN_DISAGREEMENTS.iter().any(|(t, id)| *t == type_name && *id == param_id)
}

fn name_of(p: &nullherz_traits::ParameterMetadata) -> String {
    String::from_utf8_lossy(&p.name).trim_matches(char::from(0)).to_string()
}

#[test]
fn test_declared_parameter_ranges_are_the_ranges_honoured() {
    let registry = ProcessorRegistry::new();
    let mut checked_processors = 0usize;
    let mut checked_params = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (id, type_name) in registry.list_available_processors() {
        let Some(mut proc) = registry.create_by_id(id, 0, SAMPLE_RATE) else { continue };
        let Some(meta) = proc.metadata() else { continue };
        let n = (meta.num_parameters as usize).min(meta.parameters.len());
        if n == 0 {
            continue;
        }
        checked_processors += 1;

        for spec in meta.parameters.iter().take(n) {
            let pname = name_of(spec);
            if !spec.min.is_finite() || !spec.max.is_finite() || spec.min > spec.max {
                failures.push(format!(
                    "{type_name}.{pname} (id {}) declares an unusable range [{}, {}]",
                    spec.id, spec.min, spec.max
                ));
                continue;
            }
            checked_params += 1;

            // Well past the declared maximum.
            let over = spec.max + (spec.max - spec.min).abs().max(1.0) * 10.0;
            proc.set_parameter(spec.id, over, 0);
            let got = proc.get_parameter(spec.id);
            if got.is_finite() && got > spec.max + 1e-3 && !is_known(type_name, spec.id) {
                failures.push(format!(
                    "{type_name}.{pname} (id {}) declares max {} but accepted {got} when given {over}",
                    spec.id, spec.max
                ));
            }

            // Well below the declared minimum.
            let under = spec.min - (spec.max - spec.min).abs().max(1.0) * 10.0;
            proc.set_parameter(spec.id, under, 0);
            let got = proc.get_parameter(spec.id);
            if got.is_finite() && got < spec.min - 1e-3 && !is_known(type_name, spec.id) {
                failures.push(format!(
                    "{type_name}.{pname} (id {}) declares min {} but accepted {got} when given {under}",
                    spec.id, spec.min
                ));
            }

            // A declared default must itself be inside the declared range, or
            // a host that resets to it lands outside what the control allows.
            if spec.default.is_finite()
                && (spec.default < spec.min - 1e-3 || spec.default > spec.max + 1e-3)
                && !is_known(type_name, spec.id)
            {
                failures.push(format!(
                    "{type_name}.{pname} (id {}) declares default {} outside its own range [{}, {}]",
                    spec.id, spec.default, spec.min, spec.max
                ));
            }
        }
    }

    assert!(
        checked_processors > 0,
        "no registered processor declared any parameters — this test checked nothing"
    );
    eprintln!(
        "checked {checked_params} declared parameters across {checked_processors} processors"
    );
    assert!(
        failures.is_empty(),
        "declared parameter metadata does not match what these processors accept:\n{}\n\n\
         The declaration is what a generic editor draws, so a range the processor refuses becomes \
         a control whose ends do nothing. Declare the range the clamp enforces, or clamp to the \
         range declared.",
        failures.join("\n")
    );
}

/// A processor that reports `num_parameters` must describe that many, and the
/// ids must be distinct — a host keys controls by id, so a duplicate silently
/// collapses two controls into one.
#[test]
fn test_declared_parameter_ids_are_distinct_and_counted_correctly() {
    let registry = ProcessorRegistry::new();
    let mut failures: Vec<String> = Vec::new();

    for (id, type_name) in registry.list_available_processors() {
        let Some(proc) = registry.create_by_id(id, 0, SAMPLE_RATE) else { continue };
        let Some(meta) = proc.metadata() else { continue };
        let n = meta.num_parameters as usize;
        if n == 0 {
            continue;
        }
        if n > meta.parameters.len() {
            failures.push(format!(
                "{type_name} reports {n} parameters but the array holds {}",
                meta.parameters.len()
            ));
            continue;
        }
        let ids: Vec<u32> = meta.parameters.iter().take(n).map(|p| p.id).collect();
        let mut seen = ids.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() != ids.len() {
            failures.push(format!("{type_name} declares duplicate parameter ids: {ids:?}"));
        }
        let unnamed: Vec<u32> = meta
            .parameters
            .iter()
            .take(n)
            .filter(|p| name_of(p).is_empty())
            .map(|p| p.id)
            .collect();
        if !unnamed.is_empty() {
            failures.push(format!(
                "{type_name} declares parameters with no name (ids {unnamed:?}) — a host has \
                 nothing to label the control with"
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The ratchet must actually be a ratchet: an allowlist entry that no longer
/// disagrees is a line to delete, not a permanent exemption. This fails when
/// one goes stale, so the list cannot quietly outlive the problem.
#[test]
fn test_the_allowlist_has_no_stale_entries() {
    let registry = ProcessorRegistry::new();
    let mut still_disagreeing: Vec<(String, u32)> = Vec::new();

    for (id, type_name) in registry.list_available_processors() {
        let Some(mut proc) = registry.create_by_id(id, 0, SAMPLE_RATE) else { continue };
        let Some(meta) = proc.metadata() else { continue };
        let n = (meta.num_parameters as usize).min(meta.parameters.len());
        for spec in meta.parameters.iter().take(n) {
            if !is_known(type_name, spec.id) {
                continue;
            }
            if !spec.min.is_finite() || !spec.max.is_finite() || spec.min > spec.max {
                still_disagreeing.push((type_name.to_string(), spec.id));
                continue;
            }
            let over = spec.max + (spec.max - spec.min).abs().max(1.0) * 10.0;
            proc.set_parameter(spec.id, over, 0);
            let hi = proc.get_parameter(spec.id);
            let under = spec.min - (spec.max - spec.min).abs().max(1.0) * 10.0;
            proc.set_parameter(spec.id, under, 0);
            let lo = proc.get_parameter(spec.id);
            let bad_default = spec.default.is_finite()
                && (spec.default < spec.min - 1e-3 || spec.default > spec.max + 1e-3);
            if (hi.is_finite() && hi > spec.max + 1e-3)
                || (lo.is_finite() && lo < spec.min - 1e-3)
                || bad_default
            {
                still_disagreeing.push((type_name.to_string(), spec.id));
            }
        }
    }

    let stale: Vec<&(&str, u32)> = KNOWN_DISAGREEMENTS
        .iter()
        .filter(|(t, id)| !still_disagreeing.iter().any(|(st, sid)| st == t && sid == id))
        .collect();

    assert!(
        stale.is_empty(),
        "these allowlist entries no longer disagree and should be deleted: {stale:?}\n\
         An allowlist that outlives its reason reads as permission."
    );
}
