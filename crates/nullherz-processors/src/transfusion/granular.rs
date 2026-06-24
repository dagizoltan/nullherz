use std::sync::Arc;
use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata};
use audio_dsp::{SamplerVoice, InterpolationType};

const MAX_GRAINS: usize = 32;
const MAX_SOURCES: usize = 16;
const WINDOW_LUT_SIZE: usize = 1024;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowShape {
    Hann = 0,
    Triangle = 1,
    Square = 2,
}

pub struct GranularProcessor {
    pub id: u64,
    voices: Vec<SamplerVoice>,
    voice_ages: [u32; MAX_GRAINS],
    voice_durations: [u32; MAX_GRAINS],
    pub source_pool: [Option<Arc<Vec<f32>>>; MAX_SOURCES],
    pub source_count: usize,
    render_buffer: [f32; ipc_layer::MAX_BLOCK_SIZE],
    grain_buffer: [f32; ipc_layer::MAX_BLOCK_SIZE],

    // Parameters
    density: f32,
    grain_duration_ms: f32,
    pos_jitter: f32,
    pitch_jitter: f32,
    window_shape: WindowShape,
    interpolation: InterpolationType,

    next_grain_samples: f32,
    sample_rate: f32,
    rng_state: u64,
    hann_lut: [f32; WINDOW_LUT_SIZE],
}

impl GranularProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        let voices = (0..MAX_GRAINS).map(|_| SamplerVoice::new()).collect();

        let mut hann_lut = [0.0f32; WINDOW_LUT_SIZE];
        for i in 0..WINDOW_LUT_SIZE {
            hann_lut[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (WINDOW_LUT_SIZE - 1) as f32).cos());
        }

        Self {
            id,
            voices,
            voice_ages: [0; MAX_GRAINS],
            voice_durations: [0; MAX_GRAINS],
            source_pool: std::array::from_fn(|_| None),
            source_count: 0,
            render_buffer: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            grain_buffer: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            density: 20.0,
            grain_duration_ms: 100.0,
            pos_jitter: 0.1,
            pitch_jitter: 0.05,
            window_shape: WindowShape::Hann,
            interpolation: InterpolationType::Lagrange,
            next_grain_samples: 0.0,
            sample_rate,
            rng_state: 12345,
            hann_lut,
        }
    }

    fn next_rand(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.rng_state >> 32) as f32 / 4294967296.0
    }

    pub fn add_source(&mut self, source: Arc<Vec<f32>>) {
        if self.source_count < MAX_SOURCES {
            self.source_pool[self.source_count] = Some(source);
            self.source_count += 1;
        } else {
            // FIFO replacement if full? Or just ignore.
            // For Transfusion, we'll shift and replace the oldest.
            self.source_pool.rotate_left(1);
            self.source_pool[MAX_SOURCES - 1] = Some(source);
        }
    }

    fn get_window(&self, phase: f32) -> f32 {
        match self.window_shape {
            WindowShape::Hann => {
                let idx = (phase * (WINDOW_LUT_SIZE - 1) as f32) as usize;
                self.hann_lut[idx.min(WINDOW_LUT_SIZE - 1)]
            }
            WindowShape::Triangle => {
                1.0 - (2.0 * phase - 1.0).abs()
            }
            WindowShape::Square => 1.0,
        }
    }
}

impl nullherz_traits::SignalProcessor for GranularProcessor {
fn reset(&mut self) {
        for v in self.voices.iter_mut() {
            v.is_active = false;
        }
        self.voice_ages.fill(0);
    }
fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len();
        let num_samples = num_samples.min(ipc_layer::MAX_BLOCK_SIZE);

        if self.source_count == 0 {
            for output in outputs.iter_mut() {
                output.fill(0.0);
            }
            return;
        }

        self.render_buffer[..num_samples].fill(0.0);

        let mut samples_to_process = num_samples;
        let mut current_offset = 0;

