use egui::{Ui, Color32, RichText, Frame, Margin, Rounding, Stroke, ScrollArea, Vec2};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

use super::{mixer, dna, transport, performance, waveform};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    if app.decks.focused_deck >= 4 {
        app.decks.focused_deck = 0;
    }
    let theme = app.theme;

    // Compute total available height on the finite, constrained parent UI (outside of any ScrollArea)
    let total_h = ui.available_height().max(500.0);

    render_header(ui, telemetry, &theme);
    ui.add_space(theme.space_xs);

    // Waveform stack (top half of the window)
    // Consumes exactly 50% of the available central-panel height.
    // It sits OUTSIDE any ScrollArea so it remains persistent and visible at all times.
    let waveform_section_h = total_h * 0.5;
    let spacing_h = 2.0;
    let lane_h = (waveform_section_h - spacing_h * 3.0) / 4.0;

    ui.vertical(|ui| {
        for i in 0..4 {
            render_waveform_lane(app, ui, i, lane_h, telemetry);
            if i < 3 {
                ui.add_space(spacing_h);
            }
        }
    });

    ui.add_space(theme.space_xs);

    // Mixer section (bottom half of the window)
    // Wrapped in a nested vertical ScrollArea so the mixer strips and master section scroll independently
    // within the remaining available height of the central panel.
    // horizontal_wrapped: on a narrow window (library sidebar open) the
    // strips FLOW onto a second row instead of clipping off the right edge;
    // ScrollArea::both is the backstop for extreme sizes.
    ScrollArea::both().id_source("mixer_scroll").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(theme.space_sm, theme.space_sm);
            for i in 0..4 {
                render_channel_strip(app, ui, i, telemetry);
            }
            render_master_section(app, ui, telemetry);
        });
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
                ui.heading(RichText::new("NULLHERZ DJ CONSOLE").strong().color(theme.text_primary).size(theme.type_heading).extra_letter_spacing(1.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(t) = telemetry {
                        ui.label(RichText::new("BPM").size(theme.type_caption).color(theme.text_secondary));
                        ui.label(RichText::new(format!("{:.1}", t.bpm)).monospace().strong().color(theme.accent).size(theme.type_heading));
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

                // Waveform Zone
                let remaining_wf_h = (lane_h - header_h - 2.0 * border_thickness).max(10.0);
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

    // Clicking a lane sets app.decks.focused_deck = i
    let lane_clicked = ui.interact(rect, ui.id().with(format!("lane_click_{}", i)), egui::Sense::click()).clicked();
    if lane_clicked {
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
            // Wire it: S = the deck sampler's quantize/BPM-lock (param 2).
            // The toggle used to be a local bool that did nothing.
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
            // Track Title & Artist — char-safe truncation: byte-slicing
            // (&s[..18]) PANICS on a UTF-8 boundary, so any track title with
            // an accent or emoji at the wrong offset crashed the console.
            let truncate = |s: &str, max: usize| -> String {
                if s.chars().count() > max {
                    format!("{}...", s.chars().take(max.saturating_sub(2)).collect::<String>())
                } else {
                    s.to_string()
                }
            };
            let title_text = truncate(&t.title, 20);
            ui.label(RichText::new(title_text).strong().size(theme.type_caption).color(theme.text_primary));

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

            // Time Display on the far right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(theme.space_xs); // right padding
                let sample_rate = telemetry.as_ref().map(|t| t.sample_rate).unwrap_or(44100.0);
                let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[i]).unwrap_or(0);
                let total_samples = t.metadata.total_samples;

                let elapsed_str = format_duration(elapsed_samples, sample_rate);
                let remaining_samples = total_samples.saturating_sub(elapsed_samples);
                let remaining_str = format_duration(remaining_samples, sample_rate);

                render_time_display(ui, &elapsed_str, &remaining_str, deck_color, &theme);
            });
        } else {
            ui.label(RichText::new("NO TRACK LOADED").monospace().color(theme.text_disabled).size(theme.type_caption));
        }
    });
}

fn render_channel_strip(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
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

    Frame::none()
        .fill(bg_color)
        .stroke(Stroke::new(border_thickness, stroke_color))
        .rounding(theme.radius_md)
        .inner_margin(Margin::symmetric(theme.space_sm, theme.space_md))
        .show(ui, |ui| {
            ui.set_width(132.0);
            ui.vertical_centered(|ui| {
                    // Header (Selectable title/indicator for focus)
                    let deck_id_label = (b'A' + i as u8) as char;
                    let label_text = RichText::new(format!("CH {}", deck_id_label)).strong().size(theme.type_body).color(if is_focused { deck_color } else { theme.text_secondary });
                    if ui.selectable_label(is_focused, label_text).clicked() {
                        app.decks.focused_deck = i;
                    }
                    ui.add_space(theme.space_xs);

                    // 1. Hot cues (2x4)
                    performance::render_deck_performance(app, ui, i, telemetry);
                    ui.add_space(theme.space_sm);

                    // Divider (Native separator)
                    ui.separator();
                    ui.add_space(theme.space_sm);

                    // 2 & 3. EQ + Filter Column & Volume Row
                    mixer::render_deck_mixer(app, ui, i, deck_color);
                    ui.add_space(theme.space_sm);

                    // Divider (Native separator)
                    ui.separator();
                    ui.add_space(theme.space_sm);

                    // 4. Transport Row
                    transport::render_deck_transport(app, ui, i);
                    ui.add_space(theme.space_sm);

                    // 5. Collapsible DNA panel (collapsed by default)
                    egui::CollapsingHeader::new(RichText::new("DNA").size(theme.type_caption).color(theme.text_secondary))
                        .default_open(false)
                        .show(ui, |ui| {
                            dna::render_deck_dna_panel(app, ui, i);
                        });
                });
            });
}

