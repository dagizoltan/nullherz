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

/// Signal types for strongly typed graph connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalType {
    Audio,
    Feature,
    Control,
    Event,
    Spectrum,
    Envelope,
    Pitch,
    BeatPhase,
    SpatialField,
}

/// Automation update resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutomationRate {
    AudioRate,   // Sample rate (48 kHz)
    ControlRate, // Frame / control rate (~100 Hz)
    EventRate,   // Triggered on discrete musical events
}

/// Discrete musical, performance, or engine triggers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventKind {
    Beat(u32),
    Bar(u32),
    Phrase(u32),
    Drop,
    Break,
    TrackStart(u64),
    TrackEnd(u64),
    SceneChange(String),
    DeckChange(char),
    TopologyChange(u32),
    ModelReady(String),
}

/// Timestamped event structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub timestamp_samples: u64,
    pub payload: [u8; 16],
}

/// Spatial signal domain representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpatialDomain {
    Mono,
    Stereo,
    MidSide,
    Multichannel,
    Ambisonic,
}

/// Neural model lifecycle state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelLifecycle {
    Requested,
    Loading,
    Initializing,
    Warming,
    Benchmarking,
    Ready,
}

/// Priority classification for resource degradation policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProcessingPriority {
    Critical = 0, // Transport, Output, Limiter (Never dropped)
    High = 1,     // Primary channel EQs, Compressors
    Medium = 2,   // Neural controllers, Feature analyzers
    Low = 3,      // Shadow neural processing, diagnostics
}

/// Quality budget specification (0..100 quality scale).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QualityBudget {
    pub quality_level: u8, // 0..100
    pub max_cpu_pct: f32,
    pub max_latency_samples: usize,
}

impl Default for QualityBudget {
    fn default() -> Self {
        Self {
            quality_level: 70,
            max_cpu_pct: 10.0,
            max_latency_samples: 256,
        }
    }
}

/// Global Analysis & Feature Broadcast Bus across compositions.
#[derive(Debug, Clone, Default)]
pub struct AnalysisBus {
    pub features: HashMap<SemanticPort, f32>,
    pub events: Vec<Event>,
}

impl AnalysisBus {
    pub fn new() -> Self {
        Self {
            features: HashMap::new(),
            events: Vec::new(),
        }
    }

    pub fn publish_feature(&mut self, port: SemanticPort, value: f32) {
        self.features.insert(port, value);
    }

    pub fn get_feature(&self, port: &SemanticPort) -> f32 {
        self.features.get(port).copied().unwrap_or(0.0)
    }

    pub fn emit_event(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
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
    pub spatial_domain: SpatialDomain,
    pub priority: ProcessingPriority,
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
            spatial_domain: SpatialDomain::Stereo,
            priority: ProcessingPriority::High,
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

impl std::fmt::Debug for CompositionNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompositionNode::Processor(_) => write!(f, "CompositionNode::Processor"),
            CompositionNode::NestedComposition(_) => write!(f, "CompositionNode::NestedComposition"),
            CompositionNode::Analyzer { name, output_port } => f.debug_struct("CompositionNode::Analyzer")
                .field("name", name)
                .field("output_port", output_port)
                .finish(),
            CompositionNode::NeuralController { name, input_port } => f.debug_struct("CompositionNode::NeuralController")
                .field("name", name)
                .field("input_port", input_port)
                .finish(),
        }
    }
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

/// Staged transaction for atomic graph mutations.
#[derive(Debug)]
pub struct CompositionTransaction {
    pub transaction_id: u64,
    pub target_node_idx: u32,
    pub new_composition_name: String,
    pub commit_boundary: EventKind,
    pub status: ModelLifecycle,
    pub replacement_node: Option<Box<CompositionNode>>,
}

impl CompositionTransaction {
    pub fn new(id: u64, boundary: EventKind) -> Self {
        Self {
            transaction_id: id,
            target_node_idx: 0,
            new_composition_name: String::new(),
            commit_boundary: boundary,
            status: ModelLifecycle::Requested,
            replacement_node: None,
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
    pub quality_budget: QualityBudget,
    pub random_seed: u64,
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
            quality_budget: QualityBudget::default(),
            random_seed: 42,
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
    pub feature_bus: AnalysisBus,
    pub pending_transactions: Vec<CompositionTransaction>,
    pub control_automation: HashMap<u32, (f32, f32)>, // param_id -> (current_val, target_val)
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
            feature_bus: AnalysisBus::new(),
            pending_transactions: Vec::new(),
            control_automation: HashMap::new(),
        }
    }

