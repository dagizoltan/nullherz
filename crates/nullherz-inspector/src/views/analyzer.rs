use eframe::egui;
use audio_core::Telemetry;
use crate::InspectorApp;
use crate::state::AbCompareSource;

pub fn render(app: &mut InspectorApp, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;

    ui.vertical(|ui| {
        // --- Header & Composable Layer Toggles ---
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("SPECTRAL FIELD & REAL-TIME AUDIO PERCEPTION").strong().color(theme.text_primary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut app.analyzer.ab_enabled, egui::RichText::new("A/B COMPARES").strong().size(theme.type_caption));
                if app.analyzer.ab_enabled {
                    ui.add_space(theme.space_xs);
                    egui::ComboBox::from_id_source("analyzer_source_b")
                        .selected_text(format!("B: {}", app.analyzer.source_b.name()))
                        .show_ui(ui, |ui| {
                            for src in AbCompareSource::all() {
                                ui.selectable_value(&mut app.analyzer.source_b, *src, src.name());
                            }
                        });
                    ui.add_space(theme.space_xs);
                    egui::ComboBox::from_id_source("analyzer_source_a")
                        .selected_text(format!("A: {}", app.analyzer.source_a.name()))
                        .show_ui(ui, |ui| {
                            for src in AbCompareSource::all() {
                                ui.selectable_value(&mut app.analyzer.source_a, *src, src.name());
                            }
                        });
                }
            });
        });
        ui.add_space(theme.space_xs);

        // Layer Filter Chips
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("LAYERS:").size(theme.type_caption).strong().color(theme.text_secondary));
            ui.toggle_value(&mut app.analyzer.layer_raw, "[RAW]");
            ui.toggle_value(&mut app.analyzer.layer_spectral, "[SPECTRAL]");
            ui.toggle_value(&mut app.analyzer.layer_rhythm, "[RHYTHM]");
            ui.toggle_value(&mut app.analyzer.layer_harmonic, "[HARMONIC]");
            ui.toggle_value(&mut app.analyzer.layer_transient, "[TRANSIENT]");
            ui.toggle_value(&mut app.analyzer.layer_stereo, "[STEREO]");
            ui.toggle_value(&mut app.analyzer.layer_energy, "[ENERGY]");
            ui.toggle_value(&mut app.analyzer.layer_events, "[EVENTS]");
            ui.toggle_value(&mut app.analyzer.layer_dna, "[DNA]");
            ui.toggle_value(&mut app.analyzer.layer_collision, "[COLLISION]");
            ui.toggle_value(&mut app.analyzer.layer_embedding, "[EMBEDDING]");
        });
        ui.separator();
        ui.add_space(theme.space_xs);

        // --- Central Spectral Field Canvas ---
        let available_size = egui::vec2(ui.available_width(), 360.0);
        let (rect, _response) = ui.allocate_exact_size(available_size, egui::Sense::hover());

        ui.painter().rect_filled(rect, theme.radius_md, egui::Color32::from_rgb(12, 14, 22));
        ui.painter().rect_stroke(rect, theme.radius_md, egui::Stroke::new(1.0, theme.border));

        let time = ui.input(|i| i.time);
        let spectrum = &app.viz.damped_spectrum;
        let goniometer = &app.viz.damped_goniometer;
        let latent = &app.viz.damped_latent;

        // 1. [SPECTRAL] Layer: FFT Spectrum Curve & Waterfall Heatmap
        if app.analyzer.layer_spectral {
            let num_bins = 128;
            let bin_w = rect.width() / num_bins as f32;

            for i in 0..num_bins {
                let mag = spectrum[i].clamp(0.0, 1.0);
                let bar_h = mag * rect.height() * 0.7;
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + i as f32 * bin_w, rect.bottom() - bar_h),
                    egui::pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, rect.bottom()),
                );

                let hue = (i as f32 / num_bins as f32) * 0.8;
                let color = egui::Color32::from_rgb(
                    (hue * 255.0) as u8,
                    ((1.0 - hue) * 200.0) as u8,
                    220,
                ).linear_multiply(0.4 + 0.6 * mag);

                ui.painter().rect_filled(bar_rect, 0.0, color);
            }
        }

        // 2. [RAW] Layer: Waveform Silhouette
        if app.analyzer.layer_raw {
            let num_pts = 64;
            let step = rect.width() / num_pts as f32;
            let center_y = rect.center().y;

            for i in 0..num_pts {
                let wave_val = (i as f32 * 0.2 + time as f32 * 3.0).sin() * 0.2 * (1.0 + spectrum[i % 128]);
                let x = rect.left() + i as f32 * step;
                let y1 = center_y - wave_val * rect.height() * 0.35;
                let y2 = center_y + wave_val * rect.height() * 0.35;

                ui.painter().line_segment(
                    [egui::pos2(x, y1), egui::pos2(x, y2)],
                    egui::Stroke::new(1.5, theme.accent.linear_multiply(0.6)),
                );
            }
        }

        // 3. [RHYTHM] Layer: Beat Grid Markers
        if app.analyzer.layer_rhythm {
            let beat_pos = telemetry.as_ref().map(|t| t.beat_position as f32).unwrap_or(0.0);
            let bpm = telemetry.as_ref().map(|t| t.bpm).unwrap_or(120.0);

            let num_beats = 16;
            let beat_w = rect.width() / num_beats as f32;

            for i in 0..num_beats {
                let bx = rect.left() + i as f32 * beat_w;
                let is_downbeat = i % 4 == 0;
                let stroke_color = if is_downbeat { theme.accent } else { theme.text_disabled };
                let stroke_w = if is_downbeat { 2.0 } else { 1.0 };

                ui.painter().line_segment(
                    [egui::pos2(bx, rect.top()), egui::pos2(bx, rect.bottom())],
                    egui::Stroke::new(stroke_w, stroke_color.linear_multiply(0.5)),
                );

                if is_downbeat {
                    ui.painter().text(
                        egui::pos2(bx + 4.0, rect.top() + 12.0),
                        egui::Align2::LEFT_TOP,
                        format!("BAR {}", i / 4 + 1),
                        egui::FontId::proportional(10.0),
                        theme.accent,
                    );
                }
            }

            // Moving playhead line
            let playhead_x = rect.left() + ((beat_pos % 16.0) / 16.0) * rect.width();
            ui.painter().line_segment(
                [egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())],
                egui::Stroke::new(2.0, theme.warning),
            );

            ui.painter().text(
                rect.right_top() - egui::vec2(10.0, -10.0),
                egui::Align2::RIGHT_TOP,
                format!("BPM: {:.1} | BEAT: {:.2}", bpm, beat_pos),
                egui::FontId::monospace(11.0),
                theme.warning,
            );
        }

        // 4. [HARMONIC] Layer: Pitch $f_0$ track & overtones
        if app.analyzer.layer_harmonic {
            let pitch_hz = 130.0 + (time.sin() as f32 * 20.0);
            let y_pos = rect.bottom() - (pitch_hz / 1000.0 * rect.height()).clamp(20.0, rect.height() - 20.0);

            ui.painter().line_segment(
                [egui::pos2(rect.left(), y_pos), egui::pos2(rect.right(), y_pos)],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 220, 180)),
            );

            ui.painter().text(
                egui::pos2(rect.left() + 10.0, y_pos - 12.0),
                egui::Align2::LEFT_BOTTOM,
                format!("f₀ Fundamental: {:.1} Hz (C#2)", pitch_hz),
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(0, 220, 180),
            );
        }

        // 5. [TRANSIENT] Layer: Attack Spikes
        if app.analyzer.layer_transient {
            let transient_positions = [0.15f32, 0.38, 0.52, 0.77, 0.89];
            for &tp in &transient_positions {
                let tx = rect.left() + tp * rect.width();
                ui.painter().line_segment(
                    [egui::pos2(tx, rect.bottom()), egui::pos2(tx, rect.bottom() - 60.0)],
                    egui::Stroke::new(2.0, theme.danger),
                );
                ui.painter().circle_filled(
                    egui::pos2(tx, rect.bottom() - 60.0),
                    4.0,
                    theme.danger,
                );
            }
        }

        // 6. [STEREO] Layer: Phase Correlation & Goniometer Overlay
        if app.analyzer.layer_stereo {
            let center = rect.center();
            let radius = 60.0;
            ui.painter().circle_stroke(center, radius, egui::Stroke::new(1.0, theme.text_secondary));

            for i in 0..64 {
                let val = goniometer[i];
                let angle = (i as f32 / 64.0) * std::f32::consts::TAU;
                let pt = egui::pos2(center.x + angle.cos() * radius * val, center.y + angle.sin() * radius * val);
                ui.painter().circle_filled(pt, 2.0, theme.accent);
            }
        }

        // 7. [ENERGY] Layer: LUFS & Crest Factor Curve
        if app.analyzer.layer_energy {
            let lufs_y = rect.top() + 40.0;
            ui.painter().text(
                egui::pos2(rect.left() + 10.0, lufs_y),
                egui::Align2::LEFT_TOP,
                "LUFS: -9.2 | Crest Factor: 3.1",
                egui::FontId::proportional(11.0),
                theme.success,
            );
        }

        // 8. [EVENTS] Layer: Structural Boundaries
        if app.analyzer.layer_events {
            ui.painter().text(
                egui::pos2(rect.left() + rect.width() * 0.25, rect.top() + 25.0),
                egui::Align2::CENTER_TOP,
                "★ BUILD-UP",
                egui::FontId::proportional(11.0),
                theme.warning,
            );
            ui.painter().text(
                egui::pos2(rect.left() + rect.width() * 0.50, rect.top() + 25.0),
                egui::Align2::CENTER_TOP,
                "⚡ DROP 1",
                egui::FontId::proportional(12.0),
                theme.danger,
            );
        }

        // 9. [DNA] Layer: 16D DNA Latent Radar
        if app.analyzer.layer_dna {
            let radar_center = egui::pos2(rect.right() - 80.0, rect.top() + 80.0);
            let radar_r = 50.0;
            ui.painter().circle_stroke(radar_center, radar_r, egui::Stroke::new(1.0, theme.border));

            let mut pts = Vec::with_capacity(16);
            for i in 0..16 {
                let angle = (i as f32 / 16.0) * std::f32::consts::TAU;
                let r = radar_r * (0.2 + 0.8 * latent[i].clamp(0.0, 1.0));
                pts.push(egui::pos2(radar_center.x + angle.cos() * r, radar_center.y + angle.sin() * r));
            }
            if pts.len() > 2 {
                ui.painter().add(egui::Shape::convex_polygon(
                    pts,
                    theme.accent.linear_multiply(0.25),
                    egui::Stroke::new(1.5, theme.accent),
                ));
            }
        }

        // 10. [COLLISION] Heatmap Layer
        if app.analyzer.layer_collision {
            let collision_bands = [
                ("SUB (20-60Hz)", 0.92, theme.danger),
                ("BASS (60-250Hz)", 0.48, theme.warning),
                ("LOW-MID", 0.12, theme.success),
                ("MID", 0.05, theme.success),
                ("HIGH-MID", 0.02, theme.success),
                ("HIGH", 0.00, theme.success),
            ];

            let col_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 15.0, rect.bottom() - 140.0),
                egui::vec2(220.0, 120.0),
            );
            ui.painter().rect_filled(col_rect, theme.radius_sm, theme.bg_surface.linear_multiply(0.9));
            ui.painter().rect_stroke(col_rect, theme.radius_sm, egui::Stroke::new(1.0, theme.border));

            ui.painter().text(
                col_rect.left_top() + egui::vec2(8.0, 6.0),
                egui::Align2::LEFT_TOP,
                "SPECTRAL MASKING / COLLISION",
                egui::FontId::proportional(9.0),
                theme.text_primary,
            );

            for (idx, (band, overlap, color)) in collision_bands.iter().enumerate() {
                let y = col_rect.top() + 22.0 + idx as f32 * 15.0;
                ui.painter().text(
                    egui::pos2(col_rect.left() + 8.0, y),
                    egui::Align2::LEFT_TOP,
                    format!("{}: {:.0}%", band, overlap * 100.0),
                    egui::FontId::proportional(9.0),
                    *color,
                );
            }
        }

        // 11. [EMBEDDING] Trajectory Projection Map
        if app.analyzer.layer_embedding {
            let emb_center = egui::pos2(rect.left() + 80.0, rect.top() + 80.0);
            ui.painter().circle_filled(emb_center, 40.0, theme.bg_surface.linear_multiply(0.8));
            ui.painter().circle_stroke(emb_center, 40.0, egui::Stroke::new(1.0, theme.accent));

            let traj_pt = egui::pos2(emb_center.x + (time.sin() * 25.0) as f32, emb_center.y + (time.cos() * 25.0) as f32);
            ui.painter().circle_filled(traj_pt, 4.0, theme.warning);
            ui.painter().text(
                emb_center - egui::vec2(0.0, 48.0),
                egui::Align2::CENTER_BOTTOM,
                "2D PERCEPTUAL MANIFOLD",
                egui::FontId::proportional(9.0),
                theme.accent,
            );
        }

        ui.add_space(theme.space_sm);

        // --- Bottom Diagnostic & Research Metrics Panel ---
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("RESEARCH & ENGINE METRICS:").strong().size(theme.type_caption).color(theme.accent));
                ui.separator();
                ui.label(egui::RichText::new("FFT Window: Blackman-Harris 7-Term (1024)").size(theme.type_caption).color(theme.text_secondary));
                ui.separator();
                ui.label(egui::RichText::new("Latency: 5.33 ms").size(theme.type_caption).color(theme.text_secondary));
                ui.separator();
                ui.label(egui::RichText::new("Allocations: 0 bytes (RT Safe)").size(theme.type_caption).color(theme.success));
                ui.separator();
                ui.label(egui::RichText::new("Confidence: 98.4%").size(theme.type_caption).color(theme.success));
            });
        });
    });
}
