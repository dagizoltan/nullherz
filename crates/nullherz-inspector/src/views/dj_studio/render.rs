use egui::{Ui, Color32, RichText, Frame, Margin, Rounding, Stroke, Vec2};
use crate::InspectorApp;
use audio_core::Telemetry;

use super::waveform;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    if app.decks.focused_deck >= 4 {
        app.decks.focused_deck = 0;
    }
    let theme = app.theme;

    render_header(ui, telemetry, &theme);
    ui.add_space(theme.space_xs);

    // Calculate space for 4 full waveform lanes
    let waveform_section_h = ui.available_height().max(180.0);
    let spacing_h = 2.0;
    let lane_h = ((waveform_section_h - spacing_h * 3.0) / 4.0).max(35.0);

    ui.vertical(|ui| {
        for i in 0..4 {
            render_waveform_lane(app, ui, i, lane_h, telemetry);
            if i < 3 {
                ui.add_space(spacing_h);
            }
        }
    });
}

fn render_header(ui: &mut Ui, telemetry: &Option<Telemetry>, theme: &nullherz_ui_hal::Theme) {
    Frame::none()
        .fill(theme.bg_surface)
        .stroke(theme.border_stroke)
        .rounding(theme.radius_md)
        .inner_margin(Margin::symmetric(theme.space_md, theme.space_sm))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("PERFORMANCE DECK MATRIX").strong().size(theme.type_caption).color(theme.text_secondary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(t) = telemetry {
                        ui.label(RichText::new("BPM").size(theme.type_caption).color(theme.text_secondary));
                        ui.label(RichText::new(format!("{:.1}", t.bpm)).monospace().strong().color(theme.accent).size(theme.type_body));
                    }
                });
            });
        });
}

pub fn format_duration(samples: u64, sample_rate: f32) -> String {
    if sample_rate <= 0.0 {
        return "0:00".to_string();
    }
    let total_seconds = samples as f64 / sample_rate as f64;
    let minutes = (total_seconds / 60.0).floor() as u32;
    let seconds = (total_seconds % 60.0).floor() as u32;
    format!("{}:{:02}", minutes, seconds)
}

pub fn stem_color_for_classif(classif: nullherz_traits::StemClassification) -> Color32 {
    match classif {
        nullherz_traits::StemClassification::Kick | nullherz_traits::StemClassification::Snare | nullherz_traits::StemClassification::Clap | nullherz_traits::StemClassification::Hat | nullherz_traits::StemClassification::Percussion => Color32::from_rgb(0, 220, 255),
        nullherz_traits::StemClassification::Bass => Color32::from_rgb(255, 215, 0),
        nullherz_traits::StemClassification::LeadVocal | nullherz_traits::StemClassification::BackingVocal | nullherz_traits::StemClassification::Vocal => Color32::from_rgb(255, 105, 180),
        _ => Color32::from_rgb(147, 112, 219),
    }
}

pub fn stem_label_for_classif(classif: nullherz_traits::StemClassification) -> &'static str {
    match classif {
        nullherz_traits::StemClassification::Kick => "Kick",
        nullherz_traits::StemClassification::Snare => "Snare",
        nullherz_traits::StemClassification::Clap => "Clap",
        nullherz_traits::StemClassification::Hat => "Hat",
        nullherz_traits::StemClassification::Percussion => "Perc",
        nullherz_traits::StemClassification::Bass => "Bass",
        nullherz_traits::StemClassification::LeadVocal => "Lead Voc",
        nullherz_traits::StemClassification::BackingVocal => "Back Voc",
        nullherz_traits::StemClassification::Vocal => "Vocal",
        nullherz_traits::StemClassification::Guitar => "Guitar",
        nullherz_traits::StemClassification::PianoKeys => "Keys",
        nullherz_traits::StemClassification::SynthPad => "Synth",
        nullherz_traits::StemClassification::BrassStrings => "Brass",
        _ => "Other",
    }
}