    pub fn set_automation_target(&mut self, param_id: u32, target: f32) {
        let entry = self.control_automation.entry(param_id).or_insert((0.0, target));
        entry.1 = target;
    }

    pub fn submit_transaction(&mut self, tx: CompositionTransaction) {
        self.pending_transactions.push(tx);
    }

    pub fn set_feature(&mut self, port: SemanticPort, value: f32) {
        self.feature_bus.publish_feature(port, value);
    }

    pub fn get_feature(&self, port: &SemanticPort) -> f32 {
        self.feature_bus.get_feature(port)
    }

    pub fn process_pending_transactions(&mut self, ctx: &ProcessContext, block_len: usize) {
        if self.pending_transactions.is_empty() {
            return;
        }

        let mut i = 0;
        while i < self.pending_transactions.len() {
            if Self::evaluate_commit_boundary(&self.pending_transactions[i], ctx, block_len) {
                let mut tx = self.pending_transactions.remove(i);
                tx.status = ModelLifecycle::Ready;

                if !tx.new_composition_name.is_empty() {
                    self.composition.contract.name = tx.new_composition_name;
                }

                if let Some(new_node) = tx.replacement_node.take() {
                    let idx = tx.target_node_idx as usize;
                    if idx < self.composition.nodes.len() {
                        self.composition.nodes[idx] = *new_node;
                    } else {
                        self.composition.add_node(*new_node);
                    }
                }

                let timestamp = ctx.transport.map(|t| t.absolute_samples).unwrap_or(0) + ctx.sub_block_offset as u64;
                self.feature_bus.emit_event(Event {
                    kind: EventKind::SceneChange(self.composition.contract.name.clone()),
                    timestamp_samples: timestamp,
                    payload: [0; 16],
                });
            } else {
                i += 1;
            }
        }
    }

    fn evaluate_commit_boundary(tx: &CompositionTransaction, ctx: &ProcessContext, block_len: usize) -> bool {
        let Some(t) = ctx.transport else {
            return true;
        };

        if !t.is_playing {
            return true;
        }

        match &tx.commit_boundary {
            EventKind::Beat(n) => {
                let interval = (*n as f64).max(1.0);
                Self::is_boundary_crossed(t, block_len, interval)
            }
            EventKind::Bar(n) => {
                let interval = ((*n as f64) * 4.0).max(4.0);
                Self::is_boundary_crossed(t, block_len, interval)
            }
            EventKind::Phrase(n) => {
                let interval = ((*n as f64) * 16.0).max(16.0);
                Self::is_boundary_crossed(t, block_len, interval)
            }
            EventKind::TrackStart(s) | EventKind::TrackEnd(s) => {
                let start_s = t.absolute_samples;
                let end_s = start_s + block_len as u64;
                *s >= start_s && *s <= end_s
            }
            _ => true,
        }
    }

