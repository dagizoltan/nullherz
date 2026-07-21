use nullherz_traits::{AudioProcessor, ProcessorFactory, ProcessorTypeId, ProcessorCapability};
use crate::gain::*;
use crate::delay::*;
use crate::compressor::*;
use crate::stereo_utility::*;
use crate::analysis::*;
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
use crate::limiter::*;
use crate::streaming_sampler::*;

pub struct GainFactory;
impl ProcessorFactory for GainFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(GainProcessor::new(node_idx as u64, 1.0)))
    }
    fn name(&self) -> &'static str { "Gain" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::GAIN }
}

pub struct DelayFactory;
impl ProcessorFactory for DelayFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(DelayProcessor::new(node_idx as u64, 44100))) // 1s max delay
    }
    fn name(&self) -> &'static str { "Delay" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::DELAY }
}

pub struct BiquadFactory;
impl ProcessorFactory for BiquadFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        // Identity (bypass) by default: a freshly created filter must be
        // sonically neutral until parameters arrive. The old arbitrary
        // lowpass {0.1,0.2,0.1,-0.5,0.2} silently cost ~5dB and treble at
        // EVERY biquad in the graph (deck filter, FX slot, master EQ).
        let coeffs = audio_dsp::BiquadCoefficients {
            b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0
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
        // One kernel per channel: the isolator's crossover biquads hold filter
        // state, and a stereo strip must not run L and R through one state.
        Some(Box::new(crate::dsp_kernel_processor::MultiChannelDspProcessor::new(node_idx as u64, audio_dsp::DjIsolator::new(), 2)))
    }
    fn name(&self) -> &'static str { "DjIsolator" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::DJ_ISOLATOR }
}

pub struct MasteringEqFactory;
impl ProcessorFactory for MasteringEqFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        // One kernel per channel: the shelf/peak biquads hold filter state,
        // and the stereo master must not run L and R through one state.
        Some(Box::new(crate::dsp_kernel_processor::MultiChannelDspProcessor::new(
            node_idx as u64,
            audio_dsp::MasteringEq::with_sample_rate(sample_rate),
            2,
        )))
    }
    fn name(&self) -> &'static str { "MasteringEq" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::MASTERING_EQ }
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

pub struct CompressorFactory;
impl ProcessorFactory for CompressorFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(CompressorProcessor::new(node_idx as u64, sample_rate)))
    }
    fn name(&self) -> &'static str { "Compressor" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId(170) }
}

pub struct StereoUtilityFactory;
impl ProcessorFactory for StereoUtilityFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(StereoUtilityProcessor::new(node_idx as u64)))
    }
    fn name(&self) -> &'static str { "StereoUtility" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId(160) }
}

pub struct AnalysisFactory;
impl ProcessorFactory for AnalysisFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(AnalysisProcessor::new(node_idx as u64)))
    }
    fn name(&self) -> &'static str { "Analysis" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId(180) }
}

pub struct DnaMorphFactory;
impl ProcessorFactory for DnaMorphFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(DnaMorpher::new(node_idx as u64, 1024)))
    }
    fn name(&self) -> &'static str { "DnaMorph" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::DNA_MORPH }
}

pub struct LimiterFactory;
impl ProcessorFactory for LimiterFactory {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        Some(Box::new(LimiterProcessor::new(node_idx as u64, sample_rate)))
    }
    fn name(&self) -> &'static str { "Limiter" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::LIMITER }
}

pub struct StreamingSamplerFactory;
impl ProcessorFactory for StreamingSamplerFactory {
    fn create_processor(&self, node_idx: u32, _sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        // Initialize with a mock backing ShmRingBuffer for factory creation compatibility
        let capacity = 1024;
        let (layout, _) = ipc_layer::ShmRingBuffer::<f32>::layout(capacity);
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() { return None; }
        let rb_ptr = unsafe { ipc_layer::ShmRingBuffer::<f32>::init(ptr, capacity) };
        let mut sampler = StreamingSamplerProcessor::new(node_idx as u64, rb_ptr);
        sampler._shm_holder = Some(unsafe { Vec::from_raw_parts(ptr, layout.size(), layout.size()) });
        Some(Box::new(sampler))
    }
    fn name(&self) -> &'static str { "StreamingSampler" }
    fn type_id(&self) -> ProcessorTypeId { ProcessorTypeId::STREAMING_SAMPLER }
}
