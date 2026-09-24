use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::collections::HashMap;
use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand, MidiEvent,
};

/// Execution urgency and resource budget class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionClass {
    Realtime,  // <1ms deterministic DSP
    Fast,      // 1-3ms frame-rate analysis/DSP
    Control,   // ~50-200Hz control rate updates
    Async,     // Off-thread neural worker
    Offline,   // Pre-rendering / bounce
}

/// Semantic / Musical Port identifier for feature and control signal routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticPort {
    Tempo,
    BeatPhase,
    BarPhase,
    PhrasePhase,
    Key,
    Pitch,
    Rms,
    Lufs,
    Peak,
    BassEnergy,
    MidEnergy,
    HighEnergy,
    SpectralCentroid,
    SpectralFlux,
    TransientDensity,
    HarmonicDensity,
    VocalPresence,
    RhythmicDensity,
    DeckId,
    MasterEnergy,
    Custom(String),
}

/// Topological configuration type of a Composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompositionType {
    Linear,
    Parallel,
    Hybrid,
    Mesh,
    Nested,
}

/// Declaration contract for a Composition or Insert module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionContract {
    pub name: String,
    pub description: String,
    pub composition_type: CompositionType,
    pub input_channels: usize,
    pub output_channels: usize,
    pub latency_samples: usize,
    pub execution_class: ExecutionClass,
    pub cpu_cost_pct: f32,
    pub requires_gpu: bool,
    pub exposed_ports: Vec<SemanticPort>,
}

impl Default for CompositionContract {
    fn default() -> Self {
        Self {
            name: "Default Composition".to_string(),
            description: "Modular DSP composition".to_string(),
            composition_type: CompositionType::Linear,
            input_channels: 2,
            output_channels: 2,
            latency_samples: 0,
            execution_class: ExecutionClass::Realtime,
            cpu_cost_pct: 1.0,
            requires_gpu: false,
            exposed_ports: vec![],
        }
    }
}

/// A node inside a Composition graph.
pub enum CompositionNode {
    Processor(Box<dyn AudioProcessor>),
    NestedComposition(Box<CompositionInsert>),
    Analyzer {
        name: String,
        output_port: SemanticPort,
    },
    NeuralController {
        name: String,
        input_port: SemanticPort,
    },
}

impl CompositionNode {
    pub fn latency_samples(&self) -> usize {
        match self {
            CompositionNode::Processor(p) => p.latency_samples(),
            CompositionNode::NestedComposition(c) => c.composition.effective_latency(),
            CompositionNode::Analyzer { .. } => 0,
            CompositionNode::NeuralController { .. } => 0,
        }
    }
}

/// A Composition graph representation that can be linear, parallel, hybrid, mesh, or nested.
pub struct Composition {
    pub contract: CompositionContract,
    pub nodes: Vec<CompositionNode>,
    pub connections: Vec<(usize, usize)>, // (src_node_idx, dst_node_idx)
    pub shadow_mode_enabled: bool,
    pub shadow_mix: f32,
    pub confidence: f32,
}

impl Composition {
    pub fn new(name: &str, composition_type: CompositionType) -> Self {
        Self {
            contract: CompositionContract {
                name: name.to_string(),
                composition_type,
                ..Default::default()
            },
            nodes: Vec::new(),
            connections: Vec::new(),
            shadow_mode_enabled: false,
            shadow_mix: 0.0,
            confidence: 1.0,
        }
    }

    pub fn add_node(&mut self, node: CompositionNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        idx
    }

    pub fn connect(&mut self, src: usize, dst: usize) {
        self.connections.push((src, dst));
    }

    /// Calculate total effective latency through nested compositions.
    pub fn effective_latency(&self) -> usize {
        if self.nodes.is_empty() {
            return self.contract.latency_samples;
        }

        match self.contract.composition_type {
            CompositionType::Linear => self.nodes.iter().map(|n| n.latency_samples()).sum(),
            CompositionType::Parallel => self.nodes.iter().map(|n| n.latency_samples()).max().unwrap_or(0),
            CompositionType::Hybrid | CompositionType::Mesh | CompositionType::Nested => {
                let node_lats: Vec<usize> = self.nodes.iter().map(|n| n.latency_samples()).collect();
                if self.connections.is_empty() {
                    node_lats.iter().sum()
                } else {
                    // Compute max path latency across connections
                    let mut max_path = 0;
                    for &(src, dst) in &self.connections {
                        let path_lat = node_lats.get(src).copied().unwrap_or(0) + node_lats.get(dst).copied().unwrap_or(0);
                        max_path = max_path.max(path_lat);
                    }
                    max_path.max(node_lats.iter().cloned().max().unwrap_or(0))
                }
            }
        }
    }
}

