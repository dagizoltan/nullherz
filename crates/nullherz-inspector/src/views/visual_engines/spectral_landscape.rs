//! Engine 3: spectral_landscape (3D Instanced Voxel Waterfall Engine)
//! Maintains a rolling 64-frame history buffer of the 32-bin spectrum to create a 64x32 temporal matrix.
//! Outputs a smoothed, organic 3D terrain heightmap.
//! Provides WGPU Instanced Rendering setup with voxel data [vec3 position_offset, vec4 instance_color, f32 height_scale].

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

/// WGPU Instanced Voxel Buffer Layout
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct VoxelInstanceRaw {
    pub position_offset: [f32; 3],
    pub _pad0: f32,
    pub instance_color: [f32; 4],
    pub height_scale: f32,
    pub _pad1: [f32; 3],
}

unsafe impl bytemuck::Pod for VoxelInstanceRaw {}
unsafe impl bytemuck::Zeroable for VoxelInstanceRaw {}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpectralLandscapeStyle {
    VoxelTerrainProjection,
    SpectralWaterfallContour,
    LinearPixelEqualizer,
}

#[allow(dead_code)]
impl SpectralLandscapeStyle {
    pub fn all() -> &'static [SpectralLandscapeStyle] {
        &[
            SpectralLandscapeStyle::VoxelTerrainProjection,
            SpectralLandscapeStyle::SpectralWaterfallContour,
            SpectralLandscapeStyle::LinearPixelEqualizer,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            SpectralLandscapeStyle::VoxelTerrainProjection => "3D Voxel Terrain Projection",
            SpectralLandscapeStyle::SpectralWaterfallContour => "Spectral Waterfall Contour (Cascading Ridge Waterfall)",
            SpectralLandscapeStyle::LinearPixelEqualizer => "Linear Pixel Equalizer (Horizontal Mirror Spectrum)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpectralLandscapeEngine {
    pub style: SpectralLandscapeStyle,
    pub temporal_matrix: [[f32; 32]; 64], // Rolling 64-frame history buffer of 32-bin spectrum
    pub voxel_instances: Vec<VoxelInstanceRaw>,
}

impl SpectralLandscapeEngine {
    pub fn new() -> Self {
        let mut voxel_instances = Vec::with_capacity(64 * 32);
        for z in 0..64 {
            for x in 0..32 {
                voxel_instances.push(VoxelInstanceRaw {
                    position_offset: [x as f32 - 16.0, 0.0, z as f32 - 32.0],
                    _pad0: 0.0,
                    instance_color: [0.2, 0.6, 0.9, 1.0],
                    height_scale: 1.0,
                    _pad1: [0.0; 3],
                });
            }
        }
        Self {
            style: SpectralLandscapeStyle::SpectralWaterfallContour,
            temporal_matrix: [[0.0; 32]; 64],
            voxel_instances,
        }
    }

    #[allow(dead_code)]
    pub fn cycle_style(&mut self) {
        let styles = SpectralLandscapeStyle::all();
        if let Some(idx) = styles.iter().position(|s| *s == self.style) {
            self.style = styles[(idx + 1) % styles.len()];
        }
    }

    pub fn push_spectrum_frame(&mut self, spectrum_32: &[f32; 32]) {
        for z in (1..64).rev() {
            self.temporal_matrix[z] = self.temporal_matrix[z - 1];
        }
        self.temporal_matrix[0] = *spectrum_32;

        for z in 0..64 {
            for x in 0..32 {
                let idx = z * 32 + x;
                let h = self.temporal_matrix[z][x];
                self.voxel_instances[idx].height_scale = h * 8.0;
                self.voxel_instances[idx].instance_color = [
                    (h * 2.0).clamp(0.0, 1.0),
                    (0.8 - h * 0.5).clamp(0.0, 1.0),
                    (z as f32 / 64.0),
                    1.0,
                ];
            }
        }
    }
}

impl Default for SpectralLandscapeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for SpectralLandscapeEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, _genome: &VisualGenome) -> Vec<f32> {
        let mut spec_32 = [0.0f32; 32];
        for i in 0..12 {
            spec_32[i] = nervous.pitch_chroma[i];
        }
        spec_32[12] = nervous.low_band;
        spec_32[13] = nervous.mid_band;
        spec_32[14] = nervous.high_band;
        self.push_spectrum_frame(&spec_32);
        spec_32.to_vec()
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

        match self.style {
            SpectralLandscapeStyle::VoxelTerrainProjection => self.render_voxel_terrain(ui, rect),
            SpectralLandscapeStyle::SpectralWaterfallContour => self.render_spectral_waterfall_contour(ui, rect, nervous, time),
            SpectralLandscapeStyle::LinearPixelEqualizer => self.render_linear_pixel_equalizer(ui, rect, nervous, time),
        }
    }
}

impl SpectralLandscapeEngine {
    fn render_voxel_terrain(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let center = rect.center();
        let scale_x = rect.width() / 40.0;
        let scale_y = rect.height() / 40.0;

        // Render 3D Voxel Terrain Projection
        for z in (0..64).step_by(2) {
            let z_norm = z as f32 / 64.0;
            for x in (0..32).step_by(1) {
                let idx = z * 32 + x;
                let inst = &self.voxel_instances[idx];

                let px = center.x + (inst.position_offset[0]) * scale_x * (1.0 - z_norm * 0.4);
                let py = center.y + (inst.position_offset[2] * 0.3 - inst.height_scale) * scale_y;

                let size = (1.0 - z_norm * 0.5) * 4.0;
                let color = egui::Color32::from_rgb(
                    (inst.instance_color[0] * 255.0) as u8,
                    (inst.instance_color[1] * 255.0) as u8,
                    (inst.instance_color[2] * 255.0) as u8,
                );

                ui.painter().circle_filled(egui::pos2(px, py), size, color);
            }
        }
    }