    fn is_boundary_crossed(t: &nullherz_traits::Transport, block_len: usize, interval_beats: f64) -> bool {
        let bpm = if t.bpm > 0.0 { t.bpm as f64 } else { 120.0 };
        let sr = if t.sample_rate > 0.0 { t.sample_rate as f64 } else { 48000.0 };
        let block_beat_delta = (block_len as f64 / sr) * (bpm / 60.0);
        let prev_beat = t.beat_position;
        let next_beat = prev_beat + block_beat_delta;

        (prev_beat / interval_beats).floor() != (next_beat / interval_beats).floor()
            || (prev_beat % interval_beats) < block_beat_delta
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

        // Automatically populate feature bus from active transport metrics
        if let Some(t) = ctx.transport {
            self.feature_bus.publish_feature(SemanticPort::Tempo, t.bpm);
            self.feature_bus.publish_feature(SemanticPort::BeatPhase, (t.beat_position % 1.0) as f32);
            self.feature_bus.publish_feature(SemanticPort::BarPhase, ((t.beat_position / 4.0) % 1.0) as f32);
            self.feature_bus.publish_feature(SemanticPort::PhrasePhase, ((t.beat_position / 16.0) % 1.0) as f32);
        }

        // Sub-block control-rate parameter smoothing (~100 Hz / sub-block updates)
        let alpha = 0.15f32; // Smoothing coefficient for sub-block control updates
        for (param_id, (current, target)) in self.control_automation.iter_mut() {
            if (*current - *target).abs() > 1e-5 {
                let smoothed = *current + alpha * (*target - *current);
                *current = smoothed;
                let cmd = nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: 0,
                    param_id: *param_id,
                    value: smoothed,
                    ramp_duration_samples: 0,
                });
                for node in &mut self.composition.nodes {
                    if let CompositionNode::Processor(p) = node {
                        p.apply_command(&cmd);
                    }
                }
            }
        }

        self.process_pending_transactions(ctx, block_len);

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
                    let src_scratch = if current_is_a { &self.scratch_a } else { &self.scratch_b };
                    let rms: f32 = (src_scratch[0][..block_len].iter().map(|s| s * s).sum::<f32>() / block_len as f32).sqrt();
                    self.feature_bus.publish_feature(output_port.clone(), rms);
                }
                CompositionNode::NeuralController { name: _, input_port } => {
                    let _val = self.feature_bus.get_feature(input_port);
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
                CompositionNode::NestedComposition(n) => n.apply_midi(event, ctx),
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
                CompositionNode::NestedComposition(n) => n.apply_command(command),
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

    #[test]
    fn test_transaction_commit_boundary_execution() {
        let comp = Composition::new("Base Composition", CompositionType::Linear);
        let mut insert = CompositionInsert::new(comp);

        let mut tx = CompositionTransaction::new(101, EventKind::Beat(1));
        tx.new_composition_name = "New Swapped Composition".to_string();
        insert.submit_transaction(tx);

        let transport = nullherz_traits::Transport {
            bpm: 120.0,
            beat_position: 0.99,
            is_playing: true,
            sample_rate: 48000.0,
            absolute_samples: 0,
            system_time_ns: 0,
            device_time_ns: 0,
        };

        let mut ctx = ProcessContext {
            transport: Some(&transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        let in_l = vec![0.0f32; 256];
        let mut out_l = vec![0.0f32; 256];

        insert.process(&[&in_l], &mut [&mut out_l], &mut ctx);

        // Position crossed beat boundary -> transaction committed!
        assert_eq!(insert.composition.contract.name, "New Swapped Composition");
        assert!(insert.pending_transactions.is_empty());

        let events = insert.feature_bus.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::SceneChange("New Swapped Composition".to_string()));
    }

    #[test]
    fn test_transport_feature_bus_auto_population() {
        let comp = Composition::new("Transport Comp", CompositionType::Linear);
        let mut insert = CompositionInsert::new(comp);

        let transport = nullherz_traits::Transport {
            bpm: 128.0,
            beat_position: 2.5,
            is_playing: true,
            sample_rate: 48000.0,
            absolute_samples: 0,
            system_time_ns: 0,
            device_time_ns: 0,
        };

        let mut ctx = ProcessContext {
            transport: Some(&transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        let in_l = vec![0.0f32; 64];
        let mut out_l = vec![0.0f32; 64];

        insert.process(&[&in_l], &mut [&mut out_l], &mut ctx);

        assert_eq!(insert.get_feature(&SemanticPort::Tempo), 128.0);
        assert!((insert.get_feature(&SemanticPort::BeatPhase) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_control_automation_smoothing() {
        let comp = Composition::new("Automated Comp", CompositionType::Linear);
        let mut insert = CompositionInsert::new(comp);

        insert.set_automation_target(1, 1.0);

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        let in_l = vec![0.0f32; 64];
        let mut out_l = vec![0.0f32; 64];

        insert.process(&[&in_l], &mut [&mut out_l], &mut ctx);

        let current_val = insert.control_automation.get(&1).unwrap().0;
        assert!(current_val > 0.0 && current_val < 1.0);
    }

    #[test]
    fn test_analysis_bus_publishing_and_retrieval() {
        let mut bus = AnalysisBus::new();
        bus.publish_feature(SemanticPort::Tempo, 128.0);
        bus.publish_feature(SemanticPort::Key, 5.0);

        assert_eq!(bus.get_feature(&SemanticPort::Tempo), 128.0);
        assert_eq!(bus.get_feature(&SemanticPort::Key), 5.0);
        assert_eq!(bus.get_feature(&SemanticPort::Peak), 0.0);

        bus.emit_event(Event {
            kind: EventKind::Drop,
            timestamp_samples: 48000,
            payload: [0; 16],
        });

        let events = bus.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::Drop);
        assert!(bus.drain_events().is_empty());
    }
}