/// An Insert wrapper embedding a Composition as a single first-class AudioProcessor.
pub struct CompositionInsert {
    pub composition: Composition,
    scratch_a: Vec<Vec<f32>>,
    scratch_b: Vec<Vec<f32>>,
    feature_bus: HashMap<SemanticPort, f32>,
}

impl CompositionInsert {
    pub fn new(composition: Composition) -> Self {
        let max_channels = 16;
        let block_size = 1024;
        let scratch_a = vec![vec![0.0f32; block_size]; max_channels];
        let scratch_b = vec![vec![0.0f32; block_size]; max_channels];

        Self {
            composition,
            scratch_a,
            scratch_b,
            feature_bus: HashMap::new(),
        }
    }

    pub fn set_feature(&mut self, port: SemanticPort, value: f32) {
        self.feature_bus.insert(port, value);
    }

    pub fn get_feature(&self, port: &SemanticPort) -> f32 {
        self.feature_bus.get(port).copied().unwrap_or(0.0)
    }
}

impl SignalProcessor for CompositionInsert {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        let num_out = outputs.len().min(16);
        if num_out == 0 { return; }
        let block_len = outputs[0].len().min(1024);

        if self.scratch_a[0].len() < block_len {
            for ch in 0..16 {
                self.scratch_a[ch].resize(block_len, 0.0);
                self.scratch_b[ch].resize(block_len, 0.0);
            }
        }

        // Initialize scratch_a with input audio
        let num_in = inputs.len().min(16);
        for ch in 0..num_out {
            if ch < num_in {
                let n = inputs[ch].len().min(block_len);
                self.scratch_a[ch][..n].copy_from_slice(&inputs[ch][..n]);
            } else {
                self.scratch_a[ch][..block_len].fill(0.0);
            }
        }

        let mut current_is_a = true;

        // Process internal nodes sequentially in-place
        for node in &mut self.composition.nodes {
            match node {
                CompositionNode::Processor(p) => {
                    let (src_scratch, dst_scratch) = if current_is_a {
                        (&self.scratch_a, &mut self.scratch_b)
                    } else {
                        (&self.scratch_b, &mut self.scratch_a)
                    };

                    let in_slices_arr: [&[f32]; 16] = std::array::from_fn(|i| &src_scratch[i][..block_len]);

                    let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
                    for ch in 0..num_out {
                        out_ptrs[ch] = dst_scratch[ch].as_mut_ptr();
                    }
                    let mut out_slices_arr: [&mut [f32]; 16] = std::array::from_fn(|i| {
                        if !out_ptrs[i].is_null() {
                            unsafe { std::slice::from_raw_parts_mut(out_ptrs[i], block_len) }
                        } else {
                            &mut [][..]
                        }
                    });

                    p.process(&in_slices_arr[..num_out], &mut out_slices_arr[..num_out], ctx);
                    current_is_a = !current_is_a;
                }
                CompositionNode::NestedComposition(nested) => {
                    let (src_scratch, dst_scratch) = if current_is_a {
                        (&self.scratch_a, &mut self.scratch_b)
                    } else {
                        (&self.scratch_b, &mut self.scratch_a)
                    };

                    let in_slices_arr: [&[f32]; 16] = std::array::from_fn(|i| &src_scratch[i][..block_len]);

                    let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
                    for ch in 0..num_out {
                        out_ptrs[ch] = dst_scratch[ch].as_mut_ptr();
                    }
                    let mut out_slices_arr: [&mut [f32]; 16] = std::array::from_fn(|i| {
                        if !out_ptrs[i].is_null() {
                            unsafe { std::slice::from_raw_parts_mut(out_ptrs[i], block_len) }
                        } else {
                            &mut [][..]
                        }
                    });

                    nested.process(&in_slices_arr[..num_out], &mut out_slices_arr[..num_out], ctx);
                    current_is_a = !current_is_a;
                }
                CompositionNode::Analyzer { name: _, output_port } => {
                    // Compute RMS feature from active scratch
                    let src_scratch = if current_is_a { &self.scratch_a } else { &self.scratch_b };
                    let rms: f32 = (src_scratch[0][..block_len].iter().map(|s| s * s).sum::<f32>() / block_len as f32).sqrt();
                    self.feature_bus.insert(output_port.clone(), rms);
                }
                CompositionNode::NeuralController { name: _, input_port } => {
                    let _val = self.feature_bus.get(input_port).copied().unwrap_or(0.0);
                }
            }
        }

