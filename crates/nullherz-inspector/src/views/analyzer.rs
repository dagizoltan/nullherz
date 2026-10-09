use egui::{Ui, Frame, RichText, Stroke, Color32, Margin, Vec2, Sense, Rect, pos2, Align2, FontId};
use audio_core::Telemetry;
use nullherz_ui_hal::Theme;
use crate::InspectorApp;
use crate::state::AnalyzerMode;

/// Calculate ISO 226 Equal-Loudness (Phon) attenuation factor (0.0..1.0) for a normalized frequency bin index (0..128)
pub fn calculate_iso226_phon_factor(bin_idx: usize, phon_level: f32) -> f32 {
    let freq_hz = 20.0 * (1000.0f32).powf(bin_idx as f32 / 128.0);
    let ear_sensitivity = if freq_hz < 200.0 {
        0.2 + 0.8 * (freq_hz / 200.0).powi(2)
    } else if freq_hz >= 2000.0 && freq_hz <= 5000.0 {
        1.35
    } else if freq_hz > 10000.0 {
        (1.0 - ((freq_hz - 10000.0) / 10000.0) * 0.4).max(0.2)
    } else {
        1.0
    };
    (ear_sensitivity * (phon_level / 80.0)).clamp(0.0, 1.0)
}

/// Convert a pitch root index (0..11) to musical key name and Camelot wheel code
pub fn pitch_index_to_camelot(root_idx: usize, is_minor: bool) -> (&'static str, &'static str) {
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

/// Convert frequency in Hz to closest musical note name (e.g. 440 Hz -> "A4")
pub fn hz_to_note_name(freq_hz: f32) -> String {
    if freq_hz <= 0.0 {
        return "---".to_string();
    }
    let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let midi_note = (12.0 * (freq_hz / 440.0).log2() + 69.0).round() as i32;
    if midi_note < 0 || midi_note > 127 {
        return format!("{:.0} Hz", freq_hz);
    }
    let note = note_names[(midi_note.rem_euclid(12)) as usize];
    let octave = (midi_note / 12) - 1;
    format!("{} ({}{})", format_freq_hz(freq_hz), note, octave)
}

fn format_freq_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.1} kHz", hz / 1000.0)
    } else {
        format!("{:.0} Hz", hz)
    }
}

/// Helper function to render styled filter pills
fn render_filter_pill(ui: &mut Ui, theme: &Theme, label: &str, active: &mut bool) {
    let bg_color = if *active { theme.accent.linear_multiply(0.2) } else { theme.bg_inset };
    let text_color = if *active { theme.accent } else { theme.text_secondary };
    let border_stroke = if *active { Stroke::new(1.0_f32, theme.accent) } else { Stroke::new(1.0_f32, theme.border_stroke.color) };

    let btn = egui::Button::new(RichText::new(label).size(9.0).strong().color(text_color))
        .fill(bg_color)
        .stroke(border_stroke);

    if ui.add(btn).clicked() {
        *active = !*active;
    }
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;

    egui::ScrollArea::vertical()
        .id_source("analyzer_scroll_area")
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // --- Studio Mode Selector Header Card ---
                Frame::none()
                    .fill(theme.bg_surface)
                    .rounding(theme.radius_md)
                    .stroke(Stroke::new(1.0_f32, theme.border))
                    .inner_margin(Margin::symmetric(theme.space_md, theme.space_xs))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let rt_active = app.analyzer.mode == AnalyzerMode::RealTime;
                            let ft_active = app.analyzer.mode == AnalyzerMode::FullTrack;

                            let rt_btn = egui::Button::new(
                                RichText::new(format!("{} REAL-TIME PERCEPTION", egui_phosphor::regular::LIGHTNING))
                                    .strong()
                                    .size(theme.type_body)
                                    .color(if rt_active { Color32::BLACK } else { theme.text_primary })
                            ).fill(if rt_active { theme.accent } else { theme.bg_inset });

                            if ui.add(rt_btn).clicked() {
                                app.analyzer.mode = AnalyzerMode::RealTime;
                            }

                            let ft_btn = egui::Button::new(
                                RichText::new(format!("{} FULL-TRACK PASSPORT", egui_phosphor::regular::CHART_PIE))
                                    .strong()
                                    .size(theme.type_body)
                                    .color(if ft_active { Color32::BLACK } else { theme.text_primary })
                            ).fill(if ft_active { theme.accent } else { theme.bg_inset });

                            if ui.add(ft_btn).clicked() {
                                app.analyzer.mode = AnalyzerMode::FullTrack;
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if app.analyzer.mode == AnalyzerMode::RealTime {
                                    let ab_bg = if app.analyzer.ab_enabled { theme.warning } else { theme.bg_inset };
                                    let ab_fg = if app.analyzer.ab_enabled { Color32::BLACK } else { theme.text_secondary };
                                    if ui.add(egui::Button::new(RichText::new("A/B COMPARES").strong().size(theme.type_caption).color(ab_fg)).fill(ab_bg)).clicked() {
                                        app.analyzer.ab_enabled = !app.analyzer.ab_enabled;
                                    }
                                }
                            });
                        });
                    });

                ui.add_space(theme.space_xs);

                match app.analyzer.mode {
                    AnalyzerMode::RealTime => render_realtime_screen(app, ui, telemetry),
                    AnalyzerMode::FullTrack => render_full_track_screen(app, ui, telemetry),
                }
            });
        });
}

