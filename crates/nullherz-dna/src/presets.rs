use serde::{Deserialize, Serialize};

/// Per-pad configuration for a sampler-based drum kit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SampleDrumPadPreset {
    pub pad_index: u8, // 0..15
    pub name: String,
    pub sample_id: Option<u64>,
    pub sample_path: Option<String>,
    pub start_crop: f32, // 0.0 .. 1.0
    pub end_crop: f32,   // 0.0 .. 1.0
    pub pitch_semitones: f32, // -24.0 .. +24.0
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub transient_boost: f32, // 0.0 .. 4.0
    pub choke_group: u8,      // 0 = none, 1..16
    pub output_channel: u8,   // 0..15
}

impl Default for SampleDrumPadPreset {
    fn default() -> Self {
        Self {
            pad_index: 0,
            name: "Sample Pad".to_string(),
            sample_id: None,
            sample_path: None,
            start_crop: 0.0,
            end_crop: 1.0,
            pitch_semitones: 0.0,
            attack_ms: 1.0,
            decay_ms: 200.0,
            sustain: 0.0,
            release_ms: 50.0,
            transient_boost: 1.0,
            choke_group: 0,
            output_channel: 0,
        }
    }
}

/// Full preset for a sampler drum kit (16 pads)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SampleDrumKitPreset {
    pub preset_id: String,
    pub name: String,
    pub author: String,
    pub pads: Vec<SampleDrumPadPreset>,
}

impl Default for SampleDrumKitPreset {
    fn default() -> Self {
        let pads = (0..16)
            .map(|i| SampleDrumPadPreset {
                pad_index: i as u8,
                name: format!("Pad {}", i + 1),
                output_channel: i as u8,
                ..SampleDrumPadPreset::default()
            })
            .collect();

        Self {
            preset_id: "default_sample_kit".to_string(),
            name: "Default Sample Kit".to_string(),
            author: "Nullherz Engine".to_string(),
            pads,
        }
    }
}

/// Engine type for synthesizer-based drum voice
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub enum SynthDrumEngineType {
    Kick,
    Snare,
    HiHat,
    TomPerc,
}

impl Default for SynthDrumEngineType {
    fn default() -> Self {
        Self::Kick
    }
}

/// Per-pad configuration for a synth-based drum voice
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SynthDrumPadPreset {
    pub pad_index: u8,
    pub name: String,
    pub engine_type: SynthDrumEngineType,
    pub start_pitch_hz: f32,
    pub end_pitch_hz: f32,
    pub pitch_decay_ms: f32,
    pub amp_decay_ms: f32,
    pub noise_blend: f32,
    pub filter_cutoff_hz: f32,
    pub filter_resonance: f32,
    pub drive_saturation: f32,
    pub click_intensity: f32,
    pub choke_group: u8,
    pub output_channel: u8,
}

impl Default for SynthDrumPadPreset {
    fn default() -> Self {
        Self {
            pad_index: 0,
            name: "Kick Synth".to_string(),
            engine_type: SynthDrumEngineType::Kick,
            start_pitch_hz: 150.0,
            end_pitch_hz: 45.0,
            pitch_decay_ms: 35.0,
            amp_decay_ms: 350.0,
            noise_blend: 0.1,
            filter_cutoff_hz: 8000.0,
            filter_resonance: 0.707,
            drive_saturation: 1.2,
            click_intensity: 0.8,
            choke_group: 0,
            output_channel: 0,
        }
    }
}

/// Full preset for a synthesizer drum kit (16 pads)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SynthDrumKitPreset {
    pub preset_id: String,
    pub name: String,
    pub author: String,
    pub pads: Vec<SynthDrumPadPreset>,
}

