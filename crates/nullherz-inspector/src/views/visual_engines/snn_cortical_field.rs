//! Engine 8: snn_cortical_field (Spiking Cortical Membrane Field Engine)
//! 2D Leaky Integrate-and-Fire (LIF) spiking neuron grid coupled with lateral spatial excitation,
//! dynamic refractory hyperpolarization, and synesthetic optical bloom flashes driven by DAW FFT spectrum.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[allow(dead_code)]
pub const SNN_FIELD_WGSL_SHADER: &str = r#"
struct SnnUniforms {
    spike_density: f32,
    avg_voltage: f32,
    time: f32,
    audio_energy: f32,
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u: SnnUniforms;

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (frag_coord.xy - 0.5 * u.resolution) / u.resolution.y;
    let r = length(uv);
    let angle = atan2(uv.y, uv.x);

    // SNN Membrane Potential Wave Fronts
    let wave = sin(r * 30.0 - u.time * 4.0 + u.avg_voltage * 10.0);

    // Spike Flash Transient (Driven directly by SNN Firing Ratio)
    let spike_glow = u.spike_density * 4.0 / (r + 0.1);

    // Organic Synesthetic Color Palette Modulated by SNN Activity
    let col = vec3<f32>(
        sin(angle * 6.0 + u.time + u.avg_voltage) * 0.5 + 0.5,
        cos(angle * 4.0 - u.time * 0.5) * 0.5 + 0.5,
        sin(r * 12.0 + u.spike_density * 8.0) * 0.5 + 0.5
    ) * (wave * 0.5 + 0.5) + vec3<f32>(spike_glow);

    return vec4<f32>(col, 1.0);
}
"#;

#[derive(Clone, Debug)]
pub struct SnnCorticalFieldEngine {
    pub grid_width: usize,
    pub grid_height: usize,
    pub voltage: Vec<f32>,              // Membrane potential (V)
    pub threshold: Vec<f32>,            // Dynamic spike threshold
    pub spikes: Vec<bool>,              // Firing state for current frame
    pub decay: f32,                     // Potential leakage retention rate (e.g., 0.88)
    pub lateral_coupling: f32,          // Neighbor stimulus excitation factor
    pub hyperpolarization_reset: f32,   // Post-spike refractory reset voltage (-0.2)
    pub axonal_wave: Vec<f32>,          // Smoothed wave display field
    pub spike_flash: f32,               // Global spike flash bloom accumulator
}

impl SnnCorticalFieldEngine {
    pub fn new() -> Self {
        let grid_width = 32;
        let grid_height = 32;
        let count = grid_width * grid_height;
        Self {
            grid_width,
            grid_height,
            voltage: vec![0.0; count],
            threshold: vec![1.0; count],
            spikes: vec![false; count],
            decay: 0.88,
            lateral_coupling: 0.18,
            hyperpolarization_reset: -0.2,
            axonal_wave: vec![0.0; count],
            spike_flash: 0.0,
        }
    }

    /// Step the 2D LIF Spiking Grid with incoming DAW audio spectrum & energy
    pub fn step(&mut self, audio_fft: &[f32], dt: f32) {
        let count = self.grid_width * self.grid_height;
        if audio_fft.is_empty() || count == 0 {
            return;
        }

        // Decay previous spike flash
        self.spike_flash = (self.spike_flash * 0.85).max(0.0);

        // Copy previous spikes to evaluate lateral neighbor stimulus
        let prev_spikes = self.spikes.clone();

        for i in 0..count {
            // 1. Map DAW audio frequency bins to neuron inputs
            let fft_bin = i % audio_fft.len();
            let current_in = audio_fft[fft_bin] * 2.8;

            // 2. Membrane potential leakage accumulation
            self.voltage[i] = (self.voltage[i] * self.decay) + current_in;

            // 3. Lateral spatial excitation coupling from 4-connected neighbors
            let x = i % self.grid_width;
            let y = i / self.grid_width;
            let mut neighbor_stimulus = 0.0;

            if x > 0 && prev_spikes[i - 1] { neighbor_stimulus += self.lateral_coupling; }
            if x < self.grid_width - 1 && prev_spikes[i + 1] { neighbor_stimulus += self.lateral_coupling; }
            if y > 0 && prev_spikes[i - self.grid_width] { neighbor_stimulus += self.lateral_coupling; }
            if y < self.grid_height - 1 && prev_spikes[i + self.grid_width] { neighbor_stimulus += self.lateral_coupling; }

            self.voltage[i] += neighbor_stimulus;

            // 4. Threshold Spike Check (Fire & Reset)
            if self.voltage[i] >= self.threshold[i] {
                self.spikes[i] = true;
                self.voltage[i] = self.hyperpolarization_reset; // Refractory reset drop
                self.spike_flash += 0.05;
            } else {
                self.spikes[i] = false;
            }

            // 5. Exponential spatial wave smoothing
            let target_wave = if self.spikes[i] { 1.0 } else { self.voltage[i].clamp(0.0, 1.0) };
            let alpha = (dt * 12.0).clamp(0.05, 0.50);
            self.axonal_wave[i] += (target_wave - self.axonal_wave[i]) * alpha;
        }

        self.spike_flash = self.spike_flash.clamp(0.0, 2.5);
    }

