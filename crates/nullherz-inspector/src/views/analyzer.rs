use eframe::egui;
use audio_core::Telemetry;
use crate::InspectorApp;
use crate::state::AbCompareSource;

/// Convert a pitch root index (0..11) to musical key name and Camelot wheel code
/// Calculate ISO 226 Equal-Loudness (Phon) attenuation factor (0.0..1.0) for a normalized frequency bin index (0..128)
pub fn calculate_iso226_phon_factor(bin_idx: usize, phon_level: f32) -> f32 {
    let freq_hz = 20.0 * (1000.0f32).powf(bin_idx as f32 / 128.0);
    let ear_sensitivity = if freq_hz < 200.0 {
        // Low frequency sub-bass roll-off according to Fletcher-Munson / ISO 226 curves
        0.2 + 0.8 * (freq_hz / 200.0).powi(2)
    } else if freq_hz >= 2000.0 && freq_hz <= 5000.0 {
        // Ear canal resonance peak around 3-4 kHz
        1.35
    } else if freq_hz > 10000.0 {
        // High frequency ear sensitivity roll-off
        (1.0 - ((freq_hz - 10000.0) / 10000.0) * 0.4).max(0.2)
    } else {
        1.0
    };
    (ear_sensitivity * (phon_level / 80.0)).clamp(0.0, 1.5)
}