pub fn dispatch_stem_param(app: &mut InspectorApp, deck_idx: usize, stem_idx: usize, control_type: u32, value: f32) {
    let target_node = app.get_node_id(&format!("deck_{}_stem_matrix", (b'a' + (deck_idx % 26) as u8) as char))
        .or_else(|| app.get_node_id(&format!("deck_{}_dna_slot", (b'a' + (deck_idx % 26) as u8) as char)))
        .or_else(|| app.get_node_id(&format!("deck_{}_sampler", (b'a' + (deck_idx % 26) as u8) as char)));
    if let Some(node) = target_node {
        let param_id = (stem_idx * 10) as u32 + control_type;
        let _ = app.command_sender.send(nullherz_traits::Command::Mixer(
            nullherz_traits::MixerCommand::SetParam {
                target_id: node as u64,
                param_id,
                value,
                ramp_duration_samples: 0,
            }
        ));
    }
}

fn apply_stem_macro_acapella(app: &mut InspectorApp, deck_idx: usize, stem_set: &nullherz_traits::StemSetMetadata) {
    for (s_idx, single_stem) in stem_set.stems.iter().enumerate().take(12) {
        let is_vocal = matches!(
            single_stem.classification,
            nullherz_traits::StemClassification::LeadVocal
                | nullherz_traits::StemClassification::BackingVocal
                | nullherz_traits::StemClassification::Vocal
        );
        app.mixer.stem_solos[deck_idx][s_idx] = is_vocal;
        app.mixer.stem_mutes[deck_idx][s_idx] = !is_vocal;
        dispatch_stem_param(app, deck_idx, s_idx, 1, if is_vocal { 1.0 } else { 0.0 });
        dispatch_stem_param(app, deck_idx, s_idx, 0, if !is_vocal { 1.0 } else { 0.0 });
    }
}

fn apply_stem_macro_instrumental(app: &mut InspectorApp, deck_idx: usize, stem_set: &nullherz_traits::StemSetMetadata) {
    for (s_idx, single_stem) in stem_set.stems.iter().enumerate().take(12) {
        let is_vocal = matches!(
            single_stem.classification,
            nullherz_traits::StemClassification::LeadVocal
                | nullherz_traits::StemClassification::BackingVocal
                | nullherz_traits::StemClassification::Vocal
        );
        app.mixer.stem_mutes[deck_idx][s_idx] = is_vocal;
        app.mixer.stem_solos[deck_idx][s_idx] = false;
        dispatch_stem_param(app, deck_idx, s_idx, 0, if is_vocal { 1.0 } else { 0.0 });
        dispatch_stem_param(app, deck_idx, s_idx, 1, 0.0);
    }
}

fn apply_stem_macro_drums_bass(app: &mut InspectorApp, deck_idx: usize, stem_set: &nullherz_traits::StemSetMetadata) {
    for (s_idx, single_stem) in stem_set.stems.iter().enumerate().take(12) {
        let is_rythm_bass = matches!(
            single_stem.classification,
            nullherz_traits::StemClassification::Kick
                | nullherz_traits::StemClassification::Snare
                | nullherz_traits::StemClassification::Clap
                | nullherz_traits::StemClassification::Hat
                | nullherz_traits::StemClassification::Percussion
                | nullherz_traits::StemClassification::Bass
        );
        app.mixer.stem_solos[deck_idx][s_idx] = is_rythm_bass;
        app.mixer.stem_mutes[deck_idx][s_idx] = !is_rythm_bass;
        dispatch_stem_param(app, deck_idx, s_idx, 1, if is_rythm_bass { 1.0 } else { 0.0 });
        dispatch_stem_param(app, deck_idx, s_idx, 0, if !is_rythm_bass { 1.0 } else { 0.0 });
    }
}