        // Final copy from active scratch buffer to output
        let final_scratch = if current_is_a { &self.scratch_a } else { &self.scratch_b };
        for ch in 0..num_out {
            let n = outputs[ch].len().min(block_len);

            if self.composition.shadow_mode_enabled {
                let mix = self.composition.shadow_mix * self.composition.confidence;
                for i in 0..n {
                    let trad = inputs[ch.min(num_in - 1)][i];
                    let neural = final_scratch[ch][i];
                    outputs[ch][i] = trad * (1.0 - mix) + neural * mix;
                }
            } else {
                outputs[ch][..n].copy_from_slice(&final_scratch[ch][..n]);
            }
        }
    }

    fn latency_samples(&self) -> usize {
        self.composition.effective_latency()
    }
}

impl MidiResponder for CompositionInsert {
    fn apply_midi(&mut self, event: MidiEvent, ctx: Option<&ProcessContext>) {
        for node in &mut self.composition.nodes {
            match node {
                CompositionNode::Processor(p) => p.apply_midi(event, ctx),
                CompositionNode::NestedComposition(_) => {},
                _ => {}
            }
        }
    }
}

impl SnapshotProvider for CompositionInsert {
    fn pull_snapshot(&mut self) -> Option<Arc<Vec<f32>>> {
        for node in &mut self.composition.nodes {
            if let CompositionNode::Processor(p) = node {
                if let Some(snap) = p.pull_snapshot() {
                    return Some(snap);
                }
            }
        }
        None
    }
}

impl AudioProcessor for CompositionInsert {
    fn apply_command(&mut self, command: &ProcessorCommand) {
        for node in &mut self.composition.nodes {
            match node {
                CompositionNode::Processor(p) => p.apply_command(command),
                CompositionNode::NestedComposition(_) => {},
                _ => {}
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

#[cfg(test)]
mod composition_tests {
    use super::*;

    struct SimpleGain {
        gain: f32,
        latency: usize,
    }
    impl SignalProcessor for SimpleGain {
        fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
            for (in_ch, out_ch) in inputs.iter().zip(outputs.iter_mut()) {
                for (i, o) in in_ch.iter().zip(out_ch.iter_mut()) {
                    *o = i * self.gain;
                }
            }
        }
        fn latency_samples(&self) -> usize { self.latency }
    }
    impl MidiResponder for SimpleGain {}
    impl SnapshotProvider for SimpleGain {}
    impl AudioProcessor for SimpleGain {
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    #[test]
    fn test_nested_composition_latency_calculation() {
        let mut inner_comp = Composition::new("Inner Composition", CompositionType::Linear);
        inner_comp.add_node(CompositionNode::Processor(Box::new(SimpleGain { gain: 1.0, latency: 64 })));
        inner_comp.add_node(CompositionNode::Processor(Box::new(SimpleGain { gain: 1.0, latency: 64 })));
        assert_eq!(inner_comp.effective_latency(), 128);

        let mut outer_comp = Composition::new("Outer Composition", CompositionType::Linear);
        outer_comp.add_node(CompositionNode::Processor(Box::new(SimpleGain { gain: 1.0, latency: 32 })));
        outer_comp.add_node(CompositionNode::NestedComposition(Box::new(CompositionInsert::new(inner_comp))));
        assert_eq!(outer_comp.effective_latency(), 160);
    }

    #[test]
    fn test_composition_insert_execution_and_analyzer() {
        let mut comp = Composition::new("Hybrid Composition", CompositionType::Linear);
        comp.add_node(CompositionNode::Processor(Box::new(SimpleGain { gain: 2.0, latency: 0 })));
        comp.add_node(CompositionNode::Analyzer {
            name: "RMS Analyzer".to_string(),
            output_port: SemanticPort::Rms,
        });

        let mut insert = CompositionInsert::new(comp);

        let in_l = vec![0.5f32; 256];
        let in_r = vec![0.5f32; 256];
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        insert.process(&[&in_l, &in_r], &mut [&mut out_l, &mut out_r], &mut ctx);

        // 2.0 gain applied
        assert!((out_l[0] - 1.0).abs() < 1e-5);
        assert!((out_r[0] - 1.0).abs() < 1e-5);

        // Analyzer extracted RMS
        let rms = insert.get_feature(&SemanticPort::Rms);
        assert!((rms - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_shadow_mode_confidence_blending() {
        let mut comp = Composition::new("Shadow Composition", CompositionType::Linear);
        comp.add_node(CompositionNode::Processor(Box::new(SimpleGain { gain: 2.0, latency: 0 })));
        comp.shadow_mode_enabled = true;
        comp.shadow_mix = 0.5;
        comp.confidence = 1.0;

        let mut insert = CompositionInsert::new(comp);

        let in_l = vec![1.0f32; 64];
        let mut out_l = vec![0.0f32; 64];

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        insert.process(&[&in_l], &mut [&mut out_l], &mut ctx);

        // Input = 1.0, Neural = 2.0, Shadow mix = 0.5 -> Output = 1.0*0.5 + 2.0*0.5 = 1.5
        assert!((out_l[0] - 1.5).abs() < 1e-5);
    }
}