/// Screen 1: Real-time perception and streaming acoustic field
fn render_realtime_screen(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;

    // Layer Filter Chips in Clean Studio Card
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_sm)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .inner_margin(Margin::symmetric(theme.space_md, theme.space_xs))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                ui.label(RichText::new("DISPLAY:").size(theme.type_caption).strong().color(theme.accent));
                render_filter_pill(ui, &theme, "SPECTRAL", &mut app.analyzer.layer_spectral);
                render_filter_pill(ui, &theme, "3D WATERFALL", &mut app.analyzer.show_waterfall);
                render_filter_pill(ui, &theme, "RAW", &mut app.analyzer.layer_raw);

                ui.add_space(10.0);
                ui.label(RichText::new("PERCEPTION:").size(theme.type_caption).strong().color(theme.accent));
                render_filter_pill(ui, &theme, "CAMELOT", &mut app.analyzer.show_camelot_wheel);
                render_filter_pill(ui, &theme, "ISO 226 PHON", &mut app.analyzer.layer_harmonic);
                render_filter_pill(ui, &theme, "RHYTHM", &mut app.analyzer.layer_rhythm);
                render_filter_pill(ui, &theme, "TRANSIENT", &mut app.analyzer.layer_transient);

                ui.add_space(10.0);
                ui.label(RichText::new("METRICS:").size(theme.type_caption).strong().color(theme.accent));
                render_filter_pill(ui, &theme, "LUFS", &mut app.analyzer.layer_energy);
                render_filter_pill(ui, &theme, "STEREO VECTORSCOPE", &mut app.analyzer.layer_stereo);
                render_filter_pill(ui, &theme, "COLLISION", &mut app.analyzer.layer_collision);
            });
        });

    ui.add_space(theme.space_xs);

    // Compute Live LUFS & Maintain History Queue before UI layout
    let spectrum_a = &app.viz.damped_spectrum;
    let goniometer = &app.viz.damped_goniometer;

    if app.analyzer.waterfall_history.len() >= 64 {
        app.analyzer.waterfall_history.pop_back();
    }
    app.analyzer.waterfall_history.push_front(*spectrum_a);

    let rms_val = app.viz.damped_master_peaks[0].max(app.viz.damped_master_peaks[1]);
    let momentary_lufs = if rms_val > 1e-5 { 20.0 * rms_val.log10() } else { -60.0 };
    if app.analyzer.lufs_history.len() >= 128 {
        app.analyzer.lufs_history.pop_back();
    }
    app.analyzer.lufs_history.push_front(momentary_lufs);

    let short_term_lufs = if !app.analyzer.lufs_history.is_empty() {
        let count = app.analyzer.lufs_history.len().min(30);
        app.analyzer.lufs_history.iter().take(count).sum::<f32>() / count as f32
    } else {
        momentary_lufs
    };

    let integrated_lufs = if !app.analyzer.lufs_history.is_empty() {
        app.analyzer.lufs_history.iter().sum::<f32>() / app.analyzer.lufs_history.len() as f32
    } else {
        momentary_lufs
    };

    let min_lufs = app.analyzer.lufs_history.iter().copied().fold(0.0f32, f32::min);
    let max_lufs = app.analyzer.lufs_history.iter().copied().fold(-60.0f32, f32::max);
    let loudness_range_lra = (max_lufs - min_lufs).abs().clamp(0.0, 30.0);

    // --- Central Layout: Spectral Field + Interactive Vectorscope ---
    let total_width = ui.available_width();
    let show_vectorscope = app.analyzer.layer_stereo;
    let main_canvas_w = if show_vectorscope { (total_width - 220.0).max(300.0) } else { total_width };

    ui.horizontal(|ui| {
        // --- Central Spectral Field Canvas ---
        let available_size = Vec2::new(main_canvas_w, 320.0);
        let (rect, response) = ui.allocate_exact_size(available_size, Sense::hover());

        ui.painter().rect_filled(rect, theme.radius_md, Color32::from_rgb(10, 12, 18));
        ui.painter().rect_stroke(rect, theme.radius_md, Stroke::new(1.0_f32, theme.border));

        // Background Frequency Gridlines & Labels
        let freq_ticks = [(0, "20 Hz"), (32, "200 Hz"), (64, "1 kHz"), (96, "5 kHz"), (127, "20 kHz")];
        for (bin_idx, label) in freq_ticks {
            let x = rect.left() + (bin_idx as f32 / 128.0) * rect.width();
            ui.painter().line_segment(
                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                Stroke::new(1.0_f32, theme.border_stroke.color.linear_multiply(0.3)),
            );
            ui.painter().text(
                pos2(x + 2.0, rect.bottom() - 14.0),
                Align2::LEFT_BOTTOM,
                label,
                FontId::proportional(8.0),
                theme.text_secondary,
            );
        }

        // dBFS Amplitude Y-Axis Gridlines
        let db_ticks = [(-60.0, " -60 dBFS"), (-36.0, " -36 dBFS"), (-12.0, " -12 dBFS"), (0.0, " 0 dBFS")];
        for (db_val, label) in db_ticks {
            let amp = (10.0f32).powf(db_val / 20.0);
            let y = rect.bottom() - amp * rect.height() * 0.65;
            if y >= rect.top() && y <= rect.bottom() {
                ui.painter().line_segment(
                    [pos2(rect.left(), y), pos2(rect.right(), y)],
                    Stroke::new(1.0_f32, theme.border_stroke.color.linear_multiply(0.2)),
                );
                ui.painter().text(
                    pos2(rect.right() - 4.0, y - 2.0),
                    Align2::RIGHT_BOTTOM,
                    label,
                    FontId::proportional(8.0),
                    theme.text_secondary,
                );
            }
        }

        // --- Real-Time Acoustic Anomaly Detector Banner ---
        let peak_max = app.viz.damped_master_peaks[0].max(app.viz.damped_master_peaks[1]);
        let phase_corr = goniometer.iter().sum::<f32>() / 128.0;

        let has_clipping = peak_max > 0.98;
        let has_phase_inversion = phase_corr < -0.4;

        if has_clipping || has_phase_inversion {
            let alert_rect = Rect::from_min_size(
                pos2(rect.left() + 15.0, rect.top() + 8.0),
                Vec2::new(rect.width() - 30.0, 22.0),
            );
            ui.painter().rect_filled(alert_rect, theme.radius_sm, theme.danger.linear_multiply(0.2));
            ui.painter().rect_stroke(alert_rect, theme.radius_sm, Stroke::new(1.0_f32, theme.danger));

            let alert_msg = if has_clipping {
                format!("⚡ ACOUSTIC ANOMALY: INTER-SAMPLE CLIPPING DETECTED ({:.1} dBFS)", 20.0 * peak_max.log10())
            } else {
                format!("⚡ ACOUSTIC ANOMALY: SUB-BASS PHASE COLLAPSE (Corr: {:.2})", phase_corr)
            };

            ui.painter().text(
                alert_rect.center(),
                Align2::CENTER_CENTER,
                alert_msg,
                FontId::proportional(9.5),
                theme.danger,
            );
        }

        let num_bins = 128;
        let bin_w = rect.width() / num_bins as f32;

        // 1. [3D WATERFALL SPECTROGRAM] Layer
        if app.analyzer.show_waterfall {
            let num_history = app.analyzer.waterfall_history.len();
            for (h_idx, frame) in app.analyzer.waterfall_history.iter().enumerate() {
                let y_offset = h_idx as f32 * 4.2;
                let scale = 1.0 - (h_idx as f32 / num_history as f32) * 0.65;
                let alpha = 1.0 - (h_idx as f32 / num_history as f32);

                let mut pts = Vec::with_capacity(num_bins);
                for i in 0..num_bins {
                    let mag = frame[i].clamp(0.0, 1.0);
                    let x = rect.left() + (i as f32) * bin_w * scale + (h_idx as f32 * 1.0);
                    let y = (rect.bottom() - y_offset - mag * rect.height() * 0.35 * scale).clamp(rect.top(), rect.bottom());
                    pts.push(pos2(x, y));
                }

                for i in 0..num_bins.saturating_sub(1) {
                    let color = Color32::from_rgb(
                        ((i as f32 / num_bins as f32) * 255.0) as u8,
                        (180.0 * alpha) as u8,
                        220,
                    ).linear_multiply(alpha * 0.85);

                    ui.painter().line_segment([pts[i], pts[i + 1]], Stroke::new(1.0_f32, color));
                }
            }
        }

        // 1b. [SPECTRAL] Layer
        if app.analyzer.layer_spectral && !app.analyzer.show_waterfall {
            for i in 0..num_bins {
                let mag_a = spectrum_a[i].clamp(0.0, 1.0);
                let bar_h = mag_a * rect.height() * 0.65;
                let bar_rect = Rect::from_min_max(
                    pos2(rect.left() + i as f32 * bin_w, rect.bottom() - bar_h),
                    pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, rect.bottom()),
                );

                let hue = (i as f32 / num_bins as f32) * 0.75;
                let color_a = Color32::from_rgb(
                    (hue * 255.0) as u8,
                    ((1.0 - hue) * 220.0) as u8,
                    240,
                ).linear_multiply(0.35 + 0.65 * mag_a);

                ui.painter().rect_filled(bar_rect, 0.0, color_a);

                if mag_a > 0.05 {
                    let peak_y = (rect.bottom() - bar_h - 2.0).clamp(rect.top(), rect.bottom());
                    ui.painter().line_segment(
                        [pos2(rect.left() + i as f32 * bin_w, peak_y), pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, peak_y)],
                        Stroke::new(1.5_f32, theme.accent),
                    );
                }
            }
        }

        // --- HOVER FREQUENCY / NOTE INSPECTION OVERLAY ---
        if response.hovered() {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pos) {
                    let bin_idx = (((pos.x - rect.left()) / rect.width()) * num_bins as f32).clamp(0.0, 127.0) as usize;
                    let freq_hz = 20.0 * (1000.0f32).powf(bin_idx as f32 / 128.0);
                    let note_str = hz_to_note_name(freq_hz);
                    let amp_val = spectrum_a[bin_idx].clamp(0.0001, 1.0);
                    let db_val = 20.0 * amp_val.log10();

                    // Draw vertical guideline
                    ui.painter().line_segment(
                        [pos2(pos.x, rect.top()), pos2(pos.x, rect.bottom())],
                        Stroke::new(1.0_f32, theme.accent.linear_multiply(0.6)),
                    );

                    // Draw hover tooltip pill
                    let info_text = format!("{} | {:.1} dBFS", note_str, db_val);
                    let pill_pos = pos2(
                        (pos.x + 10.0).clamp(rect.left() + 5.0, rect.right() - 150.0),
                        (pos.y - 25.0).clamp(rect.top() + 5.0, rect.bottom() - 25.0),
                    );
                    let pill_rect = Rect::from_min_size(pill_pos, Vec2::new(140.0, 20.0));
                    ui.painter().rect_filled(pill_rect, theme.radius_sm, Color32::from_rgb(20, 28, 42));
                    ui.painter().rect_stroke(pill_rect, theme.radius_sm, Stroke::new(1.0_f32, theme.accent));
                    ui.painter().text(
                        pill_rect.center(),
                        Align2::CENTER_CENTER,
                        info_text,
                        FontId::proportional(10.0),
                        theme.accent,
                    );
                }
            }
        }

        // 3. [CAMELOT KEY WHEEL] Overlay
        if app.analyzer.show_camelot_wheel {
            let focused_deck = app.decks.focused_deck.min(15);
            let track_key = app.decks.now_playing[focused_deck]
                .and_then(|id| app.get_cached_track(id))
                .and_then(|t| t.metadata.root_key);

            let root_pitch = track_key.map(|k| k as usize % 12).unwrap_or_else(|| {
                spectrum_a.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(idx, _)| idx % 12).unwrap_or(9)
            });

            let (key_name, camelot_code) = pitch_index_to_camelot(root_pitch, true);

            let wheel_center = pos2(rect.left() + 85.0, rect.top() + 85.0);
            let wheel_r = 45.0;

            ui.painter().circle_filled(wheel_center, wheel_r, Color32::from_rgb(18, 24, 38).linear_multiply(0.92));
            ui.painter().circle_stroke(wheel_center, wheel_r, Stroke::new(1.5_f32, theme.accent));

            ui.painter().text(
                wheel_center - Vec2::new(0.0, 8.0),
                Align2::CENTER_CENTER,
                camelot_code,
                FontId::proportional(14.0),
                theme.accent,
            );
            ui.painter().text(
                wheel_center + Vec2::new(0.0, 10.0),
                Align2::CENTER_CENTER,
                key_name,
                FontId::proportional(9.0),
                theme.text_secondary,
            );
        }

        // 5. [RHYTHM] Beat Grid
        if app.analyzer.layer_rhythm {
            let beat_pos = telemetry.as_ref().map(|t| t.beat_position as f32).unwrap_or(0.0);
            let bpm = telemetry.as_ref().map(|t| t.bpm).unwrap_or(120.0);

            let num_beats = 16;
            let beat_w = rect.width() / num_beats as f32;

            for i in 0..num_beats {
                let bx = rect.left() + i as f32 * beat_w;
                let is_downbeat = i % 4 == 0;
                let stroke_color = if is_downbeat { theme.accent } else { theme.text_disabled };
                let stroke_w = if is_downbeat { 2.0_f32 } else { 1.0_f32 };

                ui.painter().line_segment(
                    [pos2(bx, rect.top()), pos2(bx, rect.bottom())],
                    Stroke::new(stroke_w, stroke_color.linear_multiply(0.4)),
                );
            }

            let playhead_x = rect.left() + ((beat_pos % 16.0) / 16.0) * rect.width();
            ui.painter().line_segment(
                [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
                Stroke::new(2.5_f32, theme.warning),
            );

            ui.painter().text(
                rect.right_top() - Vec2::new(10.0, -8.0),
                Align2::RIGHT_TOP,
                format!("BPM: {:.1} | BEAT: {:.2}", bpm, beat_pos),
                FontId::monospace(10.0),
                theme.warning,
            );
        }

        // 6. [ISO 226 EQUAL-LOUDNESS PHON CONTOURS]
        if app.analyzer.layer_harmonic {
            for phon in &[20.0f32, 40.0, 80.0] {
                let mut phon_pts = Vec::with_capacity(num_bins);
                for i in 0..num_bins {
                    let factor = calculate_iso226_phon_factor(i, *phon);
                    let x = rect.left() + (i as f32 + 0.5) * bin_w;
                    let y = (rect.bottom() - factor * rect.height() * 0.35).clamp(rect.top(), rect.bottom());
                    phon_pts.push(pos2(x, y));
                }
                for i in 0..num_bins.saturating_sub(1) {
                    ui.painter().line_segment(
                        [phon_pts[i], phon_pts[i + 1]],
                        Stroke::new(1.2_f32, Color32::from_rgb(0, 220, 180).linear_multiply(*phon / 100.0)),
                    );
                }
            }
        }

        // 9. [ENERGY / EBU R128 LUFS] Dynamic Short-Term & Integrated LUFS
        if app.analyzer.layer_energy {
            let lufs_y = rect.top() + 35.0;
            ui.painter().text(
                pos2(rect.left() + 10.0, lufs_y),
                Align2::LEFT_TOP,
                format!("EBU R128: Momentary {:.1} LUFS | Short-Term {:.1} LUFS | Integrated {:.1} LUFS | LRA: {:.1} LU",
                    momentary_lufs, short_term_lufs, integrated_lufs, loudness_range_lra),
                FontId::proportional(10.0),
                theme.success,
            );
        }

        // --- Side Panel: Interactive Lissajous Goniometer / Stereo Vectorscope ---
        if show_vectorscope {
            let vec_size = Vec2::new(210.0, 320.0);
            let (v_rect, _) = ui.allocate_exact_size(vec_size, Sense::hover());

            ui.painter().rect_filled(v_rect, theme.radius_md, Color32::from_rgb(12, 14, 20));
            ui.painter().rect_stroke(v_rect, theme.radius_md, Stroke::new(1.0_f32, theme.border));

            let center = v_rect.center();
            let radius = 80.0;

            // Draw Lissajous crosshairs (M/S axes)
            ui.painter().circle_stroke(center, radius, Stroke::new(1.0_f32, theme.border));
            ui.painter().line_segment([pos2(center.x - radius, center.y), pos2(center.x + radius, center.y)], Stroke::new(1.0_f32, theme.border));
            ui.painter().line_segment([pos2(center.x, center.y - radius), pos2(center.x, center.y + radius)], Stroke::new(1.0_f32, theme.border));

            ui.painter().text(pos2(center.x, center.y - radius - 8.0), Align2::CENTER_CENTER, "+M (Mono)", FontId::proportional(8.0), theme.text_secondary);
            ui.painter().text(pos2(center.x + radius + 12.0, center.y), Align2::CENTER_CENTER, "+S (Side)", FontId::proportional(8.0), theme.text_secondary);

            // Draw Goniometer polar scatter cloud
            let mut points = Vec::with_capacity(128);
            for i in 0..128 {
                let sample_m = goniometer[i];
                let sample_s = goniometer[(i + 32) % 128];
                let px = center.x + sample_s * radius * 0.9;
                let py = center.y - sample_m * radius * 0.9;
                points.push(pos2(px, py));
            }

            for i in 0..points.len().saturating_sub(1) {
                ui.painter().line_segment([points[i], points[i + 1]], Stroke::new(1.2_f32, theme.accent.linear_multiply(0.8)));
            }

            // Phase correlation indicator
            let corr_y = v_rect.bottom() - 25.0;
            ui.painter().text(
                pos2(center.x, corr_y),
                Align2::CENTER_CENTER,
                format!("Phase Correlation: {:.2}", phase_corr),
                FontId::proportional(9.0),
                if phase_corr >= 0.0 { theme.success } else { theme.danger },
            );
        }
    });

    ui.add_space(theme.space_xs);

    // --- Dynamic 4-Deck Cross-Collision Matrix & Streaming Targets ---
    egui::CollapsingHeader::new(RichText::new("4-DECK CROSS-COLLISION MATRIX & STREAMING COMPLIANCE").strong().color(theme.accent))
        .default_open(true)
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                cols[0].group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("4-DECK FREQUENCY COLLISION").strong().size(theme.type_caption).color(theme.danger));
                        ui.add_space(2.0);

                        let peaks = &app.viz.damped_peaks;
                        let col_ab = ((peaks[0] * peaks[1]) * 100.0).clamp(0.0, 99.0) as usize;
                        let col_ac = ((peaks[0] * peaks[2]) * 100.0).clamp(0.0, 99.0) as usize;
                        let col_ad = ((peaks[0] * peaks[3]) * 100.0).clamp(0.0, 99.0) as usize;
                        let col_bc = ((peaks[1] * peaks[2]) * 100.0).clamp(0.0, 99.0) as usize;
                        let col_bd = ((peaks[1] * peaks[3]) * 100.0).clamp(0.0, 99.0) as usize;
                        let col_cd = ((peaks[2] * peaks[3]) * 100.0).clamp(0.0, 99.0) as usize;

                        let format_col = |val: usize| {
                            if val > 65 {
                                RichText::new(format!("{}% ⚡", val)).color(theme.danger)
                            } else {
                                RichText::new(format!("{}%", val)).color(theme.text_secondary)
                            }
                        };

                        egui::Grid::new("four_deck_collision_grid")
                            .spacing([12.0, 4.0])
                            .show(ui, |ui| {
                                ui.label(""); ui.label("DECK A"); ui.label("DECK B"); ui.label("DECK C"); ui.label("DECK D"); ui.end_row();
                                ui.label("DECK A"); ui.label("—"); ui.label(format_col(col_ab)); ui.label(format_col(col_ac)); ui.label(format_col(col_ad)); ui.end_row();
                                ui.label("DECK B"); ui.label(format_col(col_ab)); ui.label("—"); ui.label(format_col(col_bc)); ui.label(format_col(col_bd)); ui.end_row();
                                ui.label("DECK C"); ui.label(format_col(col_ac)); ui.label(format_col(col_bc)); ui.label("—"); ui.label(format_col(col_cd)); ui.end_row();
                                ui.label("DECK D"); ui.label(format_col(col_ad)); ui.label(format_col(col_bd)); ui.label(format_col(col_cd)); ui.label("—"); ui.end_row();
                            });
                    });
                });

                cols[1].group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("EBU R128 & STREAMING COMPLIANCE").strong().size(theme.type_caption).color(theme.success));
                        ui.add_space(2.0);

                        let spot_pen = (integrated_lufs - (-14.0)).max(0.0);
                        let apple_pen = (integrated_lufs - (-16.0)).max(0.0);
                        let yt_pen = (integrated_lufs - (-14.0)).max(0.0);

                        let format_pen = |pen: f32| {
                            if pen > 0.1 {
                                RichText::new(format!("Penalty: -{:.1} dB", pen)).color(theme.warning)
                            } else {
                                RichText::new("OK (0.0 dB)").color(theme.success)
                            }
                        };

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Spotify (-14 LUFS):").size(9.0));
                            ui.label(format_pen(spot_pen));
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Apple Music (-16 LUFS):").size(9.0));
                            ui.label(format_pen(apple_pen));
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("YouTube (-14 LUFS):").size(9.0));
                            ui.label(format_pen(yt_pen));
                        });
                    });
                });
            });
        });
}