fn apply_stem_macro_reset_all(app: &mut InspectorApp, deck_idx: usize, stem_set: &nullherz_traits::StemSetMetadata) {
    for (s_idx, _) in stem_set.stems.iter().enumerate().take(12) {
        app.mixer.stem_mutes[deck_idx][s_idx] = false;
        app.mixer.stem_solos[deck_idx][s_idx] = false;
        app.mixer.stem_gains[deck_idx][s_idx] = 0.0;
        app.mixer.stem_pans[deck_idx][s_idx] = 0.0;
        app.mixer.stem_eq_low[deck_idx][s_idx] = 1.0;
        app.mixer.stem_eq_mid[deck_idx][s_idx] = 1.0;
        app.mixer.stem_eq_high[deck_idx][s_idx] = 1.0;

        dispatch_stem_param(app, deck_idx, s_idx, 0, 0.0);
        dispatch_stem_param(app, deck_idx, s_idx, 1, 0.0);
        dispatch_stem_param(app, deck_idx, s_idx, 2, 0.0);
        dispatch_stem_param(app, deck_idx, s_idx, 3, 0.0);
        dispatch_stem_param(app, deck_idx, s_idx, 4, 1.0);
        dispatch_stem_param(app, deck_idx, s_idx, 5, 1.0);
        dispatch_stem_param(app, deck_idx, s_idx, 6, 1.0);
    }
}

pub fn render_expanded_stem_matrix(app: &mut InspectorApp, ui: &mut Ui, i: usize, _deck_color: Color32) {
    let theme = app.theme;
    let track = app.decks.cached_tracks[i].clone();
    let Some(t) = track else { return; };
    let Some(stem_set) = t.stems else { return; };

    Frame::none()
        .fill(theme.bg_inset)
        .stroke(theme.border_stroke)
        .rounding(Rounding::same(theme.radius_sm))
        .inner_margin(Margin::symmetric(theme.space_sm, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("STEM MATRIX (12-BAND ISOLATOR & ROUTING)").size(theme.type_caption).strong().color(theme.accent));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new("RESET ALL").size(9.0)).clicked() {
                        apply_stem_macro_reset_all(app, i, &stem_set);
                    }
                    if ui.button(RichText::new("DRUMS+BASS").size(9.0)).clicked() {
                        apply_stem_macro_drums_bass(app, i, &stem_set);
                    }
                    if ui.button(RichText::new("INSTRUMENTAL").size(9.0)).clicked() {
                        apply_stem_macro_instrumental(app, i, &stem_set);
                    }
                    if ui.button(RichText::new("ACAPELLA").size(9.0)).clicked() {
                        apply_stem_macro_acapella(app, i, &stem_set);
                    }
                });
            });

            ui.add_space(2.0);

            egui::ScrollArea::horizontal()
                .id_source(format!("stem_matrix_scroll_{}", i))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (s_idx, single_stem) in stem_set.stems.iter().enumerate().take(12) {
                            let stem_color = stem_color_for_classif(single_stem.classification);
                            let label = stem_label_for_classif(single_stem.classification);

                            Frame::none()
                                .fill(theme.bg_surface)
                                .stroke(Stroke::new(1.0_f32, theme.border))
                                .rounding(Rounding::same(theme.radius_sm))
                                .inner_margin(Margin::symmetric(6.0, 4.0))
                                .show(ui, |ui| {
                                    ui.set_width(110.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(label).strong().size(theme.type_caption).color(stem_color));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                let is_solo = app.mixer.stem_solos[i][s_idx];
                                                if ui.selectable_label(is_solo, RichText::new("S").size(9.0)).clicked() {
                                                    let new_solo = !is_solo;
                                                    app.mixer.stem_solos[i][s_idx] = new_solo;
                                                    dispatch_stem_param(app, i, s_idx, 1, if new_solo { 1.0 } else { 0.0 });
                                                }
                                                let is_muted = app.mixer.stem_mutes[i][s_idx];
                                                if ui.selectable_label(is_muted, RichText::new("M").size(9.0)).clicked() {
                                                    let new_mute = !is_muted;
                                                    app.mixer.stem_mutes[i][s_idx] = new_mute;
                                                    dispatch_stem_param(app, i, s_idx, 0, if new_mute { 1.0 } else { 0.0 });
                                                }
                                            });
                                        });

                                        ui.add_space(2.0);

                                        // Gain slider
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("VOL").size(8.0).color(theme.text_disabled));
                                            let mut gain = app.mixer.stem_gains[i][s_idx];
                                            if ui.add(egui::Slider::new(&mut gain, -24.0..=12.0).show_value(false)).changed() {
                                                app.mixer.stem_gains[i][s_idx] = gain;
                                                dispatch_stem_param(app, i, s_idx, 2, gain);
                                            }
                                        });

                                        // 3-Band Isolator EQ (Low, Mid, High)
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("EQ").size(8.0).color(theme.text_disabled));
                                            let mut low = app.mixer.stem_eq_low[i][s_idx];
                                            if ui.add(egui::DragValue::new(&mut low).range(0.0..=5.0).speed(0.05).prefix("L:")).changed() {
                                                app.mixer.stem_eq_low[i][s_idx] = low;
                                                dispatch_stem_param(app, i, s_idx, 4, low);
                                            }
                                            let mut mid = app.mixer.stem_eq_mid[i][s_idx];
                                            if ui.add(egui::DragValue::new(&mut mid).range(0.0..=5.0).speed(0.05).prefix("M:")).changed() {
                                                app.mixer.stem_eq_mid[i][s_idx] = mid;
                                                dispatch_stem_param(app, i, s_idx, 5, mid);
                                            }
                                            let mut high = app.mixer.stem_eq_high[i][s_idx];
                                            if ui.add(egui::DragValue::new(&mut high).range(0.0..=5.0).speed(0.05).prefix("H:")).changed() {
                                                app.mixer.stem_eq_high[i][s_idx] = high;
                                                dispatch_stem_param(app, i, s_idx, 6, high);
                                            }
                                        });

                                        // Pan
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("PAN").size(8.0).color(theme.text_disabled));
                                            let mut pan = app.mixer.stem_pans[i][s_idx];
                                            if ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false)).changed() {
                                                app.mixer.stem_pans[i][s_idx] = pan;
                                                dispatch_stem_param(app, i, s_idx, 3, pan);
                                            }
                                        });
                                    });
                                });
                        }
                    });
                });
        });
}

