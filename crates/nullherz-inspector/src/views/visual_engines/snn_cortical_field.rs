//! Engine 8: snn_cortical_field (Spiking Cortical Membrane Field Engine)
//! High-resolution 64x64 multi-layer Excitatory/Inhibitory Leaky Integrate-and-Fire (LIF) neural field,
//! continuous 2D Laplacian wave diffusion, synaptic dendrite network rendering, 3D undulating perspective
//! height displacement, and 5 distinct bioluminescent synesthetic color palettes.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnnPalette {
    BioluminescentMycelium,
    CyberpunkSynapse,
    QuantumPlasma,
    SolarFlareCortical,
    VolumetricNeuralMesh,
}

impl SnnPalette {
    pub fn name(&self) -> &'static str {
        match self {
            Self::BioluminescentMycelium => "Bioluminescent Mycelium (Teal / Cyan / Lime)",
            Self::CyberpunkSynapse => "Cyberpunk Synapse (Magenta / Violet / Gold)",
            Self::QuantumPlasma => "Quantum Plasma (Indigo / Ultraviolet / Hot Pink)",
            Self::SolarFlareCortical => "Solar Flare Cortical (Crimson / Plasma Orange / Gold)",
            Self::VolumetricNeuralMesh => "Volumetric Neural Mesh (Emerald / Ghost Cyan / Lime)",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::BioluminescentMycelium,
            Self::CyberpunkSynapse,
            Self::QuantumPlasma,
            Self::SolarFlareCortical,
            Self::VolumetricNeuralMesh,
        ]
    }

    /// Map normalized membrane potential [0, 1], wave intensity, and spike state to color
    pub fn evaluate(&self, v: f32, wave: f32, is_spike: bool, spike_density: f32) -> egui::Color32 {
        if is_spike {
            return match self {
                Self::BioluminescentMycelium => egui::Color32::from_rgb(230, 255, 240),
                Self::CyberpunkSynapse => egui::Color32::from_rgb(255, 235, 170),
                Self::QuantumPlasma => egui::Color32::from_rgb(255, 200, 255),
                Self::SolarFlareCortical => egui::Color32::from_rgb(255, 255, 180),
                Self::VolumetricNeuralMesh => egui::Color32::from_rgb(220, 255, 220),
            };
        }

        let v_c = v.clamp(0.0, 1.0);
        let w_c = wave.clamp(0.0, 1.0);

        match self {
            Self::BioluminescentMycelium => {
                let r = ((w_c * 40.0 + spike_density * 80.0) as u8).saturating_add(5);
                let g = ((v_c * 210.0 + w_c * 45.0) as u8).saturating_add(20);
                let b = ((v_c * 180.0 + w_c * 75.0) as u8).saturating_add(40);
                egui::Color32::from_rgb(r, g, b)
            }
            Self::CyberpunkSynapse => {
                let r = ((v_c * 230.0 + w_c * 25.0) as u8).saturating_add(30);
                let g = ((w_c * 60.0) as u8).saturating_add(10);
                let b = ((v_c * 190.0 + w_c * 65.0) as u8).saturating_add(50);
                egui::Color32::from_rgb(r, g, b)
            }
            Self::QuantumPlasma => {
                let r = ((v_c * 210.0 + spike_density * 90.0) as u8).saturating_add(20);
                let g = ((w_c * 30.0) as u8).saturating_add(5);
                let b = ((v_c * 240.0 + w_c * 15.0) as u8).saturating_add(60);
                egui::Color32::from_rgb(r, g, b)
            }
            Self::SolarFlareCortical => {
                let r = ((v_c * 250.0) as u8).saturating_add(40);
                let g = ((v_c * 140.0 + w_c * 80.0) as u8).saturating_add(15);
                let b = ((w_c * 20.0) as u8).saturating_add(5);
                egui::Color32::from_rgb(r, g, b)
            }
            Self::VolumetricNeuralMesh => {
                let r = ((w_c * 50.0) as u8).saturating_add(10);
                let g = ((v_c * 240.0 + w_c * 15.0) as u8).saturating_add(35);
                let b = ((v_c * 150.0 + w_c * 105.0) as u8).saturating_add(30);
                egui::Color32::from_rgb(r, g, b)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SnnCorticalFieldEngine {
    pub grid_width: usize,
    pub grid_height: usize,
    pub voltage: Vec<f32>,              // Membrane potential (V)
    pub recovery: Vec<f32>,             // Membrane recovery variable (u)
    pub threshold: Vec<f32>,            // Dynamic spike threshold
    pub spikes: Vec<bool>,              // Firing state for current frame
    pub is_inhibitory: Vec<bool>,       // 20% Inhibitory cell distribution bitmask
    pub decay: f32,                     // Potential leakage retention rate (e.g., 0.88)
    pub lateral_coupling: f32,          // Neighbor stimulus excitation factor
    pub hyperpolarization_reset: f32,   // Post-spike refractory reset voltage (-0.2)
    pub laplacian_wave: Vec<f32>,       // Continuous 2D Laplacian wave PDE diffusion grid
    pub spike_flash: f32,               // Global spike flash bloom accumulator
    pub palette: SnnPalette,            // Active color palette
    pub is_3d_projection: bool,         // Toggle 3D undulating surface projection vs 2D biomembrane
    pub draw_dendrites: bool,           // Render active firing synaptic dendrite lines
    pub dendrite_glow: f32,             // Dendrite link line brightness
}

impl SnnCorticalFieldEngine {
    pub fn new() -> Self {
        let grid_width = 64;
        let grid_height = 64;
        let count = grid_width * grid_height;

        let mut is_inhibitory = vec![false; count];
        for i in 0..count {
            if i % 5 == 0 {
                is_inhibitory[i] = true;
            }
        }

        Self {
            grid_width,
            grid_height,
            voltage: vec![0.0; count],
            recovery: vec![0.0; count],
            threshold: vec![1.0; count],
            spikes: vec![false; count],
            is_inhibitory,
            decay: 0.88,
            lateral_coupling: 0.22,
            hyperpolarization_reset: -0.2,
            laplacian_wave: vec![0.0; count],
            spike_flash: 0.0,
            palette: SnnPalette::BioluminescentMycelium,
            is_3d_projection: true,
            draw_dendrites: true,
            dendrite_glow: 1.0,
        }
    }

    /// Step the 64x64 multi-layer Excitatory/Inhibitory LIF Spiking Grid
    pub fn step(&mut self, audio_fft: &[f32], dt: f32) {
        let count = self.grid_width * self.grid_height;
        if audio_fft.is_empty() || count == 0 {
            return;
        }

        // Decay global flash
        self.spike_flash = (self.spike_flash * 0.82).max(0.0);

        let prev_spikes = self.spikes.clone();
        let half_w = self.grid_width as f32 * 0.5;
        let half_h = self.grid_height as f32 * 0.5;

        for i in 0..count {
            let x = i % self.grid_width;
            let y = i / self.grid_width;

            // Multi-frequency spatial placement
            let dist_from_center = (((x as f32 - half_w).powi(2) + (y as f32 - half_h).powi(2)).sqrt() / half_w).clamp(0.0, 1.0);

            let mut current_in = 0.0f32;
            if dist_from_center < 0.35 {
                // Core: Low frequency (kick/bass)
                let low_in = audio_fft.get(12).copied().unwrap_or(0.0);
                current_in += low_in * 3.5;
            } else if dist_from_center < 0.75 {
                // Mid ring: Synths & leads
                let mid_in = audio_fft.get(13).copied().unwrap_or(0.0);
                let chroma_idx = (x + y) % audio_fft.len().min(12).max(1);
                current_in += mid_in * 2.2 + audio_fft[chroma_idx] * 1.5;
            } else {
                // Periphery: High frequency (hi-hats & percussion)
                let high_in = audio_fft.get(14).copied().unwrap_or(0.0);
                current_in += high_in * 3.0;
            }

            // Fast transient full-field injection
            if let Some(&transient) = audio_fft.get(15) {
                current_in += transient * 2.0;
            }

            let cell_type_factor = if self.is_inhibitory[i] { -0.7 } else { 1.2 };
            current_in *= cell_type_factor;

            // 1. Membrane potential leakage & recovery accumulation
            let v = self.voltage[i];
            let u = self.recovery[i];
            self.voltage[i] = (v * self.decay) - (u * 0.05) + current_in;
            self.recovery[i] = u * 0.92 + v * 0.02;

            // 2. Lateral spatial excitation / inhibition coupling
            let mut neighbor_stimulus = 0.0f32;

            if x > 0 && prev_spikes[i - 1] {
                neighbor_stimulus += if self.is_inhibitory[i - 1] { -self.lateral_coupling * 0.8 } else { self.lateral_coupling };
            }
            if x < self.grid_width - 1 && prev_spikes[i + 1] {
                neighbor_stimulus += if self.is_inhibitory[i + 1] { -self.lateral_coupling * 0.8 } else { self.lateral_coupling };
            }
            if y > 0 && prev_spikes[i - self.grid_width] {
                neighbor_stimulus += if self.is_inhibitory[i - self.grid_width] { -self.lateral_coupling * 0.8 } else { self.lateral_coupling };
            }
            if y < self.grid_height - 1 && prev_spikes[i + self.grid_width] {
                neighbor_stimulus += if self.is_inhibitory[i + self.grid_width] { -self.lateral_coupling * 0.8 } else { self.lateral_coupling };
            }

            self.voltage[i] += neighbor_stimulus;

            // 3. Threshold Check (Fire & Refractory Reset)
            if self.voltage[i] >= self.threshold[i] {
                self.spikes[i] = true;
                self.voltage[i] = self.hyperpolarization_reset;
                self.recovery[i] += 0.3;
                self.spike_flash += 0.04;
            } else {
                self.spikes[i] = false;
            }
        }

        // 4. Continuous 2D Laplacian Wave PDE Step: laplacian = N + S + E + W - 4*C
        let mut next_lap = self.laplacian_wave.clone();
        for r in 0..self.grid_height {
            for c in 0..self.grid_width {
                let idx = r * self.grid_width + c;
                let n_idx = ((r + self.grid_height - 1) % self.grid_height) * self.grid_width + c;
                let s_idx = ((r + 1) % self.grid_height) * self.grid_width + c;
                let w_idx = r * self.grid_width + (c + self.grid_width - 1) % self.grid_width;
                let e_idx = r * self.grid_width + (c + 1) % self.grid_width;

                let lap = self.laplacian_wave[n_idx] + self.laplacian_wave[s_idx] + self.laplacian_wave[w_idx] + self.laplacian_wave[e_idx] - 4.0 * self.laplacian_wave[idx];
                let source = if self.spikes[idx] { 1.2 } else { self.voltage[idx].max(0.0) };

                next_lap[idx] += lap * 0.22 + (source - next_lap[idx]) * (dt * 15.0).clamp(0.05, 0.40);
                next_lap[idx] *= 0.94;
            }
        }
        self.laplacian_wave = next_lap;
        self.spike_flash = self.spike_flash.clamp(0.0, 3.0);
    }

    /// Extract global spike firing ratio
    pub fn global_spike_density(&self) -> f32 {
        if self.spikes.is_empty() {
            return 0.0;
        }
        let fired = self.spikes.iter().filter(|&&s| s).count();
        fired as f32 / self.spikes.len() as f32
    }

    /// Extract mean membrane voltage
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
        audio_fft[12] = nervous.low_band * genome.turbulence_scale * 1.5;
        audio_fft[13] = nervous.mid_band * genome.growth_rate * 1.5;
        audio_fft[14] = nervous.high_band * genome.roughness * 1.5;
        audio_fft[15] = nervous.fast_transient_spike * 2.0;

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
        let painter = ui.painter();

        if self.is_3d_projection {
            // Render 3D Perspective Undulating Membrane Mesh
            let pitch_angle = 0.65f32; // Perspective tilt
            let yaw_angle = (time * 0.15).sin() * 0.18;

            let cols = self.grid_width;
            let rows = self.grid_height;
            let center_x = rect.center().x;
            let center_y = rect.center().y + rect.height() * 0.15;

            let scale_x = rect.width() * 0.80 / cols as f32;
            let scale_y = rect.height() * 0.50 / rows as f32;
            let height_scale = rect.height() * 0.28;

            // Render height-displaced quad grid rows back-to-front
            for r in (0..rows - 1).rev() {
                for c in 0..cols - 1 {
                    let idx0 = r * cols + c;
                    let idx1 = r * cols + (c + 1);
                    let idx2 = (r + 1) * cols + (c + 1);
                    let idx3 = (r + 1) * cols + c;

                    let v0 = self.voltage[idx0];
                    let v1 = self.voltage[idx1];
                    let v2 = self.voltage[idx2];
                    let v3 = self.voltage[idx3];

                    let is_spiking = self.spikes[idx0] || self.spikes[idx1] || self.spikes[idx2] || self.spikes[idx3];
                    let wave0 = self.laplacian_wave[idx0];

                    // Project 3D (x, y, z) into 2D screen coordinates
                    let project = |col: usize, row: usize, v_val: f32| -> egui::Pos2 {
                        let nx = (col as f32 - cols as f32 * 0.5) * scale_x;
                        let ny = (row as f32 - rows as f32 * 0.5) * scale_y;

                        // Rotate yaw
                        let rx = nx * yaw_angle.cos() - ny * yaw_angle.sin();
                        let ry = nx * yaw_angle.sin() + ny * yaw_angle.cos();

                        // Tilt pitch & height displacement
                        let p_x = center_x + rx;
                        let p_y = center_y + ry * pitch_angle.sin() - v_val * height_scale * pitch_angle.cos();

                        egui::pos2(p_x, p_y)
                    };

                    let p0 = project(c, r, v0);
                    let p1 = project(c + 1, r, v1);
                    let p2 = project(c + 1, r + 1, v2);
                    let p3 = project(c, r + 1, v3);

                    let fill_color = self.palette.evaluate((v0 + v1 + v2 + v3) * 0.25, wave0, is_spiking, spike_density);

                    // Draw quad polygon
                    painter.add(egui::Shape::convex_polygon(
                        vec![p0, p1, p2, p3],
                        fill_color,
                        egui::Stroke::new(0.5, egui::Color32::from_white_alpha(30)),
                    ));

                    // Draw active firing synaptic dendrite lines
                    if self.draw_dendrites && is_spiking {
                        let stroke_color = egui::Color32::from_rgba_unmultiplied(255, 255, 200, (200.0 * self.dendrite_glow) as u8);
                        painter.line_segment([p0, p1], egui::Stroke::new(1.5, stroke_color));
                        painter.line_segment([p1, p2], egui::Stroke::new(1.5, stroke_color));
                    }
                }
            }
        } else {
            // Render 2D High-Density Biomembrane Surface
            let cell_w = rect.width() / self.grid_width as f32;
            let cell_h = rect.height() / self.grid_height as f32;

            for r in 0..self.grid_height {
                for c in 0..self.grid_width {
                    let idx = r * self.grid_width + c;
                    let v_val = self.voltage[idx];
                    let is_spiking = self.spikes[idx];
                    let wave_val = self.laplacian_wave[idx];

                    let cell_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.left() + c as f32 * cell_w, rect.top() + r as f32 * cell_h),
                        egui::vec2(cell_w, cell_h),
                    );

                    let fill_color = self.palette.evaluate(v_val, wave_val, is_spiking, spike_density);
                    painter.rect_filled(cell_rect, 0.0, fill_color);

                    // Draw firing dendrite connection vectors
                    if self.draw_dendrites && is_spiking && c < self.grid_width - 1 {
                        let p_start = cell_rect.center();
                        let p_end = egui::pos2(p_start.x + cell_w, p_start.y);
                        painter.line_segment([p_start, p_end], egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 255, 220)));
                    }
                }
            }
        }

        // Global Spike Transient Optical Bloom Overlay
        if self.spike_flash > 0.03 {
            let bloom_alpha = ((self.spike_flash * 70.0).clamp(0.0, 200.0)) as u8;
            painter.rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 240, 210, bloom_alpha),
            );
        }
    }
}
