//! Engine 1: radial_mandala (Hyper-Symmetric CPPN & Multi-Mandala Engine)
//! Renders multiple intricate sacred geometry mandalas with concentric petal layers,
//! pointed stars, scalloped arches, and filigree loops.
//! Features 1 primary central mandala, 4 orbiting satellite mandalas, and 4 corner quarter-mandalas.
//! Driven by spiking neuron states and multi-band audio modulation.
//! Colors follow a dynamic diagonal Cyan-Blue-Violet-Magenta gradient.

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

#[derive(Copy, Clone, Debug)]
enum PetalMotif {
    Circle,
    PointedPetal,
    StarPoints,
    ScallopedArch,
    FiligreeLoop,
    GothicLancet,
}

impl NeuralVisualEngine for RadialMandalaEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, _genome: &VisualGenome) -> Vec<f32> {
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
        self.prepare_tensor_inputs(nervous, genome);

        let center = rect.center();
        let min_dim = rect.width().min(rect.height());
        let main_radius = min_dim * 0.45 * self.log_polar_depth;

        // Gradient color lookup based on canvas diagonal position
        let get_gradient_color = |pos: egui::Pos2, alpha: f32| -> egui::Color32 {
            let tx = ((pos.x - rect.min.x) / rect.width().max(1.0)).clamp(0.0, 1.0);
            let ty = ((pos.y - rect.min.y) / rect.height().max(1.0)).clamp(0.0, 1.0);
            let t = (tx * 0.5 + ty * 0.5 + (time * 0.05).sin() * 0.1).clamp(0.0, 1.0);

            // Cyan (0, 230, 255) -> Electric Blue (0, 100, 255) -> Deep Violet (150, 0, 255) -> Bright Magenta (255, 0, 200)
            let (r, g, b) = if t < 0.33 {
                let s = t / 0.33;
                (0.0, 230.0 * (1.0 - s) + 100.0 * s, 255.0)
            } else if t < 0.66 {
                let s = (t - 0.33) / 0.33;
                (150.0 * s, 100.0 * (1.0 - s), 255.0)
            } else {
                let s = (t - 0.66) / 0.34;
                (150.0 + 105.0 * s, 0.0, 255.0 - 55.0 * s)
            };

            egui::Color32::from_rgba_premultiplied(
                (r * alpha) as u8,
                (g * alpha) as u8,
                (b * alpha) as u8,
                (255.0 * alpha) as u8,
            )
        };

        // 1. Render Primary Central Mandala
        render_mandala(
            ui,
            center,
            main_radius,
            self.symmetry_n as usize,
            nervous,
            time,
            1.0,
            &get_gradient_color,
        );

        // 2. Render 4 Orbital Satellite Mandalas
        let satellite_orbit_r = main_radius * 0.72;
        let satellite_radius = main_radius * 0.32;
        let sat_symmetry = ((self.symmetry_n * 0.75) as usize).max(6);

        for i in 0..4 {
            let angle = (i as f32 * std::f32::consts::FRAC_PI_2) + (time * 0.1);
            let sat_center = egui::pos2(
                center.x + angle.cos() * satellite_orbit_r,
                center.y + angle.sin() * satellite_orbit_r,
            );

            render_mandala(
                ui,
                sat_center,
                satellite_radius,
                sat_symmetry,
                nervous,
                time * 1.3 + i as f32,
                0.75,
                &get_gradient_color,
            );
        }

        // 3. Render 4 Corner Mandalas
        let corner_radius = main_radius * 0.5;
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ];

        for (idx, corner) in corners.iter().enumerate() {
            render_mandala(
                ui,
                *corner,
                corner_radius,
                12,
                nervous,
                time * 0.8 + idx as f32,
                0.6,
                &get_gradient_color,
            );
        }
    }
}

