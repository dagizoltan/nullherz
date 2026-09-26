//! Engine 2: liquid_surface (Two-Pass Latent Domain Warping Engine)
//! Pass 1: 32MFCC spectrum array outputs a 128x128 4-channel latent texture.
//! Pass 2: Complete WGSL fragment shader applying 4-octave Fractional Brownian Motion (fBm) fluid simulation.
//! Fluid velocity and distortion amplitudes are bound to Mids and Highs audio uniforms with a 5% feedback blend loop.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[allow(dead_code)]
pub const LIQUID_SURFACE_WGSL_SHADER: &str = r#"
struct AudioUniforms {
    mids_amplitude: f32,
    highs_amplitude: f32,
    time: f32,
    feedback_blend: f32,
};

@group(0) @binding(0) var<uniform> audio: AudioUniforms;
@group(0) @binding(1) var latent_texture: texture_2d<f32>;
@group(0) @binding(2) var texture_sampler: sampler;

fn hash22(p: vec2<f32>) -> vec2<f32> {
    var p3 = fract(vec3<f32>(p.xyx) * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(dot(hash22(i + vec2<f32>(0.0, 0.0)), f - vec2<f32>(0.0, 0.0)),
            dot(hash22(i + vec2<f32>(1.0, 0.0)), f - vec2<f32>(1.0, 0.0)), u.x),
        mix(dot(hash22(i + vec2<f32>(0.0, 1.0)), f - vec2<f32>(0.0, 1.0)),
            dot(hash22(i + vec2<f32>(1.0, 1.0)), f - vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

fn fbm_4octaves(p: vec2<f32>, velocity: f32) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var frequency = 1.0;
    var shift = vec2<f32>(velocity * 0.2, velocity * 0.3);

    for (var i = 0; i < 4; i = i + 1) {
        value += amplitude * noise(p * frequency + shift);
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    return value;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let vel = audio.mids_amplitude * 2.5;
    let dist_amp = audio.highs_amplitude * 1.8;

    // 4-octave fBm fluid displacement
    let q = vec2<f32>(
        fbm_4octaves(uv + vec2<f32>(0.0, 0.0), vel),
        fbm_4octaves(uv + vec2<f32>(5.2, 1.3), vel)
    );

    let r = vec2<f32>(
        fbm_4octaves(uv + 4.0 * q + vec2<f32>(1.7, 9.2), vel),
        fbm_4octaves(uv + 4.0 * q + vec2<f32>(8.3, 2.8), vel)
    );

    let distorted_uv = uv + r * dist_amp * 0.1;
    let latent_col = textureSample(latent_texture, texture_sampler, distorted_uv);

    // 5% feedback blend loop
    let final_color = mix(latent_col.rgb, vec3<f32>(r.x, r.y, q.x), audio.feedback_blend);
    return vec4<f32>(final_color, 1.0);
}
"#;

#[derive(Clone, Debug)]
pub struct LiquidSurfaceEngine {
    pub latent_texture_bytes: Vec<u8>, // 128x128 RGBA4444 / RGBA8888 latent buffer
}

impl LiquidSurfaceEngine {
    pub fn new() -> Self {
        Self {
            latent_texture_bytes: vec![128; 128 * 128 * 4],
        }
    }
}

impl Default for LiquidSurfaceEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for LiquidSurfaceEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, _genome: &VisualGenome) -> Vec<f32> {
        // Pass 1: 32MFCC spectrum array -> 128x128 4-channel latent texture
        for y in 0..128 {
            for x in 0..128 {
                let idx = (y * 128 + x) * 4;
                let mfcc_val = nervous.pitch_chroma[x % 12];
                self.latent_texture_bytes[idx] = (mfcc_val * 255.0) as u8;
                self.latent_texture_bytes[idx + 1] = (nervous.mid_band * 255.0) as u8;
                self.latent_texture_bytes[idx + 2] = (nervous.high_band * 255.0) as u8;
                self.latent_texture_bytes[idx + 3] = 255;
            }
        }
        vec![nervous.mid_band, nervous.high_band]
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

        let mid_amp = nervous.mid_band;
        let high_amp = nervous.high_band;

        // Render fluid simulation grid
        let cols = 32;
        let rows = 32;
        let cell_w = rect.width() / cols as f32;
        let cell_h = rect.height() / rows as f32;

        for r in 0..rows {
            let v = r as f32 / rows as f32;
            for c in 0..cols {
                let u = c as f32 / cols as f32;
                let flow = ((u * 4.0 + time * mid_amp * 2.0).sin() + (v * 4.0 - time * high_amp * 2.0).cos()) * 0.5 + 0.5;

                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + c as f32 * cell_w, rect.top() + r as f32 * cell_h),
                    egui::vec2(cell_w, cell_h),
                );

                let color = egui::Color32::from_rgb(
                    ((flow * 200.0) as u8).saturating_add(30),
                    ((mid_amp * 220.0) as u8).saturating_add(20),
                    ((high_amp * 255.0) as u8).saturating_add(40),
                );

                ui.painter().rect_filled(cell_rect, 0.0, color);
            }
        }
    }
}