impl Default for SynthDrumKitPreset {
    fn default() -> Self {
        let mut pads = Vec::with_capacity(16);
        for i in 0..16 {
            let (name, engine, start_p, end_p, choke) = match i {
                0 => ("Analog Kick", SynthDrumEngineType::Kick, 150.0, 45.0, 0),
                1 => ("Snare Drum", SynthDrumEngineType::Snare, 220.0, 80.0, 0),
                2 => ("Closed HiHat", SynthDrumEngineType::HiHat, 8000.0, 8000.0, 1),
                3 => ("Open HiHat", SynthDrumEngineType::HiHat, 7000.0, 7000.0, 1),
                4 => ("Low Tom", SynthDrumEngineType::TomPerc, 120.0, 60.0, 0),
                5 => ("Mid Tom", SynthDrumEngineType::TomPerc, 180.0, 90.0, 0),
                6 => ("High Tom", SynthDrumEngineType::TomPerc, 240.0, 120.0, 0),
                7 => ("Percussion", SynthDrumEngineType::TomPerc, 400.0, 200.0, 0),
                _ => ("Synth Pad", SynthDrumEngineType::TomPerc, 300.0, 150.0, 0),
            };
            pads.push(SynthDrumPadPreset {
                pad_index: i as u8,
                name: name.to_string(),
                engine_type: engine,
                start_pitch_hz: start_p,
                end_pitch_hz: end_p,
                choke_group: choke,
                output_channel: i as u8,
                ..SynthDrumPadPreset::default()
            });
        }

        Self {
            preset_id: "default_synth_kit".to_string(),
            name: "Default Analog Synth Kit".to_string(),
            author: "Nullherz Engine".to_string(),
            pads,
        }
    }
}

/// Full preset for a neural network-driven drum kit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct NeuralDrumKitPreset {
    pub preset_id: String,
    pub name: String,
    pub author: String,
    pub latent_coords: Vec<[f32; 16]>, // 16-D continuous coordinates per pad (16 pads)
    pub cortical_temp: f32,             // Neural spiking temperature
    pub shell_warmth: f32,              // TCN / SSM shell shaping drive
    pub sound_dna: nullherz_traits::SoundDNA,
}

impl Default for NeuralDrumKitPreset {
    fn default() -> Self {
        let latent_coords = (0..16)
            .map(|i| {
                let mut v = [0.0f32; 16];
                v[i % 16] = 1.0;
                v[(i + 1) % 16] = 0.5;
                v
            })
            .collect();

        Self {
            preset_id: "default_neural_kit".to_string(),
            name: "Default Cortical Neural Kit".to_string(),
            author: "Nullherz Engine".to_string(),
            latent_coords,
            cortical_temp: 0.7,
            shell_warmth: 1.5,
            sound_dna: nullherz_traits::SoundDNA::default(),
        }
    }
}

/// Morphing and interpolation utilities for drum kit presets using SoundDNA
pub fn morph_sample_presets(preset_a: &SampleDrumKitPreset, preset_b: &SampleDrumKitPreset, bias: f32) -> SampleDrumKitPreset {
    let b = bias.clamp(0.0, 1.0);
    let inv_b = 1.0 - b;

    let mut morphed_pads = Vec::with_capacity(preset_a.pads.len().max(preset_b.pads.len()));
    let count = preset_a.pads.len().min(preset_b.pads.len());

    for i in 0..count {
        let pa = &preset_a.pads[i];
        let pb = &preset_b.pads[i];

        morphed_pads.push(SampleDrumPadPreset {
            pad_index: pa.pad_index,
            name: if b < 0.5 { pa.name.clone() } else { pb.name.clone() },
            sample_id: if b < 0.5 { pa.sample_id } else { pb.sample_id },
            sample_path: if b < 0.5 { pa.sample_path.clone() } else { pb.sample_path.clone() },
            start_crop: pa.start_crop * inv_b + pb.start_crop * b,
            end_crop: pa.end_crop * inv_b + pb.end_crop * b,
            pitch_semitones: pa.pitch_semitones * inv_b + pb.pitch_semitones * b,
            attack_ms: pa.attack_ms * inv_b + pb.attack_ms * b,
            decay_ms: pa.decay_ms * inv_b + pb.decay_ms * b,
            sustain: pa.sustain * inv_b + pb.sustain * b,
            release_ms: pa.release_ms * inv_b + pb.release_ms * b,
            transient_boost: pa.transient_boost * inv_b + pb.transient_boost * b,
            choke_group: if b < 0.5 { pa.choke_group } else { pb.choke_group },
            output_channel: if b < 0.5 { pa.output_channel } else { pb.output_channel },
        });
    }

    SampleDrumKitPreset {
        preset_id: format!("morphed_{}_{}", preset_a.preset_id, preset_b.preset_id),
        name: format!("Morphed Kit ({:.0}% B)", b * 100.0),
        author: "Nullherz Transfusion".to_string(),
        pads: morphed_pads,
    }
}

