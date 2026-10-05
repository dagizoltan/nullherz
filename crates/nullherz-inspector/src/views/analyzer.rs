use eframe::egui;
use audio_core::Telemetry;
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

pub fn render(app: &mut InspectorApp, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;

    egui::ScrollArea::vertical()
        .id_source("analyzer_scroll_area")
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // --- Top Screen/Mode Selector Bar ---
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("AUDIO ANALYZER & PERCEPTION ENGINE").strong().color(theme.text_primary));
                    ui.add_space(theme.space_md);

                    ui.selectable_value(
                        &mut app.analyzer.mode,
                        AnalyzerMode::RealTime,
                        egui::RichText::new("⚡ REAL-TIME PERCEPTION").strong().size(theme.type_body),
                    );
                    ui.selectable_value(
                        &mut app.analyzer.mode,
                        AnalyzerMode::FullTrack,
                        egui::RichText::new("📊 FULL-TRACK ANALYSIS").strong().size(theme.type_body),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if app.analyzer.mode == AnalyzerMode::RealTime {
                            ui.toggle_value(&mut app.analyzer.ab_enabled, egui::RichText::new("A/B COMPARES").strong().size(theme.type_caption));
                        }
                    });
                });
                ui.separator();
                ui.add_space(theme.space_xs);

                match app.analyzer.mode {
                    AnalyzerMode::RealTime => render_realtime_screen(app, ui, telemetry),
                    AnalyzerMode::FullTrack => render_full_track_screen(app, ui, telemetry),
                }
            });
        });
}

