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

#[derive(Clone, Debug)]
pub struct SpectralLandscapeEngine {
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
            temporal_matrix: [[0.0; 32]; 64],
            voxel_instances,
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
        _time: f32,
    ) {
        self.prepare_tensor_inputs(nervous, genome);

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
}
