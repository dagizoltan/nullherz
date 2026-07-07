use std::path::Path;
use hound::{WavWriter, WavSpec, SampleFormat};

/// Utility for persisting audio buffers to WAV files.
pub struct WavPersistence;

impl WavPersistence {
    /// Saves a stereo interleaved buffer to a WAV file.
    pub fn save_stereo(path: &str, buffer: &[f32], sample_rate: u32) -> Result<(), Box<dyn std::error::Error>> {
        let parent = Path::new(path).parent().ok_or("Invalid path")?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }

        let spec = WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };

        let mut writer = WavWriter::create(path, spec)?;
        for &sample in buffer {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        Ok(())
    }

    /// Saves a mono buffer to a WAV file.
    pub fn save_mono(path: &str, buffer: &[f32], sample_rate: u32) -> Result<(), Box<dyn std::error::Error>> {
        let parent = Path::new(path).parent().ok_or("Invalid path")?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }

        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };

        let mut writer = WavWriter::create(path, spec)?;
        for &sample in buffer {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        Ok(())
    }
}