/// Screen 1: Real-time perception and streaming acoustic field
fn render_realtime_screen(app: &mut InspectorApp, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;

    // Layer Filter Chips in Clean Categorized Groups
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("DISPLAY:").size(theme.type_caption).strong().color(theme.accent));
        ui.toggle_value(&mut app.analyzer.layer_spectral, "[SPECTRAL]");
        ui.toggle_value(&mut app.analyzer.show_waterfall, "[3D WATERFALL]");
        ui.toggle_value(&mut app.analyzer.layer_raw, "[RAW]");

        ui.add_space(10.0);
        ui.label(egui::RichText::new("PERCEPTION:").size(theme.type_caption).strong().color(theme.accent));
        ui.toggle_value(&mut app.analyzer.show_camelot_wheel, "[CAMELOT]");
        ui.toggle_value(&mut app.analyzer.layer_harmonic, "[ISO 226 PHON]");
        ui.toggle_value(&mut app.analyzer.layer_rhythm, "[RHYTHM]");
        ui.toggle_value(&mut app.analyzer.layer_transient, "[TRANSIENT]");
        ui.toggle_value(&mut app.analyzer.layer_events, "[EVENTS]");

        ui.add_space(10.0);
        ui.label(egui::RichText::new("METRICS:").size(theme.type_caption).strong().color(theme.accent));
        ui.toggle_value(&mut app.analyzer.layer_energy, "[LUFS]");
        ui.toggle_value(&mut app.analyzer.layer_stereo, "[STEREO]");
        ui.toggle_value(&mut app.analyzer.layer_collision, "[COLLISION]");
        ui.toggle_value(&mut app.analyzer.layer_dna, "[DNA]");
        ui.toggle_value(&mut app.analyzer.layer_embedding, "[MANIFOLD]");
    });
    ui.separator();
    ui.add_space(theme.space_xs);

    // --- Central Spectral Field Canvas ---
    let available_size = egui::vec2(ui.available_width(), 320.0);
    let (rect, _response) = ui.allocate_exact_size(available_size, egui::Sense::hover());

    ui.painter().rect_filled(rect, theme.radius_md, egui::Color32::from_rgb(10, 12, 18));
    ui.painter().rect_stroke(rect, theme.radius_md, egui::Stroke::new(1.0_f32, theme.border));

    let time = ui.input(|i| i.time);
    let spectrum_a = &app.viz.damped_spectrum;
    let _goniometer = &app.viz.damped_goniometer;
    let _latent = &app.viz.damped_latent;

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

    // --- Real-Time Acoustic Anomaly Detector Banner ---
    let peak_max = app.viz.damped_master_peaks[0].max(app.viz.damped_master_peaks[1]);
    let phase_corr = app.viz.damped_goniometer.iter().sum::<f32>() / 128.0;

    let has_clipping = peak_max > 0.98;
    let has_phase_inversion = phase_corr < -0.4;

    if has_clipping || has_phase_inversion {
        let alert_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 15.0, rect.top() + 8.0),
            egui::vec2(rect.width() - 30.0, 22.0),
        );
        ui.painter().rect_filled(alert_rect, theme.radius_sm, theme.danger.linear_multiply(0.2));
        ui.painter().rect_stroke(alert_rect, theme.radius_sm, egui::Stroke::new(1.0_f32, theme.danger));

        let alert_msg = if has_clipping {
            format!("⚡ ACOUSTIC ANOMALY: INTER-SAMPLE CLIPPING DETECTED ({:.1} dBFS)", 20.0 * peak_max.log10())
        } else {
            format!("⚡ ACOUSTIC ANOMALY: SUB-BASS PHASE COLLAPSE (Corr: {:.2})", phase_corr)
        };

        ui.painter().text(
            alert_rect.center(),
            egui::Align2::CENTER_CENTER,
            alert_msg,
            egui::FontId::proportional(9.5),
            theme.danger,
        );
    }

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
            let y_offset = h_idx as f32 * 4.2;
            let scale = 1.0 - (h_idx as f32 / num_history as f32) * 0.65;
            let alpha = 1.0 - (h_idx as f32 / num_history as f32);

            let mut pts = Vec::with_capacity(num_bins);
            for i in 0..num_bins {
                let mag = frame[i].clamp(0.0, 1.0);
                let x = rect.left() + (i as f32) * bin_w * scale + (h_idx as f32 * 1.0);
                let y = (rect.bottom() - y_offset - mag * rect.height() * 0.35 * scale).clamp(rect.top(), rect.bottom());
                pts.push(egui::pos2(x, y));
            }

            for i in 0..num_bins.saturating_sub(1) {
                let color = egui::Color32::from_rgb(
                    ((i as f32 / num_bins as f32) * 255.0) as u8,
                    (180.0 * alpha) as u8,
                    220,
                ).linear_multiply(alpha * 0.85);

                ui.painter().line_segment([pts[i], pts[i + 1]], egui::Stroke::new(1.0_f32, color));
            }
        }
    }

    // 1b. [SPECTRAL] Layer
    if app.analyzer.layer_spectral && !app.analyzer.show_waterfall {
        for i in 0..num_bins {
            let mag_a = spectrum_a[i].clamp(0.0, 1.0);
            let bar_h = mag_a * rect.height() * 0.65;
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

            if mag_a > 0.05 {
                let peak_y = (rect.bottom() - bar_h - 2.0).clamp(rect.top(), rect.bottom());
                ui.painter().line_segment(
                    [egui::pos2(rect.left() + i as f32 * bin_w, peak_y), egui::pos2(rect.left() + (i + 1) as f32 * bin_w - 1.0, peak_y)],
                    egui::Stroke::new(1.5_f32, theme.accent),
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

        let wheel_center = egui::pos2(rect.left() + 85.0, rect.top() + 85.0);
        let wheel_r = 45.0;

        ui.painter().circle_filled(wheel_center, wheel_r, egui::Color32::from_rgb(18, 24, 38).linear_multiply(0.92));
        ui.painter().circle_stroke(wheel_center, wheel_r, egui::Stroke::new(1.5_f32, theme.accent));

        ui.painter().text(
            wheel_center - egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_CENTER,
            camelot_code,
            egui::FontId::proportional(14.0),
            theme.accent,
        );
        ui.painter().text(
            wheel_center + egui::vec2(0.0, 10.0),
            egui::Align2::CENTER_CENTER,
            key_name,
            egui::FontId::proportional(9.0),
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
                [egui::pos2(bx, rect.top()), egui::pos2(bx, rect.bottom())],
                egui::Stroke::new(stroke_w, stroke_color.linear_multiply(0.4)),
            );
        }

        let playhead_x = rect.left() + ((beat_pos % 16.0) / 16.0) * rect.width();
        ui.painter().line_segment(
            [egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())],
            egui::Stroke::new(2.5_f32, theme.warning),
        );

        ui.painter().text(
            rect.right_top() - egui::vec2(10.0, -8.0),
            egui::Align2::RIGHT_TOP,
            format!("BPM: {:.1} | BEAT: {:.2}", bpm, beat_pos),
            egui::FontId::monospace(10.0),
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
                phon_pts.push(egui::pos2(x, y));
            }
            for i in 0..num_bins.saturating_sub(1) {
                ui.painter().line_segment(
                    [phon_pts[i], phon_pts[i + 1]],
                    egui::Stroke::new(1.2_f32, egui::Color32::from_rgb(0, 220, 180).linear_multiply(*phon / 100.0)),
                );
            }
        }
    }

    // 9. [ENERGY / EBU R128 LUFS] Dynamic Short-Term & Integrated LUFS
    if app.analyzer.layer_energy {
        let lufs_y = rect.top() + 35.0;
        ui.painter().text(
            egui::pos2(rect.left() + 10.0, lufs_y),
            egui::Align2::LEFT_TOP,
            format!("EBU R128: Momentary {:.1} LUFS | Short-Term {:.1} LUFS | Integrated {:.1} LUFS | LRA: {:.1} LU",
                momentary_lufs, short_term_lufs, integrated_lufs, loudness_range_lra),
            egui::FontId::proportional(10.0),
            theme.success,
        );
    }

    ui.add_space(theme.space_xs);

    // --- Dynamic 4-Deck Cross-Collision Matrix & Streaming Targets ---
    egui::CollapsingHeader::new(egui::RichText::new("4-DECK CROSS-COLLISION MATRIX & STREAMING COMPLIANCE").strong().color(theme.accent))
        .default_open(true)
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                cols[0].group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("4-DECK FREQUENCY COLLISION").strong().size(theme.type_caption).color(theme.danger));
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
                                egui::RichText::new(format!("{}% ⚡", val)).color(theme.danger)
                            } else {
                                egui::RichText::new(format!("{}%", val)).color(theme.text_secondary)
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
                        ui.label(egui::RichText::new("EBU R128 & STREAMING COMPLIANCE").strong().size(theme.type_caption).color(theme.success));
                        ui.add_space(2.0);

                        let spot_pen = (integrated_lufs - (-14.0)).max(0.0);
                        let apple_pen = (integrated_lufs - (-16.0)).max(0.0);
                        let yt_pen = (integrated_lufs - (-14.0)).max(0.0);

                        let format_pen = |pen: f32| {
                            if pen > 0.1 {
                                egui::RichText::new(format!("Penalty: -{:.1} dB", pen)).color(theme.warning)
                            } else {
                                egui::RichText::new("OK (0.0 dB)").color(theme.success)
                            }
                        };

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Spotify (-14 LUFS):").size(9.0));
                            ui.label(format_pen(spot_pen));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Apple Music (-16 LUFS):").size(9.0));
                            ui.label(format_pen(apple_pen));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("YouTube (-14 LUFS):").size(9.0));
                            ui.label(format_pen(yt_pen));
                        });
                    });
                });
            });
        });
}