pub fn render_time_display(ui: &mut egui::Ui, elapsed: &str, remaining: &str, accent_color: Color32, theme: &nullherz_ui_hal::Theme) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(elapsed).monospace().size(13.0).color(theme.text_secondary));
        ui.add_space(theme.space_sm);
        ui.label(egui::RichText::new(format!("-{}", remaining)).monospace().size(13.0).color(accent_color));
    });
}

fn render_waveform_lane(app: &mut InspectorApp, ui: &mut Ui, i: usize, lane_h: f32, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, i);
    let is_focused = app.decks.focused_deck == i;

    let bg_color = if is_focused {
        theme.bg_surface
    } else {
        theme.bg_canvas
    };

    let stroke_color = if is_focused {
        deck_color
    } else {
        theme.border
    };

    let border_thickness = if is_focused { 1.5 } else { 1.0 };
    let header_h = 22.0;
    let is_stem_expanded = app.mixer.stem_controls_expanded[i];
    let stem_panel_h = if is_stem_expanded { 110.0 } else { 0.0 };

    let response = Frame::none()
        .fill(bg_color)
        .stroke(Stroke::new(border_thickness, stroke_color))
        .rounding(Rounding::same(theme.radius_sm))
        .inner_margin(Margin::same(0.0))
        .show(ui, |ui| {
            ui.set_height(lane_h);
            ui.vertical(|ui| {
                // Condensed Header Strip
                render_condensed_deck_header(app, ui, i, deck_color, is_focused, telemetry);

                if is_stem_expanded {
                    render_expanded_stem_matrix(app, ui, i, deck_color);
                }

                // Waveform Zone
                let remaining_wf_h = (lane_h - header_h - stem_panel_h - 2.0 * border_thickness).max(20.0);
                waveform::render_deck_waveform_zone(app, ui, i, telemetry, deck_color, remaining_wf_h);
            });
        });

    // Draw left vertical accent bar inside the lane bounds
    let rect = response.response.rect;
    let bar_width = if is_focused { 4.0 } else { 1.5 };
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, rect.top() + 1.0),
        egui::pos2(rect.left() + 1.0 + bar_width, rect.bottom() - 1.0)
    );
    let bar_color = if is_focused { deck_color } else { theme.border };
    ui.painter().rect_filled(bar_rect, Rounding::ZERO, bar_color);

    let header_rect = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.max.x, (rect.min.y + header_h).min(rect.max.y)),
    );
    if ui.interact(header_rect, ui.id().with(format!("lane_click_{i}")), egui::Sense::click()).clicked() {
        app.decks.focused_deck = i;
    }
}

