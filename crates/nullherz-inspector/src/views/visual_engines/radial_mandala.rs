//! Engine 1: radial_mandala (Hyper-Symmetric & Multi-Style Radial Mandala Engine)
//! Maps spatial pixels to Polar Coordinates (r, theta).
//! Generates multiple distinct radial mandala visual styles inspired by sacred geometry,
//! neon matrix grids, celestial pulses, floral fractals, hyper-symmetric CPPN motifs,
//! spectrum equalizer rings, wavy contour stars, nebula dust vortices, and solar flare coronas.

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
    RainbowEqualizerRing,
    WavyContourStar,
    NebulaDustVortex,
    SolarFlareCorona,
}

impl MandalaStyle {
    pub fn all() -> &'static [MandalaStyle] {
        &[
            MandalaStyle::SacredGeometry,
            MandalaStyle::NeonMatrixGrid,
            MandalaStyle::CelestialPulse,
            MandalaStyle::FloralFractal,
            MandalaStyle::HyperSymmetricCPPN,
            MandalaStyle::RainbowEqualizerRing,
            MandalaStyle::WavyContourStar,
            MandalaStyle::NebulaDustVortex,
            MandalaStyle::SolarFlareCorona,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            MandalaStyle::SacredGeometry => "Sacred Geometry (Polygons & Filigree)",
            MandalaStyle::NeonMatrixGrid => "Neon Matrix Grid (Cyberpunk Radar)",
            MandalaStyle::CelestialPulse => "Celestial Pulse (Corona & Solar Flares)",
            MandalaStyle::FloralFractal => "Floral Fractal (Blooming Petals)",
            MandalaStyle::HyperSymmetricCPPN => "Hyper-Symmetric CPPN (Log-Polar Tunnel)",
            MandalaStyle::RainbowEqualizerRing => "Rainbow Equalizer Ring (Radial Spectrum)",
            MandalaStyle::WavyContourStar => "Wavy Contour Star (Undulating Geometry)",
            MandalaStyle::NebulaDustVortex => "Nebula Dust Vortex (Cosmic Particle Swirl)",
            MandalaStyle::SolarFlareCorona => "Solar Flare Corona (Eclipse Spikes & Field)",
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
            MandalaStyle::RainbowEqualizerRing => self.render_rainbow_equalizer_ring(ui, rect, nervous, time),
            MandalaStyle::WavyContourStar => self.render_wavy_contour_star(ui, rect, nervous, time),
            MandalaStyle::NebulaDustVortex => self.render_nebula_dust_vortex(ui, rect, nervous, time),
            MandalaStyle::SolarFlareCorona => self.render_solar_flare_corona(ui, rect, nervous, time),
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

    /// Style 6 (from image.png): Circular Radial Equalizer Spectrum Ring with vibrant rainbow hue sweep.
    fn render_rainbow_equalizer_ring(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.38;
        let inner_r = max_r * 0.70;
        let num_bars = 90;

        for i in 0..num_bars {
            let frac = i as f32 / num_bars as f32;
            let theta = frac * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2 + time * 0.05;

            // Frequency response simulation along circular perimeter
            let band_val = match (i * 3) / num_bars {
                0 => nervous.low_band,
                1 => nervous.mid_band,
                _ => nervous.high_band,
            };
            let synth_bar = ((theta * 12.0 + time * 3.0).sin() * 0.5 + 0.5) * band_val + (theta * 3.0).cos().abs() * 0.3;
            let bar_len = (inner_r * 0.08) + (max_r - inner_r) * synth_bar.clamp(0.05, 1.2) * (1.0 + nervous.rms_energy * 0.4);

            let p_start = egui::pos2(center.x + theta.cos() * inner_r, center.y + theta.sin() * inner_r);
            let p_end = egui::pos2(center.x + theta.cos() * (inner_r + bar_len), center.y + theta.sin() * (inner_r + bar_len));

            // Rainbow gradient along circle (violet -> blue -> cyan -> green -> yellow -> red) matching reference image.png
            let hue = (frac * 0.85 + 0.65 + self.color_palette_shift) % 1.0;
            let color = hsva_to_color32(hue, 0.95, 1.0, 0.85 + nervous.onset_strength * 0.15);

            ui.painter().line_segment([p_start, p_end], egui::Stroke::new(2.5, color));
        }
    }

    /// Style 7 (from Screenshot From 2026-09-26 21-11-48.png): 8-pointed star mandala made of concentric undulating wave contours.
    fn render_wavy_contour_star(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.44;
        let star_points = 8.0;
        let contours = 16;

        for c in 1..=contours {
            let frac = c as f32 / contours as f32;
            let base_radius = max_r * frac;

            let num_pts = 120;
            let mut pts = Vec::with_capacity(num_pts + 1);

            for i in 0..=num_pts {
                let theta = (i as f32 / num_pts as f32) * std::f32::consts::TAU;

                // 8-star shape radial modulation + undulating wave harmonics
                let star_mod = (theta * star_points).cos();
                let wave_mod = (theta * star_points * 2.0 + time * 2.0).sin() * 0.08 * nervous.mid_band;
                let r = base_radius * (0.65 + 0.35 * star_mod + wave_mod) * (1.0 + nervous.low_band * 0.08);

                pts.push(egui::pos2(center.x + theta.cos() * r, center.y + theta.sin() * r));
            }

            // Crisp monochrome white outline matching reference style
            let alpha = 0.5 + 0.5 * frac + nervous.rms_energy * 0.2;
            let color = egui::Color32::from_rgba_unmultiplied(240, 245, 255, (alpha * 255.0) as u8);

            for i in 0..pts.len() - 1 {
                ui.painter().line_segment([pts[i], pts[i + 1]], egui::Stroke::new(1.2, color));
            }
        }

        // Intersecting outer rounded bounding arcs
        let outer_r = max_r * 0.95;
        for i in 0..4 {
            let theta = (i as f32 / 4.0) * std::f32::consts::TAU + std::f32::consts::FRAC_PI_4;
            let arc_center = egui::pos2(center.x + theta.cos() * (outer_r * 0.3), center.y + theta.sin() * (outer_r * 0.3));
            ui.painter().circle_stroke(
                arc_center,
                outer_r * 0.7,
                egui::Stroke::new(0.8, egui::Color32::from_rgba_unmultiplied(200, 220, 255, 100)),
            );
        }
    }

    /// Style 8 (from Screenshot From 2026-09-26 21-12-07.png): Cosmic dusty particle vortex with organic swirling tendrils.
    fn render_nebula_dust_vortex(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.48;

        // Swirling spiral particle dust tendrils
        let num_tendrils = 12;
        let particles_per_tendril = 35;

        for t_i in 0..num_tendrils {
            let tendril_phase = (t_i as f32 / num_tendrils as f32) * std::f32::consts::TAU;

            for p_i in 0..particles_per_tendril {
                let frac = p_i as f32 / particles_per_tendril as f32;
                let radius = max_r * frac.powf(0.8) * (1.0 + nervous.low_band * 0.15);

                let theta = tendril_phase + frac * std::f32::consts::TAU * 1.5 + time * 0.3;
                let wobble = (theta * 5.0 + time * 2.0).sin() * 8.0 * nervous.mid_band;

                let pos = egui::pos2(
                    center.x + theta.cos() * (radius + wobble),
                    center.y + theta.sin() * (radius + wobble),
                );

                // Deep dusty rose/magenta palette matching reference 21-12-07
                let hue = (0.92 + frac * 0.12 + self.color_palette_shift) % 1.0;
                let alpha = (1.0 - frac * 0.7) * (0.3 + nervous.rms_energy * 0.5);
                let color = hsva_to_color32(hue, 0.75, 0.85, alpha);

                let pt_size = (1.0 + (1.0 - frac) * 2.5 + nervous.high_band * 2.0).max(0.5);
                ui.painter().circle_filled(pos, pt_size, color);
            }
        }
    }

    /// Style 9 (from Screenshot From 2026-09-26 21-12-28.png): High-density solar flare eclipse corona with stark white core and starfield.
    fn render_solar_flare_corona(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        time: f32,
    ) {
        let center = rect.center();
        let max_r = (rect.width().min(rect.height())) * 0.45;

        // Outer background star dust particles
        let star_count = 32;
        for s in 0..star_count {
            let s_frac = s as f32 / star_count as f32;
            let star_r = max_r * (0.6 + 0.4 * ((s_frac * 17.0).fract()));
            let star_theta = s_frac * std::f32::consts::TAU + (s_frac * 3.0).sin() * 0.5;
            let star_p = egui::pos2(center.x + star_theta.cos() * star_r, center.y + star_theta.sin() * star_r);
            let twinkle = ((s_frac * 20.0 + time * 4.0).sin() * 0.5 + 0.5) * 0.8;
            ui.painter().circle_filled(
                star_p,
                1.2 + twinkle * 1.5,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, (twinkle * 220.0) as u8),
            );
        }

        // Dense radiating flare spikes matching reference 21-12-28
        let spike_count = 140;
        let inner_disc_r = max_r * 0.32;

        for i in 0..spike_count {
            let frac = i as f32 / spike_count as f32;
            let theta = frac * std::f32::consts::TAU;

            // Variable length spike spikes with 4 major corner cardinal jets
            let cardinal_boost = ((theta * 2.0).cos().abs().powf(8.0)) * 1.8;
            let noise_length = ((i as f32 * 13.37 + time * 2.0).sin() * 0.5 + 0.5).powf(1.5);

            let spike_len = (inner_disc_r * 0.15)
                + (max_r - inner_disc_r) * (0.2 + 0.3 * noise_length + cardinal_boost) * (1.0 + nervous.high_band * 0.5);

            let p1 = egui::pos2(center.x + theta.cos() * inner_disc_r, center.y + theta.sin() * inner_disc_r);
            let p2 = egui::pos2(center.x + theta.cos() * (inner_disc_r + spike_len), center.y + theta.sin() * (inner_disc_r + spike_len));

            let stroke_w = if cardinal_boost > 1.0 { 1.8 } else { 1.0 };
            ui.painter().line_segment(
                [p1, p2],
                egui::Stroke::new(stroke_w, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 220)),
            );
        }

        // Stark solid white central eclipse disc
        ui.painter().circle_filled(
            center,
            inner_disc_r,
            egui::Color32::WHITE,
        );
        ui.painter().circle_stroke(
            center,
            inner_disc_r,
            egui::Stroke::new(2.5, egui::Color32::from_rgb(220, 225, 235)),
        );
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
