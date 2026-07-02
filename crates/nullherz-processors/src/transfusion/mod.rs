pub mod envelope_follower;
pub mod granular;
pub mod spectral_morph;
pub mod capture;
pub mod personality_inheritance;

pub use envelope_follower::EnvelopeFollowerProcessor;
pub use granular::GranularProcessor;
pub use spectral_morph::SpectralMorphProcessor;
pub use capture::CaptureProcessor;
pub use personality_inheritance::PersonalityInheritanceProcessor;
