//! Engine 1: radial_mandala (Hyper-Symmetric & Multi-Style Radial Mandala Engine)
//! Maps spatial pixels to Polar Coordinates (r, theta).
//! Generates multiple distinct radial mandala visual styles inspired by sacred geometry,
//! neon matrix grids, celestial pulses, floral fractals, and hyper-symmetric CPPN motifs.

use eframe::egui;
use crate::state::{AudioNervousSystem, VisualGenome};
use super::NeuralVisualEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MandalaStyle {
    SacredGeometry,
    NeonMatrixGrid,
    CelestialPulse,
    FloralFractal,
    HyperSymmetricCPPN,
}

impl MandalaStyle {
    pub fn all() -> &'static [MandalaStyle] {
        &[
            MandalaStyle::SacredGeometry,
            MandalaStyle::NeonMatrixGrid,
            MandalaStyle::CelestialPulse,
            MandalaStyle::FloralFractal,
            MandalaStyle::HyperSymmetricCPPN,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            MandalaStyle::SacredGeometry => "Sacred Geometry (Polygons & Filigree)",
            MandalaStyle::NeonMatrixGrid => "Neon Matrix Grid (Cyberpunk Radar)",
            MandalaStyle::CelestialPulse => "Celestial Pulse (Corona & Solar Flares)",
            MandalaStyle::FloralFractal => "Floral Fractal (Blooming Petals)",
            MandalaStyle::HyperSymmetricCPPN => "Hyper-Symmetric CPPN (Log-Polar Tunnel)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RadialMandalaEngine {
    pub style: MandalaStyle,
    pub symmetry_n: f32,
    pub log_polar_depth: f32,
    pub layer_count: usize,
    pub rotation_speed: f32,
    pub color_palette_shift: f32,
    pub _last_onset_time: f32,
}

impl RadialMandalaEngine {
    pub fn new() -> Self {
        Self {
            style: MandalaStyle::SacredGeometry,
            symmetry_n: 8.0,
            log_polar_depth: 1.0,
            layer_count: 16,
            rotation_speed: 0.5,
            color_palette_shift: 0.0,
            _last_onset_time: 0.0,
        }
    }

    #[allow(dead_code)]
    pub fn cycle_style(&mut self) {
        let styles = MandalaStyle::all();
        if let Some(idx) = styles.iter().position(|s| *s == self.style) {
            self.style = styles[(idx + 1) % styles.len()];
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
            self.color_palette_shift = (self.color_palette_shift + 0.2) % 1.0;
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
        self.prepare_tensor_inputs(nervous, genome);

        match self.style {
            MandalaStyle::SacredGeometry => self.render_sacred_geometry(ui, rect, nervous, time),
            MandalaStyle::NeonMatrixGrid => self.render_neon_matrix_grid(ui, rect, nervous, time),
            MandalaStyle::CelestialPulse => self.render_celestial_pulse(ui, rect, nervous, time),
            MandalaStyle::FloralFractal => self.render_floral_fractal(ui, rect, nervous, time),
            MandalaStyle::HyperSymmetricCPPN => self.render_hyper_symmetric_cppn(ui, rect, nervous, time),
        }
    }
}

impl RadialMandalaEngine {
    fn render_sacred_geometry(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.45;
        let n = self.symmetry_n as usize;
        let rot = time * self.rotation_speed * 0.2;

        // Concentric star polygons
        let layers = self.layer_count.min(12);
        for l in 1..=layers {
            let layer_frac = l as f32 / layers as f32;
            let radius = max_r * layer_frac * (1.0 + nervous.low_band * 0.15);
            let layer_rot = rot * if l % 2 == 0 { 1.0 } else { -1.0 };

            // Outer star polygon
            let mut star_pts = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let theta = (i as f32 / n as f32) * std::f32::consts::TAU + layer_rot;
                let mod_r = radius * (1.0 + 0.1 * ((theta * n as f32 * 0.5).sin() * nervous.mid_band));
                star_pts.push(egui::pos2(center.x + theta.cos() * mod_r, center.y + theta.sin() * mod_r));
            }

            let hue = (layer_frac * 0.8 + time * 0.05 + self.color_palette_shift) % 1.0;
            let color = hsva_to_color32(hue, 0.8, 0.9, 0.7 + nervous.rms_energy * 0.3);

            for i in 0..star_pts.len() - 1 {
                ui.painter().line_segment(
                    [star_pts[i], star_pts[i + 1]],
                    egui::Stroke::new(1.5 + layer_frac * 1.5, color),
                );
            }

            // Cross-filigree diagonals
            if l % 2 == 0 {
                for i in 0..n {
                    let j = (i + n / 2) % n;
                    ui.painter().line_segment(
                        [star_pts[i], star_pts[j]],
                        egui::Stroke::new(0.8, color.linear_multiply(0.4)),
                    );
                }
            }

            // Sacred geometry overlapping circles on vertices
            if l == layers / 2 || l == layers {
                for i in 0..n {
                    let circle_r = radius * 0.35 * (1.0 + nervous.high_band * 0.2);
                    ui.painter().circle_stroke(
                        star_pts[i],
                        circle_r,
                        egui::Stroke::new(1.0, color.linear_multiply(0.5)),
                    );
                }
            }
        }
    }