    /// Extract global spike firing ratio for bloom & shader controls
    pub fn global_spike_density(&self) -> f32 {
        if self.spikes.is_empty() {
            return 0.0;
        }
        let fired = self.spikes.iter().filter(|&&s| s).count();
        fired as f32 / self.spikes.len() as f32
    }

    /// Extract mean membrane voltage across the grid
    pub fn avg_voltage(&self) -> f32 {
        if self.voltage.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.voltage.iter().sum();
        sum / self.voltage.len() as f32
    }
}

impl Default for SnnCorticalFieldEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for SnnCorticalFieldEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, genome: &VisualGenome) -> Vec<f32> {
        let mut audio_fft = vec![0.0f32; 32];
        let scale = genome.topology_complexity.max(0.1);
        for i in 0..12 {
            audio_fft[i] = nervous.pitch_chroma[i] * scale;
        }
        audio_fft[12] = nervous.low_band * genome.turbulence_scale;
        audio_fft[13] = nervous.mid_band * genome.growth_rate;
        audio_fft[14] = nervous.high_band * genome.roughness;
        audio_fft[15] = nervous.fast_transient_spike;

        for i in 16..32 {
            let gene_val = genome.genes[i % 32];
            audio_fft[i] = nervous.pitch_chroma[i % 12] * 0.7 + nervous.rms_energy * gene_val;
        }

        self.step(&audio_fft, 0.016);
        vec![self.global_spike_density(), self.avg_voltage()]
    }

    fn render(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        genome: &VisualGenome,
        _telemetry: &Option<audio_core::Telemetry>,
        time: f32,
    ) {
        self.prepare_tensor_inputs(nervous, genome);

        let spike_density = self.global_spike_density();
        let avg_v = self.avg_voltage();
        let cell_w = rect.width() / self.grid_width as f32;
        let cell_h = rect.height() / self.grid_height as f32;

        let center_x = rect.center().x;
        let center_y = rect.center().y;

        // Render 2D LIF Membrane Potential Wavefront Grid
        for r in 0..self.grid_height {
            for c in 0..self.grid_width {
                let idx = r * self.grid_width + c;
                let v_val = self.voltage[idx];
                let is_spiking = self.spikes[idx];
                let wave_val = self.axonal_wave[idx];

                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + c as f32 * cell_w, rect.top() + r as f32 * cell_h),
                    egui::vec2(cell_w, cell_h),
                );

                // Compute synesthetic color based on membrane voltage + spike state
                let norm_u = c as f32 / self.grid_width as f32;
                let norm_v = r as f32 / self.grid_height as f32;
                let dist_from_center = (((cell_rect.center().x - center_x).powi(2) + (cell_rect.center().y - center_y).powi(2)).sqrt() / (rect.width() * 0.5)).clamp(0.0, 1.0);

                let r_channel = ((((norm_u * 6.0 + time + avg_v * 4.0).sin() * 0.5 + 0.5) * wave_val * 220.0) as u8)
                    .saturating_add((spike_density * 180.0) as u8);
                let g_channel = ((((norm_v * 4.0 - time * 0.5).cos() * 0.5 + 0.5) * v_val.max(0.0) * 200.0) as u8)
                    .saturating_add((nervous.mid_band * 120.0) as u8);
                let b_channel = ((((dist_from_center * 12.0 + spike_density * 8.0).sin() * 0.5 + 0.5) * 230.0) as u8)
                    .saturating_add((nervous.high_band * 140.0) as u8);

                let mut fill_color = egui::Color32::from_rgb(r_channel, g_channel, b_channel);

                if is_spiking {
                    // Bright optical spike bloom flash
                    fill_color = egui::Color32::from_rgb(255, 240, 200);
                }

                ui.painter().rect_filled(cell_rect, 1.0, fill_color);
            }
        }

        // Global Spike Flash Transient Bloom Overlay
        if self.spike_flash > 0.05 {
            let bloom_alpha = ((self.spike_flash * 60.0).clamp(0.0, 180.0)) as u8;
            ui.painter().rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 230, 200, bloom_alpha),
            );
        }
    }
}