fn render_mandala<F>(
    ui: &mut egui::Ui,
    center: egui::Pos2,
    max_radius: f32,
    symmetry: usize,
    nervous: &AudioNervousSystem,
    time: f32,
    scale_weight: f32,
    get_color: &F,
) where
    F: Fn(egui::Pos2, f32) -> egui::Color32,
{
    let bass = nervous.low_band * scale_weight;
    let mid = nervous.mid_band * scale_weight;
    let high = nervous.high_band * scale_weight;
    let pulse = (time * 2.0).sin() * 0.05 + bass * 0.15;

    // Define concentric layers from center outwards
    let layers = [
        (0.08, PetalMotif::Circle, 1, 1.5),
        (0.15, PetalMotif::PointedPetal, symmetry, 1.2),
        (0.24, PetalMotif::Circle, symmetry * 2, 1.0),
        (0.32, PetalMotif::ScallopedArch, symmetry, 1.3),
        (0.42, PetalMotif::StarPoints, symmetry * 2, 1.4),
        (0.53, PetalMotif::FiligreeLoop, symmetry, 1.2),
        (0.65, PetalMotif::PointedPetal, symmetry * 2, 1.5),
        (0.78, PetalMotif::GothicLancet, symmetry, 1.6),
        (0.90, PetalMotif::ScallopedArch, symmetry * 3, 1.3),
        (1.00, PetalMotif::StarPoints, symmetry * 2, 1.8),
    ];

    for (r_factor, motif, count, stroke_w) in layers {
        let r = max_radius * r_factor * (1.0 + pulse);
        render_motif_ring(ui, center, r, count, motif, time, bass, mid, high, stroke_w * scale_weight, get_color);
    }
}