    fn render_neon_matrix_grid(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.48;
        let n = self.symmetry_n as usize * 2;
        let rot = time * self.rotation_speed;

        // Concentric log-polar grid rings
        let ring_count = 10;
        for r_i in 1..=ring_count {
            let frac = r_i as f32 / ring_count as f32;
            let radius = max_r * frac.powf(1.2) * (1.0 + nervous.low_band * 0.1);

            let hue = (0.5 + frac * 0.3 + time * 0.1) % 1.0; // Cyan-purple cyber palette
            let color = hsva_to_color32(hue, 0.9, 1.0, 0.6 + nervous.rms_energy * 0.4);

            ui.painter().circle_stroke(
                center,
                radius,
                egui::Stroke::new(1.2 + nervous.high_band, color),
            );
        }

        // Radial radar sweep rays
        for i in 0..n {
            let theta = (i as f32 / n as f32) * std::f32::consts::TAU + rot;
            let ray_len = max_r * (1.0 + 0.15 * (theta * 4.0 + time * 3.0).sin() * nervous.mid_band);

            let p1 = center;
            let p2 = egui::pos2(center.x + theta.cos() * ray_len, center.y + theta.sin() * ray_len);

            let hue = (i as f32 / n as f32 + self.color_palette_shift) % 1.0;
            let color = hsva_to_color32(hue, 0.85, 0.95, 0.5 + nervous.low_band * 0.5);

            ui.painter().line_segment([p1, p2], egui::Stroke::new(1.0, color));
        }

        // Active radar pulse arc
        let sweep_angle = (time * 2.0) % std::f32::consts::TAU;
        let sweep_p2 = egui::pos2(center.x + sweep_angle.cos() * max_r, center.y + sweep_angle.sin() * max_r);
        ui.painter().line_segment([center, sweep_p2], egui::Stroke::new(3.0, egui::Color32::from_rgb(0, 255, 200)));
    }

    fn render_celestial_pulse(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.45;
        let rays = (self.symmetry_n * 3.0) as usize;

        // Central glowing solar core
        let core_r = max_r * 0.15 * (1.0 + nervous.low_band * 0.4);
        ui.painter().circle_filled(
            center,
            core_r,
            egui::Color32::from_rgb(255, 220, 120),
        );

        // Coronal flare rays
        for i in 0..rays {
            let theta = (i as f32 / rays as f32) * std::f32::consts::TAU + time * 0.1;
            let ray_pulse = (theta * 8.0 + time * 4.0).sin().abs();
            let ray_len = core_r + (max_r - core_r) * (0.3 + 0.7 * ray_pulse) * (1.0 + nervous.rms_energy * 0.3);

            let p_end = egui::pos2(center.x + theta.cos() * ray_len, center.y + theta.sin() * ray_len);
            let hue = (0.05 + ray_pulse * 0.15 + self.color_palette_shift) % 1.0; // Warm gold-orange
            let color = hsva_to_color32(hue, 0.9, 1.0, 0.6 + nervous.high_band * 0.4);

            ui.painter().line_segment([center, p_end], egui::Stroke::new(1.8, color));
        }

        // Orbital starlight points
        let star_count = 24;
        for s in 0..star_count {
            let orbital_r = max_r * (0.4 + 0.5 * (s as f32 / star_count as f32));
            let orb_theta = (s as f32 / star_count as f32) * std::f32::consts::TAU + time * (0.2 + s as f32 * 0.02);
            let star_p = egui::pos2(center.x + orb_theta.cos() * orbital_r, center.y + orb_theta.sin() * orbital_r);

            let star_size = 2.0 + (s % 3) as f32 + nervous.high_band * 3.0;
            ui.painter().circle_filled(star_p, star_size, egui::Color32::from_rgb(255, 255, 230));
        }
    }