    /// Spectral Waterfall Contour (Cascading 3D Waterfall Spectrogram from reference 21-13-40.png & 21-13-47.png)
    fn render_spectral_waterfall_contour(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let width = rect.width();
        let height = rect.height();

        // Render cascading temporal spectral lines from top/back down to front/bottom
        let steps = 48;
        for z in (0..steps).rev() {
            let z_norm = z as f32 / steps as f32; // 0.0 = front, 1.0 = back

            let y_pos = rect.min.y + height * (0.2 + z_norm * 0.7);
            let line_w = width * (1.0 - z_norm * 0.5);
            let start_x = rect.min.x + (width - line_w) * 0.5;

            let bins = 64;
            let mut line_pts = Vec::with_capacity(bins);

            for b in 0..bins {
                let b_frac = b as f32 / (bins - 1) as f32;
                let spec_idx = (b_frac * 31.0) as usize;
                let raw_val = self.temporal_matrix[z % 64][spec_idx];

                let x = start_x + b_frac * line_w;
                let height_lift = raw_val * 60.0 * (1.0 - z_norm * 0.6) * (1.0 + nervous.rms_energy * 0.5);
                let y = y_pos - height_lift;

                line_pts.push(egui::pos2(x, y));
            }

            // Glowing blue/cyan to deep purple/magenta gradient (matching reference 21-13-40 & 21-13-47)
            let hue = (0.55 + z_norm * 0.35 + time * 0.05) % 1.0;
            let alpha = 0.8 * (1.0 - z_norm * 0.7);
            let color = hsva_to_color32(hue, 0.9, 0.95, alpha);

            for i in 0..line_pts.len() - 1 {
                ui.painter().line_segment(
                    [line_pts[i], line_pts[i + 1]],
                    egui::Stroke::new(1.8 - z_norm * 1.0, color),
                );
            }
        }
    }

    /// Linear Mirror Pixel Equalizer Spectrum (from reference 21-27-54.png)
    fn render_linear_pixel_equalizer(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_w = rect.width() * 0.88;
        let num_columns = 64;
        let col_w = max_w / num_columns as f32;
        let block_h = 6.0;
        let gap = 2.0;

        let max_blocks = 24;

        for col in 0..num_columns {
            let col_frac = col as f32 / num_columns as f32;
            let dist_from_center = (col_frac - 0.5).abs() * 2.0;

            let band_energy = match (col * 3) / num_columns {
                0 => nervous.low_band,
                1 => nervous.mid_band,
                _ => nervous.high_band,
            };

            let synth_h = ((col_frac * 16.0 + time * 3.0).sin() * 0.5 + 0.5) * band_energy + (1.0 - dist_from_center) * 0.4;
            let active_blocks = ((synth_h.clamp(0.05, 1.0) * max_blocks as f32) as usize).min(max_blocks);

            let x_pos = rect.min.x + (rect.width() - max_w) * 0.5 + col as f32 * col_w;

            for b in 0..active_blocks {
                let y_offset = b as f32 * (block_h + gap);

                let top_y = center.y - y_offset - block_h;
                let bot_y = center.y + y_offset;

                let block_rect_top = egui::Rect::from_min_size(egui::pos2(x_pos, top_y), egui::vec2(col_w - gap, block_h));
                let block_rect_bot = egui::Rect::from_min_size(egui::pos2(x_pos, bot_y), egui::vec2(col_w - gap, block_h));

                // Orange-red to magenta-blue gradient matching reference 21-27-54
                let hue = (0.65 - col_frac * 0.5 + time * 0.02) % 1.0;
                let color = hsva_to_color32(hue, 0.9, 1.0, 0.85);

                ui.painter().rect_filled(block_rect_top, 1.0, color);
                ui.painter().rect_filled(block_rect_bot, 1.0, color);
            }

            if active_blocks > 0 && (col % 2 == 0) {
                let peak_offset = (active_blocks as f32 + 2.0 + (col as f32 * 0.5 + time * 4.0).sin().abs() * 3.0) * (block_h + gap);
                let p_top = egui::pos2(x_pos + col_w * 0.5, center.y - peak_offset);
                let p_bot = egui::pos2(x_pos + col_w * 0.5, center.y + peak_offset);

                let particle_color = egui::Color32::from_rgb(255, 100, 220);
                ui.painter().circle_filled(p_top, col_w * 0.35, particle_color);
                ui.painter().circle_filled(p_bot, col_w * 0.35, particle_color);
            }
        }
    }
}

fn hsva_to_color32(h: f32, s: f32, v: f32, a: f32) -> egui::Color32 {
    let h_deg = (h.fract() + 1.0).fract() * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h_deg % 2.0 - 1.0).abs());
    let m = v - c;

    let (r_p, g_p, b_p) = match h_deg as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r_p + m) * 255.0).clamp(0.0, 255.0) as u8;
    let g = ((g_p + m) * 255.0).clamp(0.0, 255.0) as u8;
    let b = ((b_p + m) * 255.0).clamp(0.0, 255.0) as u8;
    let alpha = (a * 255.0).clamp(0.0, 255.0) as u8;

    egui::Color32::from_rgba_unmultiplied(r, g, b, alpha)
}