        while samples_to_process > 0 {
            if self.next_grain_samples <= 0.0 {
                if let Some(idx) = self.voices.iter().position(|v| !v.is_active) {
                    let r_src = self.next_rand();
                    let r_pos = self.next_rand();
                    let r_pitch = self.next_rand();

                    let source_idx = (r_src * self.source_count as f32) as usize % self.source_count;
                    let source = self.source_pool[source_idx].as_ref().unwrap().clone();

                    let base_pos = r_pos * (source.len() as f32);
                    let pos_jitter_offset = (self.next_rand() * 2.0 - 1.0) * self.pos_jitter * self.sample_rate;
                    let start_pos = (base_pos + pos_jitter_offset).clamp(0.0, (source.len() as f32 - 10.0).max(0.0));

                    let pitch_jitter_val = (r_pitch * 2.0 - 1.0) * self.pitch_jitter;
                    let playback_rate = (1.0 + pitch_jitter_val).max(0.01);

                    let duration_samples = (self.grain_duration_ms * 0.001 * self.sample_rate) as u32;

                    self.voices[idx].trigger(source, playback_rate, 1.0);
                    self.voices[idx].play_head = start_pos;
                    self.voices[idx].interpolation = self.interpolation;
                    self.voice_ages[idx] = 0;
                    self.voice_durations[idx] = duration_samples;
                }

                self.next_grain_samples = (1.0 / self.density.max(0.1)) * self.sample_rate;
            }

            let chunk = (samples_to_process as f32).min(self.next_grain_samples).ceil() as usize;
            let chunk = chunk.min(samples_to_process);

            for i in 0..MAX_GRAINS {
                if !self.voices[i].is_active { continue; }

                self.grain_buffer[..chunk].fill(0.0);
                self.voices[i].process_block(&mut self.grain_buffer[..chunk]);

                let duration = self.voice_durations[i];
                let initial_age = self.voice_ages[i];

                for j in 0..chunk {
                    let age = initial_age + j as u32;
                    if age >= duration {
                        self.voices[i].is_active = false;
                    } else {
                        let phase = age as f32 / duration as f32;
                        let win = self.get_window(phase);
                        self.render_buffer[current_offset + j] += self.grain_buffer[j] * win;
                    }
                }
                self.voice_ages[i] += chunk as u32;
            }

            self.next_grain_samples -= chunk as f32;
            samples_to_process -= chunk;
            current_offset += chunk;
        }

        for output in outputs.iter_mut() {
            output.copy_from_slice(&self.render_buffer[..num_samples]);
        }
    }
fn latency_samples(&self) -> usize {
        // Granular doesn't have inherent block latency like spectral,
        // but it might have sub-block scheduling latency.
        0
    }
}

impl nullherz_traits::MidiResponder for GranularProcessor { }

impl nullherz_traits::SnapshotProvider for GranularProcessor { }

impl AudioProcessor for GranularProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = *command {
            if target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
        }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => self.density = value.clamp(0.1, 500.0),
            1 => self.grain_duration_ms = value.clamp(1.0, 2000.0),
            2 => self.pos_jitter = value.clamp(0.0, 1.0),
            3 => self.pitch_jitter = value.clamp(0.0, 1.0),
            4 => {
                self.window_shape = match value as u32 {
                    0 => WindowShape::Hann,
                    1 => WindowShape::Triangle,
                    2 => WindowShape::Square,
                    _ => WindowShape::Hann,
                };
            }
            5 => {
                self.interpolation = match value as u32 {
                    0 => InterpolationType::Linear,
                    1 => InterpolationType::Lagrange,
                    _ => InterpolationType::Lagrange,
                };
            }
            _ => {}
        }
    }
fn apply_topology_mutation(&mut self, mutation: nullherz_traits::TopologyMutation) {
        if let nullherz_traits::TopologyMutation::AddSource { node_idx: _, buffer, sample_id: _ } = mutation {
            self.add_source(buffer);
        }
    }
fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.1,
            max: 500.0,
            default: 20.0,
        }; 16];

        let names = [
            (0, "Density", 0.1, 500.0, 20.0),
            (1, "Duration", 1.0, 2000.0, 100.0),
            (2, "Pos Jitter", 0.0, 1.0, 0.1),
            (3, "Pitch Jitter", 0.0, 1.0, 0.05),
            (4, "Window", 0.0, 2.0, 0.0),
            (5, "Quality", 0.0, 1.0, 1.0),
        ];

        for (i, (id, name, min, max, default)) in names.iter().enumerate() {
            parameters[i].id = *id;
            parameters[i].min = *min;
            parameters[i].max = *max;
            parameters[i].default = *default;
            let name_bytes = name.as_bytes();
            parameters[i].name[..name_bytes.len()].copy_from_slice(name_bytes);
        }

        Some(ProcessorMetadata {
            processor_id: 0,
            num_parameters: names.len() as u32,
            parameters,
        })
    }
}
