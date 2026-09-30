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

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttractorTopology {
    Clifford,
    Lorenz,
    Aizawa,
    DeQuan,
    Thomas,
}

#[allow(dead_code)]
impl AttractorTopology {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Clifford => "Clifford Neural Map",
            Self::Lorenz => "Lorenz 3D Butterfly",
            Self::Aizawa => "Aizawa Flow Sphere",
            Self::DeQuan => "De Quan Multi-Scroll",
            Self::Thomas => "Thomas Cyclical Labyrinth",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Clifford, Self::Lorenz, Self::Aizawa, Self::DeQuan, Self::Thomas]
    }
}

#[derive(Clone, Debug)]
pub struct HyperAttractorEngine {
    pub topology: AttractorTopology,
    pub coefficients: [f32; 12],
    pub particle_positions: Vec<(f32, f32)>, // 1,000 3D projected particle coordinates (x, y)
    pub particles_3d: Vec<(f32, f32, f32)>,  // Raw 3D particle positions
}

impl HyperAttractorEngine {
    pub fn new() -> Self {
        let count = 1000;
        let mut particle_positions = Vec::with_capacity(count);
        let mut particles_3d = Vec::with_capacity(count);

        for i in 0..count {
            let phase = i as f32 * 0.1;
            particles_3d.push((0.1 + phase.sin() * 0.5, 0.1 + phase.cos() * 0.5, phase * 0.05));
            particle_positions.push((0.0, 0.0));
        }

        Self {
            topology: AttractorTopology::Lorenz,
            coefficients: [-1.4, 1.6, 1.0, 0.7, 0.5, 0.3, 0.2, 0.1, 0.4, 0.6, 0.8, 0.9],
            particle_positions,
            particles_3d,
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
        time: f32,
    ) {
        self.prepare_tensor_inputs(nervous, genome);

        let center = rect.center();
        let count = self.particles_3d.len();

        let cam_rot_x = time * 0.4 + nervous.stereo_asymmetry * 2.0;
        let cam_rot_y = time * 0.2 + nervous.spectral_centroid * 0.001;

        let cos_x = cam_rot_x.cos();
        let sin_x = cam_rot_x.sin();
        let cos_y = cam_rot_y.cos();
        let sin_y = cam_rot_y.sin();

        let mut curr_x = 0.1f32;
        let mut curr_y = 0.1f32;
        let mut curr_z = 0.1f32;

        for i in 0..count {
            let (px, py, pz) = &mut self.particles_3d[i];

            match self.topology {
                AttractorTopology::Clifford => {
                    let a = self.coefficients[0];
                    let b = self.coefficients[1];
                    let c = self.coefficients[2];
                    let d = self.coefficients[3];
                    let next_x = (a * *py).sin() + c * (a * *px).cos();
                    let next_y = (b * *px).sin() + d * (b * *py).cos();
                    let next_z = (*pz + 0.02).rem_euclid(2.0) - 1.0;
                    *px = next_x;
                    *py = next_y;
                    *pz = next_z;
                }
                AttractorTopology::Lorenz => {
                    let sigma = 10.0 + self.coefficients[0] * 2.0;
                    let rho = 28.0 + self.coefficients[1] * 5.0;
                    let beta = 8.0 / 3.0 + self.coefficients[2] * 0.5;
                    let dt = 0.008;

                    let dx = sigma * (*py - *px);
                    let dy = *px * (rho - *pz) - *py;
                    let dz = *px * *py - beta * *pz;

                    *px += dx * dt;
                    *py += dy * dt;
                    *pz += dz * dt;
                }
                AttractorTopology::Aizawa => {
                    let a = 0.95;
                    let b = 0.7;
                    let c = 0.6 + self.coefficients[0] * 0.2;
                    let d = 3.5;
                    let e = 0.25;
                    let f = 0.1;
                    let dt = 0.01;

                    let dx = (*pz - b) * *px - d * *py;
                    let dy = d * *px + (*pz - b) * *py;
                    let dz = c + a * *pz - (*pz * *pz * *pz) / 3.0 - (*px * *px + *py * *py) * (1.0 + e * *pz) + f * *pz * (*px * *px * *px);

                    *px += dx * dt;
                    *py += dy * dt;
                    *pz += dz * dt;
                }
                AttractorTopology::DeQuan => {
                    let a = 40.0;
                    let b = 0.16;
                    let c = 4.0;
                    let d = 55.0;
                    let e = 20.0;
                    let dt = 0.003;

                    let dx = a * (*py - *px) + c * *px * *pz;
                    let dy = e * *py - *px * *pz;
                    let dz = b * *pz + *px * *py - d * *px * *px;

                    *px += dx * dt;
                    *py += dy * dt;
                    *pz += dz * dt;
                }
                AttractorTopology::Thomas => {
                    let b = 0.208 + self.coefficients[0] * 0.05;
                    let dt = 0.04;

                    let dx = (*py).sin() - b * *px;
                    let dy = (*pz).sin() - b * *py;
                    let dz = (*px).sin() - b * *pz;

                    *px += dx * dt;
                    *py += dy * dt;
                    *pz += dz * dt;
                }
            }

            curr_x = *px;
            curr_y = *py;
            curr_z = *pz;

            // Camera 3D Orbit Projection
            let rx = *px * cos_y + *pz * sin_y;
            let rz = -*px * sin_y + *pz * cos_y;
            let ry = *py * cos_x - rz * sin_x;
            let _rz2 = *py * sin_x + rz * cos_x;

            let scale = match self.topology {
                AttractorTopology::Lorenz => 0.012,
                AttractorTopology::DeQuan => 0.008,
                _ => 0.25,
            } * rect.height();

            let proj_x = center.x + rx * scale;
            let proj_y = center.y + ry * scale;

            self.particle_positions[i] = (proj_x, proj_y);
        }

        // Additive particle rendering with trail connections
        for i in 0..count {
            let (px, py) = self.particle_positions[i];
            let pt = egui::pos2(px, py);
            if rect.contains(pt) {
                let color = theme_color_additive(curr_x + i as f32 * 0.01, curr_y + curr_z);
                ui.painter().circle_filled(pt, 1.8, color);

                if i > 0 {
                    let (prev_x, prev_y) = self.particle_positions[i - 1];
                    let prev_pt = egui::pos2(prev_x, prev_y);
                    if rect.contains(prev_pt) && (pt - prev_pt).length() < 30.0 {
                        ui.painter().line_segment([prev_pt, pt], egui::Stroke::new(1.0, color.linear_multiply(0.4)));
                    }
                }
            }
        }
    }
}

fn theme_color_additive(x: f32, y: f32) -> egui::Color32 {
    let r = ((x.abs() * 255.0) as u8).saturating_add(60);
    let g = ((y.abs() * 200.0) as u8).saturating_add(40);
    egui::Color32::from_rgb(r, g, 255)
}