pub fn morph_synth_presets(preset_a: &SynthDrumKitPreset, preset_b: &SynthDrumKitPreset, bias: f32) -> SynthDrumKitPreset {
    let b = bias.clamp(0.0, 1.0);
    let inv_b = 1.0 - b;

    let mut morphed_pads = Vec::with_capacity(preset_a.pads.len().max(preset_b.pads.len()));
    let count = preset_a.pads.len().min(preset_b.pads.len());

    for i in 0..count {
        let pa = &preset_a.pads[i];
        let pb = &preset_b.pads[i];

        morphed_pads.push(SynthDrumPadPreset {
            pad_index: pa.pad_index,
            name: if b < 0.5 { pa.name.clone() } else { pb.name.clone() },
            engine_type: if b < 0.5 { pa.engine_type } else { pb.engine_type },
            start_pitch_hz: pa.start_pitch_hz * inv_b + pb.start_pitch_hz * b,
            end_pitch_hz: pa.end_pitch_hz * inv_b + pb.end_pitch_hz * b,
            pitch_decay_ms: pa.pitch_decay_ms * inv_b + pb.pitch_decay_ms * b,
            amp_decay_ms: pa.amp_decay_ms * inv_b + pb.amp_decay_ms * b,
            noise_blend: pa.noise_blend * inv_b + pb.noise_blend * b,
            filter_cutoff_hz: pa.filter_cutoff_hz * inv_b + pb.filter_cutoff_hz * b,
            filter_resonance: pa.filter_resonance * inv_b + pb.filter_resonance * b,
            drive_saturation: pa.drive_saturation * inv_b + pb.drive_saturation * b,
            click_intensity: pa.click_intensity * inv_b + pb.click_intensity * b,
            choke_group: if b < 0.5 { pa.choke_group } else { pb.choke_group },
            output_channel: if b < 0.5 { pa.output_channel } else { pb.output_channel },
        });
    }

    SynthDrumKitPreset {
        preset_id: format!("morphed_{}_{}", preset_a.preset_id, preset_b.preset_id),
        name: format!("Morphed Synth Kit ({:.0}% B)", b * 100.0),
        author: "Nullherz Transfusion".to_string(),
        pads: morphed_pads,
    }
}

pub fn morph_neural_presets(preset_a: &NeuralDrumKitPreset, preset_b: &NeuralDrumKitPreset, bias: f32) -> NeuralDrumKitPreset {
    let b = bias.clamp(0.0, 1.0);
    let inv_b = 1.0 - b;

    let count = preset_a.latent_coords.len().min(preset_b.latent_coords.len());
    let mut morphed_coords = Vec::with_capacity(count);

    for i in 0..count {
        let mut coord = [0.0f32; 16];
        for j in 0..16 {
            coord[j] = preset_a.latent_coords[i][j] * inv_b + preset_b.latent_coords[i][j] * b;
        }
        morphed_coords.push(coord);
    }

    let sound_dna = crate::transfuse_dna(&preset_a.sound_dna, &preset_b.sound_dna, b);

    NeuralDrumKitPreset {
        preset_id: format!("morphed_{}_{}", preset_a.preset_id, preset_b.preset_id),
        name: format!("Morphed Neural Kit ({:.0}% B)", b * 100.0),
        author: "Nullherz Transfusion".to_string(),
        latent_coords: morphed_coords,
        cortical_temp: preset_a.cortical_temp * inv_b + preset_b.cortical_temp * b,
        shell_warmth: preset_a.shell_warmth * inv_b + preset_b.shell_warmth * b,
        sound_dna,
    }
}
