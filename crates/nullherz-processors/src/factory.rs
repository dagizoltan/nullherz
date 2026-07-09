use nullherz_traits::{AudioProcessor, ProcessorFactory, ProcessorTypeId, ProcessorCapability};
use crate::gain::*;
use crate::biquad::*;
use crate::crossfader::*;
use crate::summing::*;
use crate::wavetable::*;
use crate::spectral::*;
use crate::sampler::*;
use crate::modulation::*;
use crate::sequencer::*;
use crate::transfusion::*;
use crate::keysync::*;

pub struct GainFactory;
impl ProcessorFactory for GainFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(GainProcessor::new(node_idx as u64, 1.0)))
    }
    fn name(&self) -> &'static str { "Gain" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::GAIN }
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
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::BIQUAD }
}

pub struct SamplerFactory;
impl ProcessorFactory for SamplerFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SamplerProcessor::new(node_idx as u64)))
    }
    fn name(&self) -> &'static str { "Sampler" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::SAMPLER }
    fn capabilities(&self) -> ProcessorCapability { ProcessorCapability { has_midi_input: true, ..ProcessorCapability::default() } }
}

pub struct CrossfaderFactory;
impl ProcessorFactory for CrossfaderFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(CrossfaderProcessor::new(node_idx as u64)))
    }
    fn name(&self) -> &'static str { "Crossfader" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::CROSSFADER }
}

pub struct SummingFactory;
impl ProcessorFactory for SummingFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SummingProcessor::new(node_idx as u64)))
    }
    fn name(&self) -> &'static str { "Summing" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::SUMMING }
}

pub struct SpectralFactory;
impl ProcessorFactory for SpectralFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SpectralProcessor::new(node_idx as u64, 1024)))
    }
    fn name(&self) -> &'static str { "Spectral" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::SPECTRAL }
}

pub struct WavetableFactory;
impl ProcessorFactory for WavetableFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(WavetableProcessor::new(node_idx as u64, sample_rate)))
    }
    fn name(&self) -> &'static str { "Wavetable" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::WAVETABLE }
    fn capabilities(&self) -> ProcessorCapability { ProcessorCapability { is_instrument: true, has_midi_input: true, has_audio_input: false, ..ProcessorCapability::default() } }
}

pub struct ModulationFactory;
impl ProcessorFactory for ModulationFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(ModulationProcessor::new(node_idx as u64, 0, 0, 1.0, 0.0)))
    }
    fn name(&self) -> &'static str { "Modulation" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::MODULATION }
}

pub struct SequencerFactory;
impl ProcessorFactory for SequencerFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SequencerProcessor::new(node_idx, sample_rate, 120.0)))
    }
    fn name(&self) -> &'static str { "Sequencer" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::SEQUENCER }
    fn capabilities(&self) -> ProcessorCapability { ProcessorCapability { is_instrument: true, has_audio_input: false, ..ProcessorCapability::default() } }
}

pub struct EnvelopeFollowerFactory;
impl ProcessorFactory for EnvelopeFollowerFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(EnvelopeFollowerProcessor::new(node_idx as u64, sample_rate)))
    }
    fn name(&self) -> &'static str { "EnvelopeFollower" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::ENVELOPE_FOLLOWER }
}

pub struct GranularFactory;
impl ProcessorFactory for GranularFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(GranularProcessor::new(node_idx as u64, sample_rate)))
    }
    fn name(&self) -> &'static str { "Granular" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::GRANULAR }
}

pub struct SpectralMorphFactory;
impl ProcessorFactory for SpectralMorphFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(SpectralMorphProcessor::new(node_idx as u64, 1024)))
    }
    fn name(&self) -> &'static str { "SpectralMorph" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::SPECTRAL_MORPH }
}

pub struct CaptureFactory;
impl ProcessorFactory for CaptureFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(CaptureProcessor::new(sample_rate as usize * 2, node_idx as u64))) // 2 seconds
    }
    fn name(&self) -> &'static str { "Capture" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::CAPTURE }
}

pub struct DjIsolatorFactory;
impl ProcessorFactory for DjIsolatorFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(crate::dsp_kernel_processor::DspKernelProcessor::new(node_idx as u64, audio_dsp::DjIsolator::new())))
    }
    fn name(&self) -> &'static str { "DjIsolator" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::DJ_ISOLATOR }
}

pub struct SimdBiquadFactory;
impl ProcessorFactory for SimdBiquadFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        let coeffs = audio_dsp::BiquadCoefficients::default();
        Some(Box::new(crate::dsp_kernel_processor::MultiChannelDspProcessor::new(node_idx as u64, audio_dsp::SimdBiquad::new(coeffs), 8)))
    }
    fn name(&self) -> &'static str { "SimdBiquad" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::SIMD_BIQUAD }
}

pub struct KeySyncFactory;
impl ProcessorFactory for KeySyncFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(KeySyncProcessor::new(node_idx as u64, 1024)))
    }
    fn name(&self) -> &'static str { "KeySync" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::KEY_SYNC }
}

pub struct PersonalityInheritanceFactory;
impl ProcessorFactory for PersonalityInheritanceFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(PersonalityInheritanceProcessor::new(node_idx as u64, 1024)))
    }
    fn name(&self) -> &'static str { "PersonalityInheritance" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::PERSONALITY_INHERITANCE }
}
