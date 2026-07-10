pub mod sidecar;
pub mod delay;
pub mod compressor;
pub mod stereo_utility;
pub mod analysis;
pub mod streaming_sampler;
pub mod sampler;
pub mod factory;
pub mod registry;
pub mod gain;
pub mod biquad;
pub mod crossfader;
pub mod summing;
pub mod wavetable;
pub mod spectral;
pub mod keysync;
pub mod dsp_kernel_processor;
pub mod modulation;
pub mod sequencer;
pub mod transfusion;
pub mod fallback;
#[cfg(test)]
mod sampler_tests;
#[cfg(test)]
mod sampler_slicer_tests;
#[cfg(feature = "test-utils")]
pub mod test_kit;

pub use nullherz_traits::{MAX_CHANNELS, MAX_NODES};

pub use sidecar::SidecarProcessor;
pub use delay::DelayProcessor;
pub use compressor::CompressorProcessor;
pub use stereo_utility::StereoUtilityProcessor;
pub use analysis::AnalysisProcessor;
pub use streaming_sampler::StreamingSamplerProcessor;
pub use sampler::SamplerProcessor;
pub use gain::GainProcessor;
pub use biquad::{BiquadProcessor, SimdBiquadProcessor};
pub use crossfader::CrossfaderProcessor;
pub use summing::SummingProcessor;
pub use wavetable::WavetableProcessor;
pub use spectral::SpectralProcessor;
pub use modulation::ModulationProcessor;
pub use sequencer::SequencerProcessor;
pub use transfusion::*;
pub use fallback::FallbackProcessor;
pub use registry::ProcessorRegistry;

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::test_kit::ConformanceSuite;

    #[test]
    fn test_gain_parameter_bounds() {
        let mut gain = GainProcessor::new(0, 1.0);
        ConformanceSuite::verify_parameter_bounds(&mut gain, 0).expect("Gain failed parameter bounds check");
    }

    #[test]
    fn test_capture_snapshot_safety() {
        let mut capture = CaptureProcessor::new(1024, 0);
        ConformanceSuite::verify_snapshot_safety(&mut capture).expect("Capture failed snapshot safety check");
    }
}