    fn render_floral_fractal(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.45;
        let petals = self.symmetry_n as usize;
        let layers = 5;

        for l in 0..layers {
            let layer_frac = (l + 1) as f32 / layers as f32;
            let radius = max_r * layer_frac * (1.0 + nervous.low_band * 0.2);
            let layer_rot = time * self.rotation_speed * (if l % 2 == 0 { 0.3 } else { -0.3 });

            for p in 0..petals {
                let base_theta = (p as f32 / petals as f32) * std::f32::consts::TAU + layer_rot;

                // Floral petal Bezier/curved segment points
                let pts_per_petal = 12;
                let mut petal_pts = Vec::with_capacity(pts_per_petal);

                for step in 0..pts_per_petal {
                    let t = step as f32 / (pts_per_petal - 1) as f32;
                    let angle_offset = (t * std::f32::consts::PI).sin() * (0.4 / petals as f32 * std::f32::consts::TAU);
                    let theta = base_theta + angle_offset;

                    let petal_r = radius * t * (1.0 + 0.2 * (t * std::f32::consts::PI).sin() * nervous.mid_band);
                    petal_pts.push(egui::pos2(center.x + theta.cos() * petal_r, center.y + theta.sin() * petal_r));
                }

                let hue = (0.8 + layer_frac * 0.3 + p as f32 / petals as f32 * 0.2 + self.color_palette_shift) % 1.0; // Magenta-violet-rose
                let color = hsva_to_color32(hue, 0.8, 0.95, 0.65 + nervous.rms_energy * 0.35);

                for i in 0..petal_pts.len() - 1 {
                    ui.painter().line_segment(
                        [petal_pts[i], petal_pts[i + 1]],
                        egui::Stroke::new(1.8, color),
                    );
                }
            }
        }
    }

    fn render_hyper_symmetric_cppn(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let low_energy = nervous.low_band;
        let beat_phase = nervous.beat_phase;
        let bass_energy = nervous.rms_energy;

        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.5;

        let rings = self.layer_count.max(12);
        let n = self.symmetry_n;

        for r_i in 0..rings {
            let frac = (r_i as f32 + (time * self.rotation_speed).fract()) / rings as f32;
            let log_r = (frac * self.log_polar_depth + 1.0).ln() * max_r;

            let pts_count = (n * 6.0) as usize;
            let mut ring_pts = Vec::with_capacity(pts_count + 1);

            for p in 0..=pts_count {
                let theta = (p as f32 / pts_count as f32) * std::f32::consts::TAU;

                let sin_nt = (theta * n).sin();
                let cos_nt = (theta * n).cos();

                let cppn_out = (sin_nt * cos_nt * 2.0 + (theta * 2.0 + time * 1.5).cos() * bass_energy).abs();
                let disp_r = log_r * (1.0 + cppn_out * 0.25 * (1.0 + low_energy));

                let x = center.x + theta.cos() * disp_r;
                let y = center.y + theta.sin() * disp_r;
                ring_pts.push(egui::pos2(x, y));
            }

            let hue = (frac + time * 0.1 + beat_phase * 0.2 + self.color_palette_shift) % 1.0;
            let color = hsva_to_color32(hue, 0.9, 0.95, 0.4 + 0.6 * low_energy);

            for i in 0..ring_pts.len() - 1 {
                ui.painter().line_segment(
                    [ring_pts[i], ring_pts[i + 1]],
                    egui::Stroke::new(1.8 + frac * 2.0, color),
                );
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