/// Screen 2: Full-track offline analysis and SoundDNA passport inspection
fn render_full_track_screen(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;

    // --- On-Demand Track Selector Bar ---
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_sm)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .inner_margin(Margin::symmetric(theme.space_md, theme.space_xs))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("SELECT TRACK FOR ANALYSIS:").strong().size(theme.type_caption).color(theme.accent));

                let focused_deck = app.decks.focused_deck.min(15);
                let deck_track_id = app.decks.now_playing[focused_deck];

                let selected_id = app.library.selected_library_track.or(deck_track_id);

                let current_title = selected_id
                    .and_then(|id| app.get_cached_track(id))
                    .map(|t| format!("{} — {}", t.artist, t.title))
                    .unwrap_or_else(|| "Select track from library...".to_string());

                egui::ComboBox::from_id_source("full_track_analyzer_combo")
                    .selected_text(current_title)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for track in &app.library.cached_library {
                            let text = format!("{} — {}", track.artist, track.title);
                            if ui.selectable_label(app.library.selected_library_track == Some(track.id), text).clicked() {
                                app.library.selected_library_track = Some(track.id);
                            }
                        }
                    });
            });
        });

    ui.add_space(theme.space_xs);

    let focused_deck = app.decks.focused_deck.min(15);
    let track_id = app.library.selected_library_track.or(app.decks.now_playing[focused_deck]);
    let cached_track = track_id.and_then(|id| app.get_cached_track(id));

    if let Some(track) = cached_track {
        ui.group(|ui| {
            ui.vertical(|ui| {
                let total_samples = track.metadata.total_samples.max(1);
                let sr = track.metadata.sample_rate.max(1) as f64;
                let total_sec = total_samples as f64 / sr;
                let duration_min = (total_sec / 60.0).floor() as u32;
                let duration_sec = (total_sec % 60.0).floor() as u32;

                ui.horizontal(|ui| {
                    ui.label(RichText::new(&track.title).strong().size(theme.type_body).color(theme.text_primary));
                    ui.label(RichText::new(format!("by {}", track.artist)).size(theme.type_caption).color(theme.text_secondary));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let root_k = track.metadata.root_key.map(|k| k as usize % 12).unwrap_or(0);
                        let (k_name, camelot) = pitch_index_to_camelot(root_k, true);
                        ui.label(RichText::new(format!("KEY: {} ({}) | BPM: {:.1} | DURATION: {:02}:{:02} ({} samples)",
                            k_name, camelot, track.metadata.bpm, duration_min, duration_sec, total_samples)).strong().size(theme.type_caption).color(theme.accent));
                    });
                });

                ui.separator();
                ui.add_space(2.0);

                // --- Full-Track Multi-Band Waveform Canvas ---
                let available_size = Vec2::new(ui.available_width(), 150.0);
                let (rect, _response) = ui.allocate_exact_size(available_size, Sense::click_and_drag());

                ui.painter().rect_filled(rect, theme.radius_sm, Color32::from_rgb(12, 16, 24));
                ui.painter().rect_stroke(rect, theme.radius_sm, Stroke::new(1.0_f32, theme.border));

                let peaks = track.metadata.peaks.as_slice();
                if !peaks.is_empty() {
                    let num_peaks = peaks.len();
                    let step_w = rect.width() / num_peaks as f32;
                    let center_y = rect.center().y;
                    let half_h = rect.height() * 0.42;

                    for (i, &amp) in peaks.iter().enumerate() {
                        let x = rect.left() + i as f32 * step_w;
                        let h = (amp.abs() * half_h).clamp(1.0, half_h);
                        ui.painter().line_segment(
                            [pos2(x, center_y - h), pos2(x, center_y + h)],
                            Stroke::new(1.2_f32, theme.accent.linear_multiply(0.8)),
                        );
                    }
                }

                // Render ALL transients across the complete track duration
                let transients = track.metadata.transients.as_slice();
                if !transients.is_empty() {
                    for &t_frame in transients.iter() {
                        let pos_norm = (t_frame as f64 / total_samples as f64).clamp(0.0, 1.0) as f32;
                        let tx = rect.left() + pos_norm * rect.width();
                        ui.painter().line_segment(
                            [pos2(tx, rect.bottom()), pos2(tx, rect.bottom() - 25.0)],
                            Stroke::new(1.0_f32, theme.danger.linear_multiply(0.7)),
                        );
                    }
                }

                // Render Hot-Cues across the track timeline
                for (cue_idx, &cue_opt) in track.metadata.hot_cues.iter().enumerate() {
                    if let Some(cue_frame) = cue_opt {
                        let cue_norm = (cue_frame as f64 / total_samples as f64).clamp(0.0, 1.0) as f32;
                        let cx = rect.left() + cue_norm * rect.width();
                        ui.painter().line_segment(
                            [pos2(cx, rect.top()), pos2(cx, rect.bottom())],
                            Stroke::new(1.5_f32, theme.success),
                        );
                        ui.painter().text(
                            pos2(cx + 3.0, rect.top() + 10.0),
                            Align2::LEFT_TOP,
                            format!("CUE {}", cue_idx + 1),
                            FontId::proportional(8.0),
                            theme.success,
                        );
                    }
                }

                // Playhead position line
                let beat_pos = telemetry.as_ref().map(|t| t.beat_position as f32).unwrap_or(0.0);
                let total_beats = (total_sec * (track.metadata.bpm as f64 / 60.0)).max(1.0) as f32;
                let playhead_norm = ((beat_pos % total_beats) / total_beats).clamp(0.0, 1.0);
                let playhead_x = rect.left() + playhead_norm * rect.width();

                ui.painter().line_segment(
                    [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
                    Stroke::new(2.0_f32, theme.warning),
                );

                ui.add_space(theme.space_xs);

                // --- Full-Track SoundDNA Latent Embedding & Polar Radar Blueprint ---
                ui.columns(2, |cols| {
                    cols[0].group(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("16D SOUNDDNA POLAR LATENT RADAR").strong().size(theme.type_caption).color(theme.accent));
                            ui.add_space(4.0);

                            let radar_size = Vec2::new(ui.available_width(), 160.0);
                            let (r_rect, _) = ui.allocate_exact_size(radar_size, Sense::hover());

                            ui.painter().rect_filled(r_rect, theme.radius_sm, Color32::from_rgb(10, 14, 22));
                            ui.painter().rect_stroke(r_rect, theme.radius_sm, Stroke::new(1.0_f32, theme.border));

                            let r_center = r_rect.center();
                            let r_max_radius = 65.0;

                            // Concentric radar rings
                            for ring in &[0.33f32, 0.66, 1.0] {
                                ui.painter().circle_stroke(r_center, r_max_radius * ring, Stroke::new(1.0_f32, theme.border.linear_multiply(0.5)));
                            }

                            let latent = &track.metadata.dna.spectral.latent_space;
                            let mut polygon_pts = Vec::with_capacity(16);

                            for i in 0..16 {
                                let angle = (i as f32 / 16.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                                let val = latent[i].clamp(0.05, 1.0);
                                let dist = val * r_max_radius;

                                let px = r_center.x + angle.cos() * dist;
                                let py = r_center.y + angle.sin() * dist;
                                polygon_pts.push(pos2(px, py));

                                // Draw radial spokes
                                let edge_x = r_center.x + angle.cos() * r_max_radius;
                                let edge_y = r_center.y + angle.sin() * r_max_radius;
                                ui.painter().line_segment([r_center, pos2(edge_x, edge_y)], Stroke::new(1.0_f32, theme.border.linear_multiply(0.3)));
                            }

                            // Fill SoundDNA radar polygon
                            if polygon_pts.len() >= 3 {
                                for i in 0..16 {
                                    let next_i = (i + 1) % 16;
                                    ui.painter().line_segment([polygon_pts[i], polygon_pts[next_i]], Stroke::new(1.5_f32, theme.accent));
                                }
                            }
                        });
                    });

                    cols[1].group(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("FULL TRACK PERCEPTION & GROOVE PROFILE").strong().size(theme.type_caption).color(theme.success));
                            ui.add_space(4.0);

                            ui.label(RichText::new(format!("Integrated LUFS: {:.1} LUFS", track.metadata.dna.perception.lufs_integrated)).size(9.0));
                            ui.label(RichText::new(format!("Crest Factor: {:.1} dB", track.metadata.dna.perception.crest_factor_db)).size(9.0));
                            ui.label(RichText::new(format!("Zero Crossing Rate: {:.3}", track.metadata.dna.perception.zero_crossing_rate)).size(9.0));
                            ui.label(RichText::new(format!("Spectral Brightness: {:.2}", track.metadata.dna.perception.brightness)).size(9.0));
                            ui.label(RichText::new(format!("Syncopation Index: {:.2}", track.metadata.dna.rhythmic.syncopation_index)).size(9.0));

                            ui.add_space(6.0);
                            ui.label(RichText::new("MICRO-TIMING GROOVE DEVIATIONS (16THs)").strong().size(8.5).color(theme.warning));

                            let groove = &track.metadata.dna.rhythmic.micro_timing;
                            for (idx, &dev) in groove.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("STEP {:02}", idx + 1)).size(7.5).color(theme.text_secondary));
                                    let dev_norm = (dev as f32 / 128.0).clamp(-1.0, 1.0);
                                    let dev_ms = dev_norm * 25.0;
                                    let sign = if dev_ms >= 0.0 { "+" } else { "" };
                                    ui.label(RichText::new(format!("{}{:.1} ms", sign, dev_ms)).size(7.5).monospace().color(theme.warning));
                                });
                            }
                        });
                    });
                });

                ui.add_space(theme.space_xs);

                // --- Export Actions ---
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("💾 EXPORT FULL TRACK DNA PASSPORT (JSON)").size(theme.type_caption)).clicked() {
                        if let Ok(json) = serde_json::to_string_pretty(&track) {
                            let _ = std::fs::write(format!("dna_passport_{}.json", track.id), json);
                        }
                    }
                    if ui.button(RichText::new("🔍 FIND SIMILAR SOUNDS IN LIBRARY").size(theme.type_caption)).clicked() {
                        app.active_view = crate::View::Library;
                    }
                });
            });
        });
    } else {
        ui.group(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(RichText::new("NO TRACK LOADED FOR FULL-TRACK ANALYSIS").strong().size(theme.type_body).color(theme.text_secondary));
                ui.label(RichText::new("Load a track on the focused DJ Deck or select a track in the Library to inspect its complete SoundDNA passport, multi-band waveform, and full-length perception profile.").size(theme.type_caption).color(theme.text_disabled));
                ui.add_space(20.0);
                if ui.button("→ OPEN TRACK LIBRARY").clicked() {
                    app.active_view = crate::View::Library;
                }
                ui.add_space(40.0);
            });
        });
    }
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
    fn test_hz_to_note_name() {
        let note_a4 = hz_to_note_name(440.0);
        assert!(note_a4.contains("A4"));

        let note_c4 = hz_to_note_name(261.63);
        assert!(note_c4.contains("C4"));
    }

    #[test]
    fn test_analyzer_view_state_defaults() {
        let state = crate::state::AnalyzerViewState::default();
        assert_eq!(state.mode, AnalyzerMode::RealTime);
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
        assert!(ear_resonance_factor > 0.5, "Resonance bin factor should be positive");
    }

    #[test]
    fn test_calculate_iso226_phon_factor_boundary_clamping() {
        for bin in 0..128 {
            for phon in &[20.0f32, 40.0, 80.0, 120.0] {
                let factor = calculate_iso226_phon_factor(bin, *phon);
                assert!(factor >= 0.0 && factor <= 1.0, "Phon factor at bin {} phon {} must be clamped to [0.0, 1.0]", bin, phon);
            }
        }
    }

    #[test]
    fn test_analyzer_mode_toggle_and_full_track_defaults() {
        let mut state = crate::state::AnalyzerViewState::default();
        assert_eq!(state.mode, AnalyzerMode::RealTime);
        state.mode = AnalyzerMode::FullTrack;
        assert_eq!(state.mode, AnalyzerMode::FullTrack);
    }
}