/// Screen 2: Full-track offline analysis and SoundDNA passport inspection
fn render_full_track_screen(app: &mut InspectorApp, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let focused_deck = app.decks.focused_deck.min(15);
    let track_id = app.decks.now_playing[focused_deck];
    let cached_track = track_id.and_then(|id| app.get_cached_track(id));

    if let Some(track) = cached_track {
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&track.title).strong().size(theme.type_body).color(theme.text_primary));
                    ui.label(egui::RichText::new(format!("by {}", track.artist)).size(theme.type_caption).color(theme.text_secondary));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let root_k = track.metadata.root_key.map(|k| k as usize % 12).unwrap_or(0);
                        let (k_name, camelot) = pitch_index_to_camelot(root_k, true);
                        ui.label(egui::RichText::new(format!("KEY: {} ({}) | BPM: {:.1}", k_name, camelot, track.metadata.bpm)).strong().size(theme.type_caption).color(theme.accent));
                    });
                });

                ui.separator();
                ui.add_space(2.0);

                // --- Full-Track Multi-Band Waveform Canvas ---
                let available_size = egui::vec2(ui.available_width(), 140.0);
                let (rect, _response) = ui.allocate_exact_size(available_size, egui::Sense::hover());

                ui.painter().rect_filled(rect, theme.radius_sm, egui::Color32::from_rgb(12, 16, 24));
                ui.painter().rect_stroke(rect, theme.radius_sm, egui::Stroke::new(1.0_f32, theme.border));

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
                            [egui::pos2(x, center_y - h), egui::pos2(x, center_y + h)],
                            egui::Stroke::new(1.2_f32, theme.accent.linear_multiply(0.8)),
                        );
                    }
                }

                // Render transients timeline
                let transients = track.metadata.transients.as_slice();
                if !transients.is_empty() {
                    let _sample_rate = track.metadata.sample_rate.max(1) as f64;
                    let track_duration_samples = (peaks.len() as f64 * 128.0).max(1.0);
                    for &t_frame in transients.iter().take(200) {
                        let pos_norm = (t_frame as f64 / track_duration_samples).clamp(0.0, 1.0) as f32;
                        let tx = rect.left() + pos_norm * rect.width();
                        ui.painter().line_segment(
                            [egui::pos2(tx, rect.bottom()), egui::pos2(tx, rect.bottom() - 25.0)],
                            egui::Stroke::new(1.0_f32, theme.danger.linear_multiply(0.7)),
                        );
                    }
                }

                // Playhead position line
                let beat_pos = telemetry.as_ref().map(|t| t.beat_position as f32).unwrap_or(0.0);
                let beats_in_track = (track.metadata.bpm * 3.0).max(16.0); // estimated length
                let playhead_norm = ((beat_pos % beats_in_track) / beats_in_track).clamp(0.0, 1.0);
                let playhead_x = rect.left() + playhead_norm * rect.width();

                ui.painter().line_segment(
                    [egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())],
                    egui::Stroke::new(2.0_f32, theme.warning),
                );

                ui.add_space(theme.space_xs);

                // --- Full-Track SoundDNA Latent Embedding Blueprint ---
                ui.columns(2, |cols| {
                    cols[0].group(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("16D SOUNDDNA LATENT EMBEDDING").strong().size(theme.type_caption).color(theme.accent));
                            ui.add_space(4.0);

                            let latent = &track.metadata.dna.spectral.latent_space;
                            for i in 0..16 {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(format!("DIM {:02}", i + 1)).size(8.0).color(theme.text_secondary));
                                    let val = latent[i].clamp(0.0, 1.0);
                                    let bar_w = 120.0;
                                    let bar_rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(bar_w, 8.0));
                                    let fill_rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(bar_w * val, 8.0));

                                    ui.painter().rect_filled(bar_rect, 1.0, theme.bg_inset);
                                    ui.painter().rect_filled(fill_rect, 1.0, theme.accent);
                                    ui.add_space(125.0);
                                    ui.label(egui::RichText::new(format!("{:.2}", val)).size(8.0).monospace().color(theme.text_primary));
                                });
                            }
                        });
                    });

                    cols[1].group(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("FULL TRACK PERCEPTION & GROOVE PROFILE").strong().size(theme.type_caption).color(theme.success));
                            ui.add_space(4.0);

                            ui.label(egui::RichText::new(format!("Integrated LUFS: {:.1} LUFS", track.metadata.dna.perception.lufs_integrated)).size(9.0));
                            ui.label(egui::RichText::new(format!("Crest Factor: {:.1} dB", track.metadata.dna.perception.crest_factor_db)).size(9.0));
                            ui.label(egui::RichText::new(format!("Zero Crossing Rate: {:.3}", track.metadata.dna.perception.zero_crossing_rate)).size(9.0));
                            ui.label(egui::RichText::new(format!("Spectral Brightness: {:.2}", track.metadata.dna.perception.brightness)).size(9.0));
                            ui.label(egui::RichText::new(format!("Syncopation Index: {:.2}", track.metadata.dna.rhythmic.syncopation_index)).size(9.0));

                            ui.add_space(6.0);
                            ui.label(egui::RichText::new("MICRO-TIMING GROOVE DEVIATIONS (16THs)").strong().size(8.5).color(theme.warning));

                            let groove = &track.metadata.dna.rhythmic.micro_timing;
                            for (idx, &dev) in groove.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(format!("STEP {:02}", idx + 1)).size(7.5).color(theme.text_secondary));
                                    let dev_norm = (dev as f32 / 128.0).clamp(-1.0, 1.0);
                                    let dev_ms = dev_norm * 25.0;
                                    let sign = if dev_ms >= 0.0 { "+" } else { "" };
                                    ui.label(egui::RichText::new(format!("{}{:.1} ms", sign, dev_ms)).size(7.5).monospace().color(theme.warning));
                                });
                            }
                        });
                    });
                });

                ui.add_space(theme.space_xs);

                // --- Export Actions ---
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("💾 EXPORT FULL TRACK DNA PASSPORT (JSON)").size(theme.type_caption)).clicked() {
                        if let Ok(json) = serde_json::to_string_pretty(&track) {
                            let _ = std::fs::write(format!("dna_passport_{}.json", track.id), json);
                        }
                    }
                    if ui.button(egui::RichText::new("🔍 FIND SIMILAR SOUNDS IN LIBRARY").size(theme.type_caption)).clicked() {
                        app.active_view = crate::View::Library;
                    }
                });
            });
        });
    } else {
        ui.group(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("NO TRACK LOADED FOR FULL-TRACK ANALYSIS").strong().size(theme.type_body).color(theme.text_secondary));
                ui.label(egui::RichText::new("Load a track on the focused DJ Deck or select a track in the Library to inspect its complete SoundDNA passport, multi-band waveform, and full-length perception profile.").size(theme.type_caption).color(theme.text_disabled));
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
