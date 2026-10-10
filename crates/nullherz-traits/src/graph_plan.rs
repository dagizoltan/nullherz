use crate::*;

#[derive(Debug, Clone, Copy)]
pub struct ParameterMetadata {
    pub id: u32,
    pub name: [u8; 32],
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

/// One parameter, declared once.
///
/// `ParameterMetadata` below is what a host reads; the clamp inside
/// `set_parameter` is what the processor enforces. Stating a range in both
/// places is how they came to disagree: measured by
/// `declared_params_match_behaviour_test`, 31 parameters across 10 processors
/// declare a range their clamp does not honour — `Sampler.PlaybackRate` offers
/// `0..2` and accepts 22, `DeckStemMatrix.Stem1Gain` offers `-96..12` dB and
/// accepts `-120..24`. A generic editor drawing the declaration would give
/// those controls ends that do nothing.
///
/// A `ParamSpec` table is the single statement both derive from: `metadata()`
/// publishes it and `clamp` enforces it, so there is no second place to drift
/// to. See `AlgorithmicReverbProcessor` for the reference use.
#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    pub id: u32,
    /// Shown on the control. Truncated to 32 bytes when published.
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ParamSpec {
    /// Clamp a value into this parameter's declared range.
    ///
    /// Non-finite input returns `None`: a NaN written into a filter coefficient
    /// or a delay length propagates through the graph, and `AGENTS.md` §2
    /// requires processors not to produce one.
    #[inline]
    pub fn clamp_value(&self, value: f32) -> Option<f32> {
        if !value.is_finite() {
            return None;
        }
        Some(value.clamp(self.min, self.max))
    }

    /// Find a spec by id. Linear over a handful of entries, called from
    /// `set_parameter` on the control path, not per sample.
    #[inline]
    pub fn find(specs: &'static [ParamSpec], id: u32) -> Option<&'static ParamSpec> {
        specs.iter().find(|s| s.id == id)
    }

    /// Clamp against the table, or `None` when the id is unknown or the value
    /// is not finite — so a caller can tell "rejected" from "accepted as 0.0".
    #[inline]
    pub fn clamp_in(specs: &'static [ParamSpec], id: u32, value: f32) -> Option<f32> {
        Self::find(specs, id)?.clamp_value(value)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessorMetadata {
    pub processor_id: u64,
    pub num_parameters: u32,
    pub parameters: [ParameterMetadata; 16],
}

impl ProcessorMetadata {
    /// Publish a [`ParamSpec`] table as the wire-format metadata.
    ///
    /// The point of going through here rather than filling the arrays by hand:
    /// the ranges a host is told about are then literally the same values the
    /// processor clamps to, because both read one table.
    ///
    /// Entries past the 16 the wire format carries are dropped rather than
    /// silently reinterpreted; nothing in the tree declares that many, and a
    /// processor that grows to need it should widen the format deliberately.
    pub fn from_specs(processor_id: u64, specs: &'static [ParamSpec]) -> Self {
        let mut parameters = [ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 }; 16];
        let n = specs.len().min(parameters.len());
        for (slot, spec) in parameters.iter_mut().zip(specs.iter()).take(n) {
            slot.id = spec.id;
            slot.min = spec.min;
            slot.max = spec.max;
            slot.default = spec.default;
            let bytes = spec.name.as_bytes();
            let len = bytes.len().min(slot.name.len());
            slot.name[..len].copy_from_slice(&bytes[..len]);
        }
        Self { processor_id, num_parameters: n as u32, parameters }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StageNodes(#[serde(with = "BigArray")] pub [u32; MAX_NODES]);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CompiledGraphPlan {
    #[serde(with = "BigArray")]
    pub stages: [StageNodes; MAX_NODES],
    #[serde(with = "BigArray")]
    pub stage_counts: [u32; MAX_NODES],
    pub num_stages: usize,
    /// Disjoint sub-graph identification for partial re-compilation and optimized O(1) swaps.
    #[serde(with = "BigArray")]
    pub node_islands: [u8; MAX_NODES],
    /// Per-node compensation delay in samples.
    #[serde(with = "BigArray")]
    pub node_latencies: [u32; MAX_NODES],
    /// Per-node, per-input compensation delay.
    #[serde(with = "BigArray")]
    pub input_delays: [InputDelays; MAX_NODES],
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InputDelays(#[serde(with = "BigArray")] pub [f32; MAX_CHANNELS]);

impl Default for CompiledGraphPlan {
    fn default() -> Self {
        Self {
            stages: [StageNodes([0; MAX_NODES]); MAX_NODES],
            stage_counts: [0; MAX_NODES],
            num_stages: 0,
            node_islands: [0; MAX_NODES],
            node_latencies: [0; MAX_NODES],
            input_delays: [InputDelays([0.0; MAX_CHANNELS]); MAX_NODES],
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NodeRouting {
    pub input_indices: [BufferId; MAX_CHANNELS],
    pub output_indices: [BufferId; MAX_CHANNELS],
    pub sidechain_indices: [BufferId; MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
    pub sidechain_count: usize,
    /// Delay compensation required for this node's inputs in samples.
    pub input_delays: [f32; MAX_CHANNELS],
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct NodeAssignment(#[serde(with = "BigArray")] pub [u8; 32]);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NodeAssignmentArray(#[serde(with = "BigArray")] pub [NodeAssignment; MAX_NODES]);

impl Default for NodeAssignmentArray {
    fn default() -> Self {
        Self([NodeAssignment::default(); MAX_NODES])
    }
}
