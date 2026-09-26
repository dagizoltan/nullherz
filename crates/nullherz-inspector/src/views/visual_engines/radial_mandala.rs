//! Engine 1: radial_mandala (Hyper-Symmetric CPPN Engine)
//! Maps spatial pixels to Polar Coordinates (r, theta).
//! Tensor input format: [r, theta, sin(theta * N), cos(theta * N), Bass_Energy, Time].
//! 'N' is a symmetry factor dynamically shifted by percussive onset triggers.
//! Provides deep log-polar scaling for infinite centripetal tunneling.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[derive(Clone, Debug)]
pub struct RadialMandalaEngine {
    pub symmetry_n: f32,
    pub log_polar_depth: f32,
    pub _last_onset_time: f32,
}

impl RadialMandalaEngine {
    pub fn new() -> Self {
        Self {
            symmetry_n: 8.0,
            log_polar_depth: 1.0,
            _last_onset_time: 0.0,
        }
    }
}

impl Default for RadialMandalaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralVisualEngine for RadialMandalaEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, _genome: &VisualGenome) -> Vec<f32> {
        // Shift symmetry factor N on percussive onset triggers
        if nervous.onset_strength > 0.75 {
            self.symmetry_n = match (nervous.onset_strength * 12.0) as u32 % 5 {
                0 => 6.0,
                1 => 8.0,
                2 => 12.0,
                3 => 16.0,
                _ => 24.0,
            };
        }
        vec![self.symmetry_n, nervous.low_band, nervous.beat_phase]
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
        let low_energy = nervous.low_band;
        let beat_phase = nervous.beat_phase;
        let bass_energy = nervous.rms_energy;

        self.prepare_tensor_inputs(nervous, genome);

        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.5;

        // Log-polar infinite centripetal tunneling rings
        let rings = 20;
        let n = self.symmetry_n;

        for r_i in 0..rings {
            let frac = (r_i as f32 + (time * 0.5).fract()) / rings as f32;
            let log_r = (frac * self.log_polar_depth + 1.0).ln() * max_r;

            let pts_count = (n * 6.0) as usize;
            let mut ring_pts = Vec::with_capacity(pts_count + 1);

            for p in 0..=pts_count {
                let theta = (p as f32 / pts_count as f32) * std::f32::consts::TAU;

                // CPPN evaluation: [r, theta, sin(theta * N), cos(theta * N), Bass_Energy, Time]
                let sin_nt = (theta * n).sin();
                let cos_nt = (theta * n).cos();

                let cppn_out = (sin_nt * cos_nt * 2.0 + (theta * 2.0 + time * 1.5).cos() * bass_energy).abs();
                let disp_r = log_r * (1.0 + cppn_out * 0.25 * (1.0 + low_energy));

                let x = center.x + theta.cos() * disp_r;
                let y = center.y + theta.sin() * disp_r;
                ring_pts.push(egui::pos2(x, y));
            }

            let hue = (frac + time * 0.1 + beat_phase * 0.2) % 1.0;
            let r_c = ((hue * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0);
            let g_c = (2.0 - (hue * 6.0 - 2.0).abs()).clamp(0.0, 1.0);
            let b_c = (2.0 - (hue * 6.0 - 4.0).abs()).clamp(0.0, 1.0);

            let color = egui::Color32::from_rgb((r_c * 255.0) as u8, (g_c * 255.0) as u8, (b_c * 255.0) as u8);

            for i in 0..ring_pts.len() - 1 {
                ui.painter().line_segment([ring_pts[i], ring_pts[i + 1]], egui::Stroke::new(1.8 + frac * 2.0, color.linear_multiply(0.4 + 0.6 * low_energy)));
            }
        }
    }
}