fn render_condensed_deck_header(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32, is_focused: bool, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let deck_id_label = (b'A' + i as u8) as char;
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        // Left padding for the left accent bar
        ui.add_space(theme.space_sm);

        // Deck label
        let label_text = RichText::new(format!("DECK {}", deck_id_label)).strong().size(theme.type_caption).color(if is_focused { deck_color } else { theme.text_secondary });
        if ui.selectable_label(is_focused, label_text).clicked() {
            app.decks.focused_deck = i;
        }

        ui.add_space(theme.space_xs);

        // Master Deck Toggle ("M")
        let is_master = app.decks.master_deck == Some(i);
        let m_color = if is_master { deck_color } else { theme.text_disabled };
        if ui.selectable_label(is_master, RichText::new("M").strong().size(theme.type_caption).color(m_color)).clicked() {
             app.decks.master_deck = Some(i);
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMasterDeck(deck_id_label)));
        }

        ui.add_space(theme.space_xs);

        // Sync toggle
        let is_sync = app.mixer.channel_sync[i];
        let sync_color = if is_sync { theme.accent } else { theme.text_disabled };
        if ui.selectable_label(is_sync, RichText::new("S").strong().size(theme.type_caption).color(sync_color)).clicked() {
            app.mixer.channel_sync[i] = !is_sync;
            if let Some(node) = app.get_node_id(&format!("deck_{}_sampler", (b'a' + i as u8) as char)) {
                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: node as u64,
                    param_id: 2,
                    value: if app.mixer.channel_sync[i] { 1.0 } else { 0.0 },
                    ramp_duration_samples: 0,
                }));
            }
        }

        ui.add_space(theme.space_sm);

        // Track metadata block
        let track = app.decks.cached_tracks[i].clone();

        if let Some(ref t) = track {
            let is_loading = telemetry.as_ref().map(|tel| tel.hydration_pending[i] == t.id).unwrap_or(false);
            let progress = if is_loading {
                telemetry.as_ref().map(|tel| tel.hydration_progress[i]).unwrap_or(0.0)
            } else {
                1.0
            };

            let truncate = |s: &str, max: usize| -> String {
                if s.chars().count() > max {
                    format!("{}...", s.chars().take(max.saturating_sub(2)).collect::<String>())
                } else {
                    s.to_string()
                }
            };
            let title_text = truncate(&t.title, 20);
            if is_loading {
                ui.label(RichText::new(format!("⏳ {}", title_text)).strong().size(theme.type_caption).color(theme.warning));
            } else {
                ui.label(RichText::new(title_text).strong().size(theme.type_caption).color(theme.text_primary));
            }

            let artist_text = if t.artist.chars().count() > 15 {
                format!("by {}...", t.artist.chars().take(13).collect::<String>())
            } else {
                format!("by {}", t.artist)
            };
            ui.label(RichText::new(artist_text).size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_sm);

            // Live BPM
            let playback_rate = telemetry.as_ref().map(|t| t.deck_playback_rates[i]).unwrap_or(1.0);
            let live_bpm = t.metadata.bpm * playback_rate;
            ui.label(RichText::new(format!("{:.1}", live_bpm)).monospace().strong().size(theme.type_caption).color(deck_color));
            ui.label(RichText::new("BPM").size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_sm);

            // Native track key & genre
            let mut meta_text = String::new();
            if let Some(key) = t.metadata.root_key {
                meta_text.push_str(&format!("K:{:.0} ", key));
            }
            if !t.genre.is_empty() {
                meta_text.push_str(&format!("G:{}", t.genre));
            }
            ui.label(RichText::new(meta_text).size(theme.type_caption).color(theme.text_secondary));

            // Interactive Stem Strip Toggle Controls
            if let Some(ref stem_set) = t.stems {
                ui.add_space(theme.space_xs);
                let count = stem_set.stems.len();
                let is_expanded = app.mixer.stem_controls_expanded[i];
                let btn_text = if is_expanded {
                    format!("🎛 STEMS ({}) ▲", count)
                } else {
                    format!("🎛 STEMS ({}) ▼", count)
                };
                if ui.button(RichText::new(btn_text).size(theme.type_caption).color(theme.accent).strong()).clicked() {
                    app.mixer.stem_controls_expanded[i] = !app.mixer.stem_controls_expanded[i];
                }

                // Quick mute/solo chips for first 4 main stems in header
                for (s_idx, single_stem) in stem_set.stems.iter().enumerate().take(4) {
                    let classif_name = stem_label_for_classif(single_stem.classification);
                    let stem_color = stem_color_for_classif(single_stem.classification);
                    ui.label(RichText::new(classif_name).size(9.0).color(stem_color).strong());

                    let is_muted = app.mixer.stem_mutes[i][s_idx];
                    let is_solo = app.mixer.stem_solos[i][s_idx];

                    if ui.selectable_label(is_muted, RichText::new("M").size(9.0)).clicked() {
                        let new_mute = !is_muted;
                        app.mixer.stem_mutes[i][s_idx] = new_mute;
                        dispatch_stem_param(app, i, s_idx, 0, if new_mute { 1.0 } else { 0.0 });
                    }
                    if ui.selectable_label(is_solo, RichText::new("S").size(9.0)).clicked() {
                        let new_solo = !is_solo;
                        app.mixer.stem_solos[i][s_idx] = new_solo;
                        dispatch_stem_param(app, i, s_idx, 1, if new_solo { 1.0 } else { 0.0 });
                    }
                }
            }

            // Time Display & Waveform Mode on the far right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(theme.space_xs); // right padding
                if is_loading {
                    ui.add(egui::ProgressBar::new(progress).desired_width(120.0).text(format!("LOADING {:.0}%", progress * 100.0)).fill(theme.accent));
                } else {
                    let sample_rate = t.metadata.sample_rate.max(1) as f32;
                    let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[i]).unwrap_or(0);
                    let total_samples = t.metadata.total_samples;

                    let elapsed_str = format_duration(elapsed_samples, sample_rate);
                    let remaining_samples = total_samples.saturating_sub(elapsed_samples);
                    let remaining_str = format_duration(remaining_samples, sample_rate);

                    render_time_display(ui, &elapsed_str, &remaining_str, deck_color, &theme);

                    ui.add_space(theme.space_xs);

                    // Waveform Mode Selector Chip
                    egui::ComboBox::from_id_source(format!("deck_wf_mode_cb_{}", i))
                        .selected_text(egui::RichText::new(format!("WF: {}", app.mixer.deck_waveform_mode[i].short_code())).size(9.0).strong().color(deck_color))
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            use crate::state::DeckWaveformMode;
                            for m in DeckWaveformMode::all() {
                                ui.selectable_value(&mut app.mixer.deck_waveform_mode[i], *m, m.name());
                            }
                        });
                }
            });
        } else {
            ui.label(RichText::new("NO TRACK LOADED").monospace().color(theme.text_disabled).size(theme.type_caption));
        }
    });
}
