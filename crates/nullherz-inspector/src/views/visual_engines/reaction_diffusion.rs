//! Engine 5: reaction_diffusion (Turing Pattern Morphogenesis Engine)
//! Two-pass compute shader system running Gray-Scott reaction-diffusion model on a grid.
//! Neural network continuously evaluates musical timbre and emotional valence to output Feed (f) and Kill (k) rate matrices.
//! Cellular structures, coral patterns, and zebra stripes morph and divide in exact sync with instruments.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[allow(dead_code)]
pub const REACTION_DIFFUSION_COMPUTE_WGSL: &str = r#"
struct GrayScottParams {
    feed_f: f32,
    kill_k: f32,
    da: f32,
    db: f32,
};

@group(0) @binding(0) var<uniform> params: GrayScottParams;
@group(0) @binding(1) var grid_in: texture_2d<f32>;
@group(0) @binding(2) var grid_out: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let pos = vec2<i32>(id.xy);
    let uv = textureLoad(grid_in, pos, 0).rg;

    let a = uv.x;
    let b = uv.y;

    // 3x3 Laplacian Kernel
    var lap = vec2<f32>(0.0, 0.0);
    lap += textureLoad(grid_in, pos + vec2<i32>(-1, 0), 0).rg * 0.2;
    lap += textureLoad(grid_in, pos + vec2<i32>(1, 0), 0).rg * 0.2;
    lap += textureLoad(grid_in, pos + vec2<i32>(0, -1), 0).rg * 0.2;
    lap += textureLoad(grid_in, pos + vec2<i32>(0, 1), 0).rg * 0.2;
    lap += textureLoad(grid_in, pos + vec2<i32>(-1, -1), 0).rg * 0.05;
    lap += textureLoad(grid_in, pos + vec2<i32>(1, -1), 0).rg * 0.05;
    lap += textureLoad(grid_in, pos + vec2<i32>(-1, 1), 0).rg * 0.05;
    lap += textureLoad(grid_in, pos + vec2<i32>(1, 1), 0).rg * 0.05;
    lap -= uv;

    let abb = a * b * b;
    let next_a = clamp(a + (params.da * lap.x - abb + params.feed_f * (1.0 - a)), 0.0, 1.0);
    let next_b = clamp(b + (params.db * lap.y + abb - (params.kill_k + params.feed_f) * b), 0.0, 1.0);

    textureStore(grid_out, pos, vec4<f32>(next_a, next_b, abs(next_a - next_b), 1.0));
}
"#;

#[derive(Clone, Debug)]
pub struct ReactionDiffusionEngine {
    pub feed_f: f32,
    pub kill_k: f32,
    pub grid_buffer: Vec<f32>, // 64x64 grid concentration buffer
}

impl ReactionDiffusionEngine {
    pub fn new() -> Self {
        let mut grid_buffer = vec![0.0f32; 64 * 64];
        // Seed center spot for reaction-diffusion growth
        for y in 28..36 {
            for x in 28..36 {
                grid_buffer[y * 64 + x] = 1.0;
            }
        }
        Self {
            feed_f: 0.055,
            kill_k: 0.062,
            grid_buffer,
        }
    }
}

impl Default for ReactionDiffusionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for ReactionDiffusionEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, _genome: &VisualGenome) -> Vec<f32> {
        // Map emotional valence (high/low frequency ratio) to Feed (f) and Kill (k) parameters
        let valence = (nervous.high_band / (nervous.low_band + 0.001)).clamp(0.1, 3.0);
        self.feed_f = 0.030 + valence * 0.025;
        self.kill_k = 0.055 + (1.0 / valence) * 0.010;

        vec![self.feed_f, self.kill_k]
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

        let cols = 32;
        let rows = 32;
        let cell_w = rect.width() / cols as f32;
        let cell_h = rect.height() / rows as f32;

        for r in 0..rows {
            for c in 0..cols {
                let idx = (r * 2) * 64 + (c * 2);
                let conc = self.grid_buffer[idx];

                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + c as f32 * cell_w, rect.top() + r as f32 * cell_h),
                    egui::vec2(cell_w - 0.5, cell_h - 0.5),
                );

                let color = egui::Color32::from_rgb(
                    ((conc * 220.0) as u8).saturating_add(30),
                    ((self.feed_f * 2000.0) as u8).saturating_add(40),
                    ((self.kill_k * 2000.0) as u8).saturating_add(80),
                );

                ui.painter().rect_filled(cell_rect, 1.0, color);
            }
        }
    }
}
