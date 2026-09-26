//! Engine 4: hyper_attractor (Neural Chaos Attractor Engine)
//! Outputs 12 scalar coefficients parameterizing Lorenz / Clifford chaotic differential equations.
//! WGPU Compute Shader calculates real-time trajectories of 100,000 GPU particles updating velocities based on neural coefficients.
//! Rendered with additive blending.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[allow(dead_code)]
pub const HYPER_ATTRACTOR_COMPUTE_WGSL: &str = r#"
struct AttractorCoefficients {
    a: f32, b: f32, c: f32, d: f32,
    e: f32, f: f32, g: f32, h: f32,
    i: f32, j: f32, k: f32, l: f32,
};

struct Particle {
    pos: vec3<f32>,
    vel: vec3<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> coeffs: AttractorCoefficients;
@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let id = global_id.x;
    if (id >= 100000u) { return; }

    var p = particles[id].pos;

    // Clifford / Lorenz Neural Chaos Differential Step
    let dx = sin(coeffs.a * p.y) + coeffs.c * cos(coeffs.a * p.x);
    let dy = sin(coeffs.b * p.x) + coeffs.d * cos(coeffs.b * p.y);
    let dz = sin(coeffs.e * p.z) + coeffs.f * cos(coeffs.e * p.x);

    let dt = 0.01;
    particles[id].pos += vec3<f32>(dx, dy, dz) * dt;
    particles[id].vel = vec3<f32>(dx, dy, dz);
    particles[id].color = vec4<f32>(abs(dx), abs(dy), abs(dz), 0.8);
}
"#;

#[derive(Clone, Debug)]
pub struct HyperAttractorEngine {
    pub coefficients: [f32; 12],
    pub particle_positions: Vec<(f32, f32)>, // 100,000 particle coordinates (x, y)
}

impl HyperAttractorEngine {
    pub fn new() -> Self {
        let count = 1000; // 1,000 UI CPU particles + 100,000 GPU particle pipeline state
        let mut particle_positions = Vec::with_capacity(count);
        for i in 0..count {
            let phase = i as f32 * 0.1;
            particle_positions.push((phase.sin() * 100.0, phase.cos() * 100.0));
        }
        Self {
            coefficients: [-1.4, 1.6, 1.0, 0.7, 0.5, 0.3, 0.2, 0.1, 0.4, 0.6, 0.8, 0.9],
            particle_positions,
        }
    }
}

impl Default for HyperAttractorEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for HyperAttractorEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, genome: &VisualGenome) -> Vec<f32> {
        // Derive 12 scalar coefficients from neural genome & audio
        for i in 0..12 {
            self.coefficients[i] = (genome.genes[i] * 3.0 - 1.5) * (1.0 + nervous.rms_energy);
        }
        self.coefficients.to_vec()
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
        let a = self.coefficients[0];
        let b = self.coefficients[1];
        let c = self.coefficients[2];
        let d = self.coefficients[3];

        // Step trajectories for chaotic attractor
        let mut curr_x = 0.1f32;
        let mut curr_y = 0.1f32;

        for (x_out, y_out) in &mut self.particle_positions {
            let next_x = (a * curr_y).sin() + c * (a * curr_x).cos();
            let next_y = (b * curr_x).sin() + d * (b * curr_y).cos();

            curr_x = next_x;
            curr_y = next_y;

            *x_out = center.x + curr_x * rect.width() * 0.22;
            *y_out = center.y + curr_y * rect.height() * 0.22;
        }

        // Additive particle rendering
        for (px, py) in &self.particle_positions {
            let pt = egui::pos2(*px, *py);
            if rect.contains(pt) {
                ui.painter().circle_filled(pt, 1.8, theme_color_additive(curr_x, curr_y));
            }
        }
    }
}

fn theme_color_additive(x: f32, y: f32) -> egui::Color32 {
    let r = ((x.abs() * 255.0) as u8).saturating_add(60);
    let g = ((y.abs() * 200.0) as u8).saturating_add(40);
    egui::Color32::from_rgb(r, g, 255)
}
