use nullherz_traits::AudioProcessor;
use crate::standard::*;
use crate::complex::*;
use crate::sampler::*;

use nullherz_traits::ProcessorType;

pub fn create_processor(type_id: u32, node_idx: u32, sample_rate: f32) -> Box<dyn AudioProcessor> {
    let p_type: ProcessorType = unsafe { std::mem::transmute(type_id) };
    match p_type {
        ProcessorType::Biquad => {
            // Default Low-pass-ish coeffs
            let coeffs = audio_dsp::BiquadCoefficients {
                b0: 0.1, b1: 0.2, b2: 0.1, a1: -0.5, a2: 0.2
            };
            Box::new(BiquadProcessor::new(node_idx as u64, coeffs))
        }
        ProcessorType::Gain => Box::new(GainProcessor::new(node_idx as u64, 1.0)),
        ProcessorType::Sampler => Box::new(SamplerProcessor::new(node_idx as u64)),
        ProcessorType::BiquadEQ => {
            // Default EQ-ish coeffs
            let coeffs = audio_dsp::BiquadCoefficients {
                b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0
            };
            Box::new(BiquadProcessor::new(node_idx as u64, coeffs))
        }
        ProcessorType::Crossfader => Box::new(CrossfaderProcessor::new()),
        ProcessorType::Summing => Box::new(SummingProcessor::new()),
        ProcessorType::Spectral => Box::new(SpectralProcessor::new(1024)),
        ProcessorType::Wavetable => Box::new(WavetableProcessor::new(sample_rate)),
    }
}