fn render_motif_ring<F>(
    ui: &mut egui::Ui,
    center: egui::Pos2,
    radius: f32,
    count: usize,
    motif: PetalMotif,
    time: f32,
    bass: f32,
    _mid: f32,
    high: f32,
    stroke_width: f32,
    get_color: &F,
) where
    F: Fn(egui::Pos2, f32) -> egui::Color32,
{
    if count == 0 || radius <= 2.0 {
        return;
    }

    let angle_step = std::f32::consts::TAU / count as f32;

    match motif {
        PetalMotif::Circle => {
            let pts = 64;
            for i in 0..pts {
                let a1 = (i as f32 / pts as f32) * std::f32::consts::TAU;
                let a2 = ((i + 1) as f32 / pts as f32) * std::f32::consts::TAU;
                let p1 = egui::pos2(center.x + a1.cos() * radius, center.y + a1.sin() * radius);
                let p2 = egui::pos2(center.x + a2.cos() * radius, center.y + a2.sin() * radius);
                let color = get_color(p1, 0.85);
                ui.painter().line_segment([p1, p2], egui::Stroke::new(stroke_width, color));
            }
        }
        PetalMotif::PointedPetal => {
            let petal_h = radius * 0.25 * (1.0 + bass * 0.3);
            for i in 0..count {
                let angle = i as f32 * angle_step + (time * 0.05);
                let base_left = angle - angle_step * 0.45;
                let base_right = angle + angle_step * 0.45;

                let p_base_l = egui::pos2(center.x + base_left.cos() * radius, center.y + base_left.sin() * radius);
                let p_base_r = egui::pos2(center.x + base_right.cos() * radius, center.y + base_right.sin() * radius);
                let p_tip = egui::pos2(
                    center.x + angle.cos() * (radius + petal_h),
                    center.y + angle.sin() * (radius + petal_h),
                );

                let color = get_color(p_tip, 0.9);
                let stroke = egui::Stroke::new(stroke_width, color);

                // Curve to tip using quadratic bezier sampling
                draw_bezier_quad(ui, p_base_l, egui::pos2(center.x + angle.cos() * (radius + petal_h * 0.4), center.y + angle.sin() * (radius + petal_h * 0.4)), p_tip, stroke);
                draw_bezier_quad(ui, p_base_r, egui::pos2(center.x + angle.cos() * (radius + petal_h * 0.4), center.y + angle.sin() * (radius + petal_h * 0.4)), p_tip, stroke);
            }
        }
        PetalMotif::StarPoints => {
            let inner_r = radius;
            let outer_r = radius * (1.2 + high * 0.2);
            for i in 0..count {
                let a1 = i as f32 * angle_step;
                let a2 = a1 + angle_step * 0.5;
                let a3 = a1 + angle_step;

                let p1 = egui::pos2(center.x + a1.cos() * inner_r, center.y + a1.sin() * inner_r);
                let p2 = egui::pos2(center.x + a2.cos() * outer_r, center.y + a2.sin() * outer_r);
                let p3 = egui::pos2(center.x + a3.cos() * inner_r, center.y + a3.sin() * inner_r);

                let color = get_color(p2, 0.85);
                let stroke = egui::Stroke::new(stroke_width, color);
                ui.painter().line_segment([p1, p2], stroke);
                ui.painter().line_segment([p2, p3], stroke);
            }
        }
        PetalMotif::ScallopedArch => {
            for i in 0..count {
                let a1 = i as f32 * angle_step;
                let a2 = (i + 1) as f32 * angle_step;
                let a_mid = a1 + angle_step * 0.5;

                let p1 = egui::pos2(center.x + a1.cos() * radius, center.y + a1.sin() * radius);
                let p2 = egui::pos2(center.x + a2.cos() * radius, center.y + a2.sin() * radius);
                let ctrl = egui::pos2(
                    center.x + a_mid.cos() * (radius * 1.15),
                    center.y + a_mid.sin() * (radius * 1.15),
                );

                let color = get_color(ctrl, 0.8);
                draw_bezier_quad(ui, p1, ctrl, p2, egui::Stroke::new(stroke_width, color));
            }
        }
        PetalMotif::FiligreeLoop => {
            let loop_h = radius * 0.2;
            for i in 0..count {
                let angle = i as f32 * angle_step;
                let p_start = egui::pos2(center.x + angle.cos() * radius, center.y + angle.sin() * radius);
                let p_tip = egui::pos2(
                    center.x + (angle + 0.05).cos() * (radius + loop_h),
                    center.y + (angle + 0.05).sin() * (radius + loop_h),
                );
                let c1 = egui::pos2(
                    center.x + (angle - 0.15).cos() * (radius + loop_h * 0.8),
                    center.y + (angle - 0.15).sin() * (radius + loop_h * 0.8),
                );
                let c2 = egui::pos2(
                    center.x + (angle + 0.25).cos() * (radius + loop_h * 0.8),
                    center.y + (angle + 0.25).sin() * (radius + loop_h * 0.8),
                );

                let color = get_color(p_tip, 0.75);
                let stroke = egui::Stroke::new(stroke_width, color);
                draw_bezier_quad(ui, p_start, c1, p_tip, stroke);
                draw_bezier_quad(ui, p_tip, c2, p_start, stroke);
            }
        }
        PetalMotif::GothicLancet => {
            let lancet_h = radius * 0.35;
            for i in 0..count {
                let angle = i as f32 * angle_step;
                let a_left = angle - angle_step * 0.3;
                let a_right = angle + angle_step * 0.3;

                let p_l = egui::pos2(center.x + a_left.cos() * radius, center.y + a_left.sin() * radius);
                let p_r = egui::pos2(center.x + a_right.cos() * radius, center.y + a_right.sin() * radius);
                let p_tip = egui::pos2(
                    center.x + angle.cos() * (radius + lancet_h),
                    center.y + angle.sin() * (radius + lancet_h),
                );

                let color = get_color(p_tip, 0.95);
                let stroke = egui::Stroke::new(stroke_width * 1.1, color);
                ui.painter().line_segment([p_l, p_tip], stroke);
                ui.painter().line_segment([p_tip, p_r], stroke);
            }
        }
    }
}

fn draw_bezier_quad(ui: &mut egui::Ui, p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, stroke: egui::Stroke) {
    let steps = 8;
    let mut prev = p0;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let inv_t = 1.0 - t;
        let curr = egui::pos2(
            inv_t * inv_t * p0.x + 2.0 * inv_t * t * p1.x + t * t * p2.x,
            inv_t * inv_t * p0.y + 2.0 * inv_t * t * p1.y + t * t * p2.y,
        );
        ui.painter().line_segment([prev, curr], stroke);
        prev = curr;
    }
}