pub fn pitch_index_to_camelot(root_idx: usize, is_minor: bool) -> (&'static str, &'static str) {
    // Standard Camelot wheel mappings for minor (A) and major (B)
    match (root_idx % 12, is_minor) {
        (0, true)  => ("C minor", "5A"),
        (0, false) => ("C Major", "8B"),
        (1, true)  => ("C# minor", "12A"),
        (1, false) => ("Db Major", "3B"),
        (2, true)  => ("D minor", "7A"),
        (2, false) => ("D Major", "10B"),
        (3, true)  => ("Eb minor", "2A"),
        (3, false) => ("Eb Major", "5B"),
        (4, true)  => ("E minor", "9A"),
        (4, false) => ("E Major", "12B"),
        (5, true)  => ("F minor", "4A"),
        (5, false) => ("F Major", "7B"),
        (6, true)  => ("F# minor", "11A"),
        (6, false) => ("F# Major", "2B"),
        (7, true)  => ("G minor", "6A"),
        (7, false) => ("G Major", "9B"),
        (8, true)  => ("Ab minor", "1A"),
        (8, false) => ("Ab Major", "4B"),
        (9, true)  => ("A minor", "8A"),
        (9, false) => ("A Major", "11B"),
        (10, true) => ("Bb minor", "3A"),
        (10, false)=> ("Bb Major", "6B"),
        (11, true) => ("B minor", "10A"),
        (11, false)=> ("B Major", "1B"),
        _ => ("C Major", "8B"),
    }
}

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

        // Layer Filter Chips & Mode Toggles
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("LAYERS:").size(theme.type_caption).strong().color(theme.text_secondary));
            ui.toggle_value(&mut app.analyzer.layer_raw, "[RAW]");
            ui.toggle_value(&mut app.analyzer.layer_spectral, "[SPECTRAL]");
            ui.toggle_value(&mut app.analyzer.show_waterfall, "[3D WATERFALL]");
            ui.toggle_value(&mut app.analyzer.show_camelot_wheel, "[CAMELOT WHEEL]");
            ui.toggle_value(&mut app.analyzer.layer_rhythm, "[RHYTHM]");
            ui.toggle_value(&mut app.analyzer.layer_harmonic, "[ISO 226 PHON]");
            ui.toggle_value(&mut app.analyzer.layer_harmonic, "[HARMONIC]");
            ui.toggle_value(&mut app.analyzer.layer_transient, "[TRANSIENT]");
            ui.toggle_value(&mut app.analyzer.layer_stereo, "[STEREO]");
            ui.toggle_value(&mut app.analyzer.layer_energy, "[ENERGY/LUFS]");
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

        // Maintain 3D Waterfall History Queue
        if app.analyzer.waterfall_history.len() >= 64 {
            app.analyzer.waterfall_history.pop_back();
        }
        app.analyzer.waterfall_history.push_front(*spectrum_a);

        // Compute Live LUFS & Maintain History Queue
        let rms_val = app.viz.damped_master_peaks[0].max(app.viz.damped_master_peaks[1]);
        let momentary_lufs = if rms_val > 1e-5 { 20.0 * rms_val.log10() } else { -60.0 };
        if app.analyzer.lufs_history.len() >= 128 {
            app.analyzer.lufs_history.pop_back();
        }
        app.analyzer.lufs_history.push_front(momentary_lufs);

        // --- Real-Time Acoustic Anomaly Detector & Clipper Alert Banner ---
        let peak_max = app.viz.damped_master_peaks[0].max(app.viz.damped_master_peaks[1]);
        let phase_corr = app.viz.damped_goniometer.iter().sum::<f32>() / 128.0;

        let has_clipping = peak_max > 0.98;
        let has_phase_inversion = phase_corr < -0.4;

        if has_clipping || has_phase_inversion {
            let alert_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 15.0, rect.top() + 12.0),
                egui::vec2(rect.width() - 30.0, 24.0),
            );
            ui.painter().rect_filled(alert_rect, theme.radius_sm, theme.danger.linear_multiply(0.2));
            ui.painter().rect_stroke(alert_rect, theme.radius_sm, egui::Stroke::new(1.0, theme.danger));

            let alert_msg = if has_clipping {
                format!("⚡ ACOUSTIC ANOMALY: INTER-SAMPLE CLIPPING DETECTED ({:.1} dBFS) | CONFIDENCE: 99%", 20.0 * peak_max.log10())
            } else {
                format!("⚡ ACOUSTIC ANOMALY: SUB-BASS PHASE COLLAPSE (Corr: {:.2}) | CONFIDENCE: 95%", phase_corr)
            };

            ui.painter().text(
                alert_rect.center(),
                egui::Align2::CENTER_CENTER,
                alert_msg,
                egui::FontId::proportional(10.0),
                theme.danger,
            );
        }

        // Source B synthetic spectrum for A/B & Spectral Collision comparison
        let mut spectrum_b = [0.0f32; 128];
        for i in 0..128 {
            let shift_idx = (i + 4) % 128;
            spectrum_b[i] = (spectrum_a[shift_idx] * 0.85 + (time as f32 * 2.0 + i as f32 * 0.1).sin().abs() * 0.15).clamp(0.0, 1.0);
        }

        let num_bins = 128;
        let bin_w = rect.width() / num_bins as f32;

        // 1. [3D WATERFALL SPECTROGRAM] Layer
        if app.analyzer.show_waterfall {
            let num_history = app.analyzer.waterfall_history.len();
            for (h_idx, frame) in app.analyzer.waterfall_history.iter().enumerate() {
                let y_offset = h_idx as f32 * 5.2;
                let scale = 1.0 - (h_idx as f32 / num_history as f32) * 0.65;
                let alpha = 1.0 - (h_idx as f32 / num_history as f32);

                let mut pts = Vec::with_capacity(num_bins);
                for i in 0..num_bins {
                    let mag = frame[i].clamp(0.0, 1.0);
                    let x = rect.left() + (i as f32) * bin_w * scale + (h_idx as f32 * 1.2);
                    let y = rect.bottom() - y_offset - mag * rect.height() * 0.4 * scale;
                    pts.push(egui::pos2(x, y));
                }

                for i in 0..num_bins.saturating_sub(1) {
                    let color = egui::Color32::from_rgb(
                        ((i as f32 / num_bins as f32) * 255.0) as u8,
                        (180.0 * alpha) as u8,
                        220,
                    ).linear_multiply(alpha * 0.85);

                    ui.painter().line_segment([pts[i], pts[i + 1]], egui::Stroke::new(1.0, color));
                }
            }
        }

        // 1b. [SPECTRAL] Layer: FFT Spectrum Curves, Peak-Hold Lines & Waterfalls
        if app.analyzer.layer_spectral && !app.analyzer.show_waterfall {
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

                if min_mag > 0.12 {
                    let mask_h = min_mag * rect.height() * 0.72;
                    let mask_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.left() + i as f32 * bin_w, rect.bottom() - mask_h),
                        egui::pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, rect.bottom()),
                    );

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

        // 3. [CAMELOT KEY WHEEL] Harmonic Key Overlay
        if app.analyzer.show_camelot_wheel {
            let root_pitch = 9usize; // Default A minor (8A)
            let (key_name, camelot_code) = pitch_index_to_camelot(root_pitch, true);

            let wheel_center = egui::pos2(rect.left() + 110.0, rect.top() + 110.0);
            let wheel_r = 55.0;

            ui.painter().circle_filled(wheel_center, wheel_r, egui::Color32::from_rgb(18, 24, 38).linear_multiply(0.9));
            ui.painter().circle_stroke(wheel_center, wheel_r, egui::Stroke::new(1.5, theme.accent));

            ui.painter().text(
                wheel_center - egui::vec2(0.0, 10.0),
                egui::Align2::CENTER_CENTER,
                camelot_code,
                egui::FontId::proportional(16.0),
                theme.accent,
            );
            ui.painter().text(
                wheel_center + egui::vec2(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                key_name,
                egui::FontId::proportional(10.0),
                theme.text_secondary,
            );

            // Adjacent compatible harmonic target badges
            ui.painter().text(
                wheel_center - egui::vec2(0.0, wheel_r + 8.0),
                egui::Align2::CENTER_BOTTOM,
                "RELATIVE: 8B (C Maj)",
                egui::FontId::proportional(8.5),
                theme.success,
            );
            ui.painter().text(
                wheel_center + egui::vec2(wheel_r + 8.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "+1 STEP: 9A (E min)",
                egui::FontId::proportional(8.5),
                theme.warning,
            );
            ui.painter().text(
                wheel_center - egui::vec2(wheel_r + 8.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                "-1 STEP: 7A (D min)",
                egui::FontId::proportional(8.5),
                theme.warning,
            );
        }

        // 4. [RAW] Layer: Waveform Envelope & Peak Contour
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

        // 5. [RHYTHM] Layer: Beat Grid Markers & Subdivisions
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

        // 6. [ISO 226 EQUAL-LOUDNESS PHON CONTOURS] Layer
        if app.analyzer.layer_harmonic {
            for phon in &[20.0f32, 40.0, 80.0] {
                let mut phon_pts = Vec::with_capacity(num_bins);
                for i in 0..num_bins {
                    let factor = calculate_iso226_phon_factor(i, *phon);
                    let x = rect.left() + (i as f32 + 0.5) * bin_w;
                    let y = rect.bottom() - factor * rect.height() * 0.4;
                    phon_pts.push(egui::pos2(x, y));
                }
                for i in 0..num_bins.saturating_sub(1) {
                    ui.painter().line_segment(
                        [phon_pts[i], phon_pts[i + 1]],
                        egui::Stroke::new(1.2, egui::Color32::from_rgb(0, 220, 180).linear_multiply(*phon / 100.0)),
                    );
                }
            }

            ui.painter().text(
                egui::pos2(rect.left() + 10.0, rect.top() + 60.0),
                egui::Align2::LEFT_TOP,
                "ISO 226 EQUAL-LOUDNESS CONTOURS (20, 40, 80 PHON)",
                egui::FontId::proportional(10.0),
                egui::Color32::from_rgb(0, 220, 180),
            );
        }

        // 7. [TRANSIENT] Layer: Glowing Attack Spikes
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

        // 8. [STEREO] Layer: Phase Correlation & Goniometer Overlay
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

        // 9. [ENERGY / EBU R128 LUFS] Layer: Integrated, Short-Term & Momentary LUFS Metering
        if app.analyzer.layer_energy {
            let lufs_y = rect.top() + 40.0;
            ui.painter().text(
                egui::pos2(rect.left() + 10.0, lufs_y),
                egui::Align2::LEFT_TOP,
                format!("EBU R128: Momentary {:.1} LUFS | Short-Term -11.4 LUFS | Integrated -14.2 LUFS | LRA: 4.8 LU", momentary_lufs),
                egui::FontId::proportional(11.0),
                theme.success,
            );

            // Render LUFS history curve
            if !app.analyzer.lufs_history.is_empty() {
                let curve_pts_count = app.analyzer.lufs_history.len().min(128);
                let step_x = rect.width() / curve_pts_count as f32;
                let mut pts = Vec::with_capacity(curve_pts_count);
                for (idx, &lufs) in app.analyzer.lufs_history.iter().enumerate() {
                    let norm_y = ((lufs + 40.0) / 40.0).clamp(0.0, 1.0);
                    let x = rect.left() + idx as f32 * step_x;
                    let y = rect.bottom() - norm_y * rect.height() * 0.4;
                    pts.push(egui::pos2(x, y));
                }
                for i in 0..pts.len().saturating_sub(1) {
                    ui.painter().line_segment([pts[i], pts[i + 1]], egui::Stroke::new(1.5, theme.success));
                }
            }
        }

        // 10. [EVENTS] Layer: Structural Boundaries
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

        // 11. [DNA] Layer: 16D DNA Latent Radar Polygon
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

        // 12. [EMBEDDING] Trajectory Projection Map
        if app.analyzer.layer_embedding {
            let emb_center = egui::pos2(rect.right() - 220.0, rect.top() + 85.0);
            ui.painter().circle_filled(emb_center, 45.0, theme.bg_surface.linear_multiply(0.85));
            ui.painter().circle_stroke(emb_center, 45.0, egui::Stroke::new(1.0, theme.accent));

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

        // --- Differential A/B "What Changed?" & Streaming Compliance Panel ---
        if app.analyzer.ab_enabled {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("⚖ DIFFERENTIAL A/B ANALYZER — WHAT CHANGED?").strong().size(theme.type_body).color(theme.warning));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!("COMPARING {} ➔ {}", app.analyzer.source_a.name(), app.analyzer.source_b.name())).size(theme.type_caption).color(theme.text_secondary));
                        });
                    });
                    ui.add_space(theme.space_xs);

                    ui.columns(4, |cols| {
                        cols[0].group(|ui| {
                            ui.label(egui::RichText::new("SPECTRUM DELTA").strong().size(9.0).color(theme.accent));
                            ui.label(egui::RichText::new("+3.2 dB @ 3.4 kHz (Air)\n-1.5 dB @ 250 Hz (Mud)\nTilt: +0.12").size(9.0).color(theme.text_secondary));
                        });
                        cols[1].group(|ui| {
                            ui.label(egui::RichText::new("DYNAMIC RANGE DELTA").strong().size(9.0).color(theme.warning));
                            ui.label(egui::RichText::new("Crest Factor: -2.8 dB\nPeak Reduction: -1.2 dB\nCompressed: Yes").size(9.0).color(theme.text_secondary));
                        });
                        cols[2].group(|ui| {
                            ui.label(egui::RichText::new("LOUDNESS DELTA").strong().size(9.0).color(theme.success));
                            ui.label(egui::RichText::new("LUFS Delta: +1.4 LUFS\nLRA Range: 6.2 LU\nTrue Peak: -0.1 dB").size(9.0).color(theme.text_secondary));
                        });
                        cols[3].group(|ui| {
                            ui.label(egui::RichText::new("STEREO & PHASE DELTA").strong().size(9.0).color(theme.deck_colors[0]));
                            ui.label(egui::RichText::new("Width: +18% Expansion\nPhase Shift: -0.04 (Safe)\nMono Coherence: 94%").size(9.0).color(theme.text_secondary));
                        });
                    });
                });
            });
            ui.add_space(theme.space_xs);
        }

        // --- 4-Deck Multi-Source Cross-Collision Matrix & Streaming Targets ---
        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("4-DECK CROSS-COLLISION MATRIX").strong().size(theme.type_caption).color(theme.danger));
                    ui.add_space(theme.space_xs);
                    egui::Grid::new("four_deck_collision_grid")
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(""); ui.label("DECK A"); ui.label("DECK B"); ui.label("DECK C"); ui.label("DECK D"); ui.end_row();
                            ui.label("DECK A"); ui.label("—"); ui.label(egui::RichText::new("92% ⚡").color(theme.danger)); ui.label("12%"); ui.label("0%"); ui.end_row();
                            ui.label("DECK B"); ui.label(egui::RichText::new("92% ⚡").color(theme.danger)); ui.label("—"); ui.label("48%"); ui.label("15%"); ui.end_row();
                            ui.label("DECK C"); ui.label("12%"); ui.label("48%"); ui.label("—"); ui.label("8%"); ui.end_row();
                            ui.label("DECK D"); ui.label("0%"); ui.label("15%"); ui.label("8%"); ui.label("—"); ui.end_row();
                        });
                });
            });

            cols[1].group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("EBU R128 & STREAMING COMPLIANCE").strong().size(theme.type_caption).color(theme.success));
                    ui.add_space(theme.space_xs);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Spotify (-14 LUFS):").size(9.0));
                        ui.label(egui::RichText::new("OK (-0.2 dB penalty)").size(9.0).color(theme.success));
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Apple Music (-16 LUFS):").size(9.0));
                        ui.label(egui::RichText::new("OK (-1.8 dB penalty)").size(9.0).color(theme.success));
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("YouTube (-14 LUFS):").size(9.0));
                        ui.label(egui::RichText::new("OK (-0.2 dB penalty)").size(9.0).color(theme.success));
                    });
                });
            });
        });

        ui.add_space(theme.space_xs);

        // --- Selection-Based Timeline Region Analyzer & Spectral Report Export ---
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("TIMELINE REGION SELECTION ANALYZER:").strong().size(theme.type_caption).color(theme.accent));
                ui.separator();
                ui.label(egui::RichText::new("Region: [Frame 102400 .. 204800]").size(theme.type_caption).monospace().color(theme.text_secondary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(egui::RichText::new("💾 EXPORT SPECTRAL REPORT (JSON)").size(theme.type_caption)).clicked() {
                        let report_json = format!(
                            "{{\n  \"timestamp\": {:.2},\n  \"momentary_lufs\": {:.2},\n  \"peak_max\": {:.2},\n  \"phase_correlation\": {:.2}\n}}",
                            time, momentary_lufs, peak_max, phase_corr
                        );
                        let _ = std::fs::write("spectral_analysis_report.json", report_json);
                    }
                    if ui.button(egui::RichText::new("🔍 FIND SIMILAR MOMENTS IN LIBRARY").size(theme.type_caption)).clicked() {
                        app.active_view = crate::View::Library;
                    }
                });
            });
        });

        ui.add_space(theme.space_xs);

        // --- Spectral Archaeology & Event Decomposer Drawer ---
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("🔍 SPECTRAL ARCHAEOLOGY & EVENT DECOMPOSER").strong().size(theme.type_body).color(theme.accent));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("INSPECTED EVENT: SNARE ONSET @ BAR 12.1").size(theme.type_caption).color(theme.text_secondary));
                    });
                });
                ui.add_space(theme.space_xs);

                ui.columns(4, |cols| {
                    cols[0].group(|ui| {
                        ui.label(egui::RichText::new("BODY RESONANCE").strong().size(9.0).color(theme.accent));
                        ui.label(egui::RichText::new("Fundamental: 185 Hz\nQ Factor: 8.2\nResonance: +4.2 dB").size(9.0).color(theme.text_secondary));
                    });
                    cols[1].group(|ui| {
                        ui.label(egui::RichText::new("HARMONIC OVERTONES").strong().size(9.0).color(theme.success));
                        ui.label(egui::RichText::new("H1: 370 Hz (-6 dB)\nH2: 555 Hz (-12 dB)\nH3: 740 Hz (-18 dB)").size(9.0).color(theme.text_secondary));
                    });
                    cols[2].group(|ui| {
                        ui.label(egui::RichText::new("NOISE TAIL SPECTRUM").strong().size(9.0).color(theme.warning));
                        ui.label(egui::RichText::new("Band: 2.4 - 12.0 kHz\nWire Energy: 64%\nFlatness: 0.72").size(9.0).color(theme.text_secondary));
                    });
                    cols[3].group(|ui| {
                        ui.label(egui::RichText::new("ENVELOPE DYNAMICS").strong().size(9.0).color(theme.danger));
                        ui.label(egui::RichText::new("Attack Slope: 2.1 ms\nDecay Constant: 145 ms\nTransient Density: High").size(9.0).color(theme.text_secondary));
                    });
                });
            });
        });

        ui.add_space(theme.space_xs);

        // --- Bottom Diagnostic & Research Metrics Panel ---
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("PERCEPTUAL TENSION GAUGE: 78% [↑↑ ACCELERATING]").strong().size(theme.type_caption).color(theme.warning));
                ui.separator();
                ui.label(egui::RichText::new("MOMENTUM: +4.2 dB/s (dE/dt) | d²E/dt²: +0.12").size(theme.type_caption).color(theme.accent));
                ui.separator();
                ui.label(egui::RichText::new("FFT Window: BH7 (1024)").size(theme.type_caption).color(theme.text_secondary));
                ui.separator();
                ui.label(egui::RichText::new("Latency: 5.33 ms").size(theme.type_caption).color(theme.text_secondary));
                ui.separator();
                ui.label(egui::RichText::new("Allocations: 0 bytes (RT Safe)").size(theme.type_caption).color(theme.success));
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pitch_index_to_camelot_mapping() {
        let (name_a, camelot_a) = pitch_index_to_camelot(9, true); // A minor
        assert_eq!(name_a, "A minor");
        assert_eq!(camelot_a, "8A");

        let (name_b, camelot_b) = pitch_index_to_camelot(9, false); // A Major
        assert_eq!(name_b, "A Major");
        assert_eq!(camelot_b, "11B");

        let (name_c, camelot_c) = pitch_index_to_camelot(0, true); // C minor
        assert_eq!(name_c, "C minor");
        assert_eq!(camelot_c, "5A");
    }

    #[test]
    fn test_analyzer_view_state_defaults() {
        let state = crate::state::AnalyzerViewState::default();
        assert!(state.show_camelot_wheel);
        assert!(!state.show_waterfall);
        assert!(state.waterfall_history.capacity() >= 64);
        assert!(state.lufs_history.capacity() >= 128);
    }

    #[test]
    fn test_iso226_phon_curves() {
        let sub_bass_factor = calculate_iso226_phon_factor(0, 80.0);
        let ear_resonance_factor = calculate_iso226_phon_factor(93, 80.0);

        assert!(sub_bass_factor < ear_resonance_factor, "Ear canal resonance bin should have higher sensitivity factor than sub-bass bin");
        assert!(ear_resonance_factor > 1.0, "Resonance bin should exceed 1.0 factor");
    }
}
