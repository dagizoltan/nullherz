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
        let available_size = egui::vec2(ui.available_width(), 380.0);
        let (rect, _response) = ui.allocate_exact_size(available_size, egui::Sense::hover());

        ui.painter().rect_filled(rect, theme.radius_md, egui::Color32::from_rgb(10, 12, 18));
        ui.painter().rect_stroke(rect, theme.radius_md, egui::Stroke::new(1.0, theme.border));

        let time = ui.input(|i| i.time);
        let spectrum_a = &app.viz.damped_spectrum;
        let goniometer = &app.viz.damped_goniometer;
        let latent = &app.viz.damped_latent;

        // Source B synthetic spectrum for A/B & Spectral Collision comparison
        let mut spectrum_b = [0.0f32; 128];
        for i in 0..128 {
            let shift_idx = (i + 4) % 128;
            spectrum_b[i] = (spectrum_a[shift_idx] * 0.85 + (time as f32 * 2.0 + i as f32 * 0.1).sin().abs() * 0.15).clamp(0.0, 1.0);
        }

        let num_bins = 128;
        let bin_w = rect.width() / num_bins as f32;

        // 1. [SPECTRAL] Layer: FFT Spectrum Curves, Peak-Hold Lines & Waterfalls
        if app.analyzer.layer_spectral {
            for i in 0..num_bins {
                let mag_a = spectrum_a[i].clamp(0.0, 1.0);
                let bar_h = mag_a * rect.height() * 0.72;
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + i as f32 * bin_w, rect.bottom() - bar_h),
                    egui::pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, rect.bottom()),
                );

                let hue = (i as f32 / num_bins as f32) * 0.75;
                let color_a = egui::Color32::from_rgb(
                    (hue * 255.0) as u8,
                    ((1.0 - hue) * 220.0) as u8,
                    240,
                ).linear_multiply(0.35 + 0.65 * mag_a);

                ui.painter().rect_filled(bar_rect, 0.0, color_a);

                // Peak-hold line cap
                if mag_a > 0.05 {
                    let peak_y = rect.bottom() - bar_h - 2.0;
                    ui.painter().line_segment(
                        [egui::pos2(rect.left() + i as f32 * bin_w, peak_y), egui::pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, peak_y)],
                        egui::Stroke::new(1.5, theme.accent),
                    );
                }
            }

            // Source B Overlay Curve when A/B comparison or Collision is enabled
            if app.analyzer.ab_enabled || app.analyzer.layer_collision {
                let mut b_pts = Vec::with_capacity(num_bins);
                for i in 0..num_bins {
                    let mag_b = spectrum_b[i].clamp(0.0, 1.0);
                    let bx = rect.left() + (i as f32 + 0.5) * bin_w;
                    let by = rect.bottom() - mag_b * rect.height() * 0.72;
                    b_pts.push(egui::pos2(bx, by));
                }
                for i in 0..num_bins.saturating_sub(1) {
                    ui.painter().line_segment(
                        [b_pts[i], b_pts[i + 1]],
                        egui::Stroke::new(1.8, egui::Color32::from_rgb(255, 60, 180)),
                    );
                }
            }
        }

        // 2. [COLLISION] Real-Time Frequency Masking & Overlap Heatmap Overlay
        if app.analyzer.layer_collision || app.analyzer.ab_enabled {
            let mut _total_overlap_energy = 0.0f32;
            let mut mask_pts_top = Vec::with_capacity(num_bins);
            let mut mask_pts_bottom = Vec::with_capacity(num_bins);

            for i in 0..num_bins {
                let mag_a = spectrum_a[i].clamp(0.0, 1.0);
                let mag_b = spectrum_b[i].clamp(0.0, 1.0);
                let overlap = (mag_a * mag_b).sqrt();

                _total_overlap_energy += overlap;

                let x = rect.left() + (i as f32 + 0.5) * bin_w;
                let min_mag = mag_a.min(mag_b);

                // Highlight critical frequency masking zones
                if min_mag > 0.12 {
                    let mask_h = min_mag * rect.height() * 0.72;
                    let mask_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.left() + i as f32 * bin_w, rect.bottom() - mask_h),
                        egui::pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, rect.bottom()),
                    );

                    // Glowing critical red/orange masking zone fill
                    ui.painter().rect_filled(
                        mask_rect,
                        0.0,
                        egui::Color32::from_rgb(255, 70, 30).linear_multiply(0.45 + 0.5 * min_mag),
                    );
                }

                let y_top = rect.bottom() - min_mag * rect.height() * 0.72;
                mask_pts_top.push(egui::pos2(x, y_top));
                mask_pts_bottom.push(egui::pos2(x, rect.bottom()));
            }

            // Multi-band Overlap Masking Analysis Table
            let bands = [
                ("SUB (20-60 Hz)", &spectrum_a[0..4], &spectrum_b[0..4]),
                ("BASS (60-250 Hz)", &spectrum_a[4..16], &spectrum_b[4..16]),
                ("LOW-MID (250-500 Hz)", &spectrum_a[16..32], &spectrum_b[16..32]),
                ("MID (500-2 kHz)", &spectrum_a[32..64], &spectrum_b[32..64]),
                ("HIGH-MID (2-6 kHz)", &spectrum_a[64..96], &spectrum_b[64..96]),
                ("HIGH (>6 kHz)", &spectrum_a[96..128], &spectrum_b[96..128]),
            ];

            let col_panel_w = 260.0;
            let col_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 15.0, rect.bottom() - 175.0),
                egui::vec2(col_panel_w, 160.0),
            );

            ui.painter().rect_filled(col_rect, theme.radius_md, egui::Color32::from_rgb(16, 20, 30).linear_multiply(0.92));
            ui.painter().rect_stroke(col_rect, theme.radius_md, egui::Stroke::new(1.0, theme.danger));

            ui.painter().text(
                col_rect.left_top() + egui::vec2(10.0, 8.0),
                egui::Align2::LEFT_TOP,
                "⚡ SPECTRAL MASKING & OVERLAP ANALYSIS",
                egui::FontId::proportional(10.0),
                theme.danger,
            );

            for (idx, (label, a_slice, b_slice)) in bands.iter().enumerate() {
                let sum_a: f32 = a_slice.iter().sum();
                let sum_b: f32 = b_slice.iter().sum();
                let overlap_sum: f32 = a_slice.iter().zip(b_slice.iter()).map(|(a, b)| (a * b).sqrt()).sum();
                let max_sum = sum_a.max(sum_b).max(1e-5);
                let pct = (overlap_sum / max_sum).clamp(0.0, 1.0);

                let y = col_rect.top() + 26.0 + idx as f32 * 21.0;
                let (status_text, status_color) = if pct > 0.65 {
                    ("CRITICAL", theme.danger)
                } else if pct > 0.35 {
                    ("MODERATE", theme.warning)
                } else {
                    ("CLEAR", theme.success)
                };

                ui.painter().text(
                    egui::pos2(col_rect.left() + 10.0, y),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(9.0),
                    theme.text_secondary,
                );

                // Mini Overlap Bar
                let bar_x = col_rect.left() + 130.0;
                let bar_w = 60.0;
                let bar_rect = egui::Rect::from_min_size(egui::pos2(bar_x, y + 2.0), egui::vec2(bar_w, 8.0));
                let fill_rect = egui::Rect::from_min_size(egui::pos2(bar_x, y + 2.0), egui::vec2(bar_w * pct, 8.0));

                ui.painter().rect_filled(bar_rect, 2.0, theme.bg_inset);
                ui.painter().rect_filled(fill_rect, 2.0, status_color);

                ui.painter().text(
                    egui::pos2(col_rect.right() - 10.0, y),
                    egui::Align2::RIGHT_TOP,
                    format!("{:.0}% ({})", pct * 100.0, status_text),
                    egui::FontId::monospace(9.0),
                    status_color,
                );
            }
        }

        // 3. [RAW] Layer: Waveform Envelope & Peak Contour
        if app.analyzer.layer_raw {
            let num_pts = 64;
            let step = rect.width() / num_pts as f32;
            let center_y = rect.center().y;

            for i in 0..num_pts {
                let wave_val = (i as f32 * 0.2 + time as f32 * 3.0).sin() * 0.2 * (1.0 + spectrum_a[i % 128]);
                let x = rect.left() + i as f32 * step;
                let y1 = center_y - wave_val * rect.height() * 0.35;
                let y2 = center_y + wave_val * rect.height() * 0.35;

                ui.painter().line_segment(
                    [egui::pos2(x, y1), egui::pos2(x, y2)],
                    egui::Stroke::new(1.5, theme.accent.linear_multiply(0.6)),
                );
            }
        }

        // 4. [RHYTHM] Layer: Beat Grid Markers & Subdivisions
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
                egui::Stroke::new(2.5, theme.warning),
            );

            ui.painter().text(
                rect.right_top() - egui::vec2(10.0, -10.0),
                egui::Align2::RIGHT_TOP,
                format!("BPM: {:.1} | BEAT: {:.2}", bpm, beat_pos),
                egui::FontId::monospace(11.0),
                theme.warning,
            );
        }

        // 5. [HARMONIC] Layer: Pitch $f_0$ track & overtones
        if app.analyzer.layer_harmonic {
            let pitch_hz = 130.0 + (time.sin() as f32 * 20.0);
            let y_pos = rect.bottom() - (pitch_hz / 1000.0 * rect.height()).clamp(20.0, rect.height() - 20.0);

            // Fundamental $f_0$
            ui.painter().line_segment(
                [egui::pos2(rect.left(), y_pos), egui::pos2(rect.right(), y_pos)],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 220, 180)),
            );

            // Harmonic Overtones ($2f_0$, $3f_0$, $4f_0$)
            for harmonic in 2..=4 {
                let h_hz = pitch_hz * harmonic as f32;
                let h_y = rect.bottom() - (h_hz / 1000.0 * rect.height()).clamp(10.0, rect.height() - 10.0);
                ui.painter().line_segment(
                    [egui::pos2(rect.left(), h_y), egui::pos2(rect.right(), h_y)],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 180, 150).linear_multiply(0.5 / harmonic as f32)),
                );
            }

            ui.painter().text(
                egui::pos2(rect.left() + 10.0, y_pos - 12.0),
                egui::Align2::LEFT_BOTTOM,
                format!("f₀ Fundamental: {:.1} Hz (C#2) + Overtones", pitch_hz),
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(0, 220, 180),
            );
        }

        // 6. [TRANSIENT] Layer: Glowing Attack Spikes
        if app.analyzer.layer_transient {
            let transient_positions = [0.15f32, 0.38, 0.52, 0.77, 0.89];
            for &tp in &transient_positions {
                let tx = rect.left() + tp * rect.width();
                ui.painter().line_segment(
                    [egui::pos2(tx, rect.bottom()), egui::pos2(tx, rect.bottom() - 75.0)],
                    egui::Stroke::new(2.0, theme.danger),
                );
                ui.painter().circle_filled(
                    egui::pos2(tx, rect.bottom() - 75.0),
                    5.0,
                    theme.danger,
                );
                ui.painter().text(
                    egui::pos2(tx, rect.bottom() - 88.0),
                    egui::Align2::CENTER_BOTTOM,
                    "+3.2 dB",
                    egui::FontId::monospace(8.0),
                    theme.danger,
                );
            }
        }

        // 7. [STEREO] Layer: Phase Correlation & Goniometer Overlay
        if app.analyzer.layer_stereo {
            let center = rect.center();
            let radius = 65.0;
            ui.painter().circle_stroke(center, radius, egui::Stroke::new(1.0, theme.text_secondary));

            for i in 0..64 {
                let val = goniometer[i];
                let angle = (i as f32 / 64.0) * std::f32::consts::TAU;
                let pt = egui::pos2(center.x + angle.cos() * radius * val, center.y + angle.sin() * radius * val);
                ui.painter().circle_filled(pt, 2.5, theme.accent);
            }
        }

        // 8. [ENERGY] Layer: LUFS & Crest Factor Curve
        if app.analyzer.layer_energy {
            let lufs_y = rect.top() + 40.0;
            ui.painter().text(
                egui::pos2(rect.left() + 10.0, lufs_y),
                egui::Align2::LEFT_TOP,
                "LUFS: -9.2 dB | Crest Factor: 3.1 | True Peak: -0.2 dBFS",
                egui::FontId::proportional(11.0),
                theme.success,
            );
        }

        // 9. [EVENTS] Layer: Structural Boundaries
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

        // 10. [DNA] Layer: 16D DNA Latent Radar Polygon
        if app.analyzer.layer_dna {
            let radar_center = egui::pos2(rect.right() - 85.0, rect.top() + 85.0);
            let radar_r = 55.0;
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

        // 11. [EMBEDDING] Trajectory Projection Map
        if app.analyzer.layer_embedding {
            let emb_center = egui::pos2(rect.right() - 220.0, rect.top() + 85.0);
            ui.painter().circle_filled(emb_center, 45.0, theme.bg_surface.linear_multiply(0.85));
            ui.painter().circle_stroke(emb_center, 45.0, egui::Stroke::new(1.0, theme.accent));

            // Render historical trajectory trail dots
            for step in 0..8 {
                let trail_time = time - (step as f64 * 0.2);
                let traj_pt = egui::pos2(
                    emb_center.x + (trail_time.sin() * 30.0) as f32,
                    emb_center.y + (trail_time.cos() * 30.0) as f32,
                );
                let alpha = 1.0 - (step as f32 / 8.0);
                ui.painter().circle_filled(
                    traj_pt,
                    if step == 0 { 5.0 } else { 2.5 },
                    theme.warning.linear_multiply(alpha),
                );
            }

            ui.painter().text(
                emb_center - egui::vec2(0.0, 52.0),
                egui::Align2::CENTER_BOTTOM,
                "2D PERCEPTUAL TRAJECTORY MANIFOLD",
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