fn render_stereo_vu_meter(ui: &mut Ui, peak_l: f32, peak_r: f32, peak_hold: f32, accent_color: Color32, height: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        widgets::render_vu_meter(ui, peak_l, peak_hold, accent_color, height);
        widgets::render_vu_meter(ui, peak_r, peak_hold, accent_color, height);
    });
}

fn render_master_section(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    // Width bounded INSIDE the frame: vertical_centered expands to the full
    // remaining width otherwise (the ballooning master card), and a
    // full-height container would stretch every wrapped row to panel height.
    Frame::none()
        .fill(theme.bg_surface)
        .stroke(theme.border_stroke)
        .rounding(theme.radius_md)
        .inner_margin(Margin::symmetric(theme.space_sm, theme.space_md))
        .show(ui, |ui| {
            ui.set_width(210.0);
            ui.vertical_centered(|ui| {

                // Crossfader — drives BOTH sides of the stereo crossfader
                // pair, resolved by name. (The old "master_crossfader" name
                // is only registered by the standalone builder; the lookup
                // fell back to node 0 = deck A's sampler.)
                ui.label(RichText::new("CROSSFADER").size(theme.type_caption).color(theme.text_secondary));
                ui.add_space(theme.space_xs);
                let r_cross = widgets::render_horizontal_fader(ui, &mut app.mixer.crossfader_pos, 0.0..=1.0, theme.text_primary, 160.0, 32.0);
                if r_cross.changed() {
                    for name in ["master_xf_l", "master_xf_r"] {
                        if let Some(&node) = app.topo.node_map.get(name) {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: node as u64,
                                param_id: 0,
                                value: app.mixer.crossfader_pos,
                                ramp_duration_samples: 0,
                            }));
                        }
                    }
                }
                if r_cross.drag_stopped() || r_cross.lost_focus() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                }
                ui.add_space(theme.space_xs);
                ui.horizontal(|ui| {
                    ui.add_space(theme.space_sm);
                    ui.label(RichText::new("A").size(theme.type_caption).color(theme.text_secondary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(theme.space_sm);
                        ui.label(RichText::new("B").size(theme.type_caption).color(theme.text_secondary));
                    });
                });

                ui.add_space(theme.space_sm);
                // Divider
                ui.separator();
                ui.add_space(theme.space_sm);

                // Master, Booth, and Rec Out with Stereo VU Meters
                ui.label(RichText::new("OUTPUTS").strong().size(theme.type_body).color(theme.text_secondary));
                ui.add_space(theme.space_xs);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;

                    // Master VU — the only real output pair in the graph.
                    // (The old BTH/REC meters displayed the master scaled by
                    // 0.8: there are no booth or record sends in the
                    // topology, and a meter that means nothing is worse than
                    // no meter. Build the sends first, then meter them.)
                    ui.vertical(|ui| {
                        ui.label(RichText::new("MST").size(theme.type_caption).color(theme.text_secondary));
                        render_stereo_vu_meter(ui, app.viz.damped_master_peaks[0], app.viz.damped_master_peaks[1], app.mixer.master_peak_hold, theme.text_primary, 100.0);
                    });

                    // Master Gain Fader — per-side summing gain, resolved by
                    // name. ("master_sum" never existed: get_node_id fell
                    // back to 0 and this fader set a deck A sampler param.)
                    ui.vertical(|ui| {
                        ui.label(RichText::new("GAIN").size(theme.type_caption).color(theme.text_secondary));
                        let r_master = widgets::render_fader(ui, &mut app.mixer.master_gain, 0.0..=1.5, theme.text_primary, 100.0, 18.0);
                        if r_master.changed() {
                            for name in ["master_sum_l", "master_sum_r"] {
                                if let Some(&node) = app.topo.node_map.get(name) {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                        target_id: node as u64,
                                        param_id: 0,
                                        value: app.mixer.master_gain,
                                        ramp_duration_samples: 0,
                                    }));
                                }
                            }
                        }
                        if r_master.drag_stopped() || r_master.lost_focus() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                        }
                    });
                });
            });
        });
}
