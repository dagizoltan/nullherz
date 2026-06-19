use nullherz_traits::{AudioProcessor, ProcessorFactory};
use crate::gain::*;
use crate::biquad::*;
use crate::crossfader::*;
use crate::summing::*;
use crate::wavetable::*;
use crate::spectral::*;
use crate::sampler::*;
use crate::modulation::*;
use crate::sequencer::*;

pub struct GainFactory;
impl ProcessorFactory for GainFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(GainProcessor::new(node_idx as u64, 1.0)))
    }
    fn name(&self) -> &'static str { "Gain" }
}

pub struct BiquadFactory;
impl ProcessorFactory for BiquadFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        let coeffs = audio_dsp::BiquadCoefficients {
            b0: 0.1, b1: 0.2, b2: 0.1, a1: -0.5, a2: 0.2
        };
        Some(Box::new(BiquadProcessor::new(node_idx as u64, coeffs)))
    }
    fn name(&self) -> &'static str { "Biquad" }
}

pub struct SamplerFactory;
impl ProcessorFactory for SamplerFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SamplerProcessor::new(node_idx as u64)))
    }
    fn name(&self) -> &'static str { "Sampler" }
}

pub struct CrossfaderFactory;
impl ProcessorFactory for CrossfaderFactory {
    fn create_processor(&self, _node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(CrossfaderProcessor::new()))
    }
    fn name(&self) -> &'static str { "Crossfader" }
}

pub struct SummingFactory;
impl ProcessorFactory for SummingFactory {
    fn create_processor(&self, _node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SummingProcessor::new()))
    }
    fn name(&self) -> &'static str { "Summing" }
}

pub struct SpectralFactory;
impl ProcessorFactory for SpectralFactory {
    fn create_processor(&self, _node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SpectralProcessor::new(1024)))
    }
    fn name(&self) -> &'static str { "Spectral" }
}

pub struct WavetableFactory;
impl ProcessorFactory for WavetableFactory {
    fn create_processor(&self, _node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(WavetableProcessor::new(sample_rate)))
    }
    fn name(&self) -> &'static str { "Wavetable" }
}

pub struct ModulationFactory;
impl ProcessorFactory for ModulationFactory {
    fn create_processor(&self, _node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(ModulationProcessor::new(0, 0, 1.0, 0.0)))
    }
    fn name(&self) -> &'static str { "Modulation" }
}

pub struct SequencerFactory;
impl ProcessorFactory for SequencerFactory {
    fn create_processor(&self, _node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SequencerProcessor::new(sample_rate, 120.0)))
    }
    fn name(&self) -> &'static str { "Sequencer" }
}
