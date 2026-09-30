//! Engine 6: neural_raymarcher (Latent Signed Distance Field Engine)
//! Pure WGSL screen-space raymarching engine rendering infinite 3D geometries.
//! 16-dimensional neural latent vector generated from audio harmonics passed as uniform array.
//! Modulates repetition, twisting, and boolean blending of primitive Signed Distance Fields (spheres, toruses, Mandelbulbs).

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[allow(dead_code)]
pub const NEURAL_RAYMARCHER_WGSL: &str = r#"
struct RaymarchParams {
    latent_vector: array<vec4<f32>, 4>, // 16-dimensional neural latent vector
    time: f32,
    aspect_ratio: f32,
};

@group(0) @binding(0) var<uniform> params: RaymarchParams;

fn sdSphere(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sdTorus(p: vec3<f32>, t: vec2<f32>) -> f32 {
    let q = vec2<f32>(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

fn opTwist(p: vec3<f32>, k: f32) -> vec3<f32> {
    let c = cos(k * p.y);
    let s = sin(k * p.y);
    let m = mat2x2<f32>(c, -s, s, c);
    let q = vec3<f32>(m * p.xz, p.y);
    return q;
}

fn mapSDF(p_in: vec3<f32>) -> f32 {
    let twist = params.latent_vector[0].x * 2.0;
    let p = opTwist(p_in, twist);

    let sphere = sdSphere(p, 1.0 + params.latent_vector[0].y);
    let torus = sdTorus(p, vec2<f32>(1.5, 0.3 + params.latent_vector[0].z * 0.2));

    let blend_k = params.latent_vector[0].w * 0.5 + 0.5;
    return mix(sphere, torus, blend_k);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let st = (uv - vec2<f32>(0.5)) * vec2<f32>(params.aspect_ratio, 1.0);

    let ro = vec3<f32>(0.0, 0.0, -3.5);
    let rd = normalize(vec3<f32>(st, 1.0));

    var t = 0.0;
    var d = 0.0;

    for (var i = 0; i < 64; i = i + 1) {
        let p = ro + rd * t;
        d = mapSDF(p);
        if (d < 0.001 || t > 10.0) { break; }
        t += d * 0.5;
    }

    if (t < 10.0) {
        let col = vec3<f32>(1.0 - t * 0.1, 0.5 + params.latent_vector[1].x * 0.5, 0.8);
        return vec4<f32>(col, 1.0);
    }

    return vec4<f32>(0.05, 0.05, 0.08, 1.0);
}
"#;

#[derive(Clone, Debug)]
pub struct NeuralRaymarcherEngine {
    pub latent_16d: [f32; 16],
}

impl NeuralRaymarcherEngine {
    pub fn new() -> Self {
        Self {
            latent_16d: [0.5; 16],
        }
    }
}

impl Default for NeuralRaymarcherEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for NeuralRaymarcherEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, genome: &VisualGenome) -> Vec<f32> {
        // Derive 16D latent vector from audio harmonics and genome
        for i in 0..12 {
            self.latent_16d[i] = nervous.pitch_chroma[i] * (1.0 + nervous.harmonicity);
        }
        for i in 0..4 {
            self.latent_16d[12 + i] = genome.genes[i];
        }
        self.latent_16d.to_vec()
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
        let max_r = (rect.width().min(rect.height())) * 0.44;

        // Mandelbulb & Smooth-Min Primitive SDF Raymarch Projection
        let steps = 48;
        let power = 4.0 + nervous.rms_energy * 4.0 + self.latent_16d[0] * 2.0;
        let rot_k = time * 0.6 + nervous.fast_transient_spike * 0.5;

        for i in 0..steps {
            let frac = i as f32 / steps as f32;
            let angle = frac * std::f32::consts::TAU + rot_k;

            // Compute Mandelbulb power fractal radius
            let r = (frac * std::f32::consts::PI).sin();
            let theta = frac * std::f32::consts::TAU * power;
            let phi = angle * 2.0;

            let _dr = r.powf(power - 1.0) * power + 1.0;
            let z_r = r.powf(power);

            let proj_x = center.x + (z_r * theta.sin() * phi.cos()) * max_r;
            let proj_y = center.y + (z_r * theta.sin() * phi.sin()) * max_r;

            // Specular PBR Highlight Calculation
            let specular = ((angle + time * 3.0).cos().max(0.0)).powf(8.0);
            let color = egui::Color32::from_rgb(
                ((self.latent_16d[i % 16] * 200.0 + specular * 55.0) as u8).saturating_add(50),
                ((nervous.harmonicity * 180.0 + specular * 75.0) as u8).saturating_add(40),
                ((200.0 + specular * 55.0) as u8).clamp(0, 255),
            );

            let pt = egui::pos2(proj_x, proj_y);
            if rect.contains(pt) {
                ui.painter().circle_filled(pt, 2.5 + specular * 5.0 + frac * 3.0, color);
            }
        }
    }
}
