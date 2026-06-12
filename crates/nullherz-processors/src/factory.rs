use nullherz_traits::AudioProcessor;
use crate::standard::*;
use crate::complex::*;
use crate::sampler::*;

pub fn create_processor(type_id: u32, node_idx: u32, sample_rate: f32) -> Box<dyn AudioProcessor> {
    match type_id {
        1 => {
            // Default Low-pass-ish coeffs
            let coeffs = audio_dsp::BiquadCoefficients {
                b0: 0.1, b1: 0.2, b2: 0.1, a1: -0.5, a2: 0.2
            };
            Box::new(BiquadProcessor::new(node_idx as u64, coeffs))
        }
        2 => Box::new(GainProcessor::new(node_idx as u64, 1.0)),
        10 => Box::new(SamplerProcessor::new(node_idx as u64)),
        11 => {
            // Default EQ-ish coeffs
            let coeffs = audio_dsp::BiquadCoefficients {
                b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0
            };
            Box::new(BiquadProcessor::new(node_idx as u64, coeffs))
        }
        20 => Box::new(CrossfaderProcessor::new()),
        30 => Box::new(SummingProcessor::new()),
        40 => Box::new(SpectralProcessor::new(1024)),
        50 => Box::new(WavetableProcessor::new(sample_rate)),
        _ => Box::new(crate::standard::GainProcessor::new(node_idx as u64, 1.0)),
    }
}
