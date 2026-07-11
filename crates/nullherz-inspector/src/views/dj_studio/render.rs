use nullherz_dna::GeneticLibrary;
use egui::{Ui, Color32, RichText, Frame, Margin, Rounding, Stroke, ScrollArea, Vec2};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

use super::{mixer, dna, transport, performance, waveform};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    if app.focused_deck >= 4 {
        app.focused_deck = 0;
    }
    let theme = app.theme;

    // Compute total available height on the finite, constrained parent UI (outside of any ScrollArea)
    let total_h = ui.available_height().max(500.0);

    render_header(ui, telemetry, &theme);
    ui.add_space(4.0);

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

    ui.add_space(4.0);

    // Mixer section (bottom half of the window)
    // Wrapped in a nested vertical ScrollArea so the mixer strips and master section scroll independently
    // within the remaining available height of the central panel.
    ScrollArea::vertical().id_source("mixer_scroll").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for i in 0..4 {
                render_channel_strip(app, ui, i, telemetry);
                if i < 3 {
                    // Vertical divider between channels
                    let (line_rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 320.0), egui::Sense::hover());
                    ui.painter().rect_filled(line_rect, Rounding::ZERO, Color32::from_rgb(22, 22, 26));
                    ui.add_space(8.0);
                }
            }
        });

        ui.add_space(12.0);
        render_master_section(app, ui, telemetry);
    });
}

fn render_header(ui: &mut Ui, telemetry: &Option<Telemetry>, theme: &nullherz_ui_hal::Theme) {
    Frame::none()
        .fill(Color32::from_rgb(10, 10, 12))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(16.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("NULLHERZ DJ CONSOLE").strong().color(theme.text_primary).size(15.0).extra_letter_spacing(1.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(t) = telemetry {
                        ui.label(RichText::new("BPM").small().color(Color32::from_gray(100)));
                        ui.label(RichText::new(format!("{:.1}", t.bpm)).monospace().strong().color(theme.accent).size(15.0));
                    }
                });
            });
        });
}

fn render_waveform_lane(app: &mut InspectorApp, ui: &mut Ui, i: usize, lane_h: f32, telemetry: &Option<Telemetry>) {
    let deck_color = InspectorApp::deck_color(i);
    let is_focused = app.focused_deck == i;

    let bg_color = if is_focused {
        Color32::from_rgb(14, 14, 16)
    } else {
        Color32::from_rgb(10, 10, 12)
    };

    let stroke_color = if is_focused {
        deck_color
    } else {
        Color32::from_gray(25)
    };

    let border_thickness = if is_focused { 1.5 } else { 1.0 };
    let header_h = 22.0;

    let response = Frame::none()
        .fill(bg_color)
        .stroke(Stroke::new(border_thickness, stroke_color))
        .rounding(Rounding::same(4.0))
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
    let bar_color = if is_focused { deck_color } else { Color32::from_gray(30) };
    ui.painter().rect_filled(bar_rect, Rounding::ZERO, bar_color);

    // Clicking a lane sets app.focused_deck = i
    let lane_clicked = ui.interact(rect, ui.id().with(format!("lane_click_{}", i)), egui::Sense::click()).clicked();
    if lane_clicked {
        app.focused_deck = i;
    }
}

fn render_condensed_deck_header(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32, is_focused: bool, telemetry: &Option<Telemetry>) {
    let deck_id_label = (b'A' + i as u8) as char;
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        // Left padding for the left accent bar
        ui.add_space(8.0);

        // Deck label
        let label_text = RichText::new(format!("DECK {}", deck_id_label)).strong().size(11.0).color(if is_focused { deck_color } else { Color32::from_gray(140) });
        if ui.selectable_label(is_focused, label_text).clicked() {
            app.focused_deck = i;
        }

        ui.add_space(6.0);

        // Animated Phase Ring next to deck ID representing beat position
        let (ring_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
        let center = ring_rect.center();
        let radius = 7.0;
        ui.painter().circle_stroke(center, radius, Stroke::new(1.0, Color32::from_gray(50)));
        let beat_pos = telemetry.as_ref().map(|t| t.beat_position).unwrap_or(0.0);
        let angle = (beat_pos % 1.0) as f32 * 2.0 * std::f32::consts::PI;
        let indicator_pos = center + Vec2::new(angle.sin(), -angle.cos()) * radius;
        ui.painter().circle_filled(indicator_pos, 2.0, deck_color);

        ui.add_space(6.0);

        // Master Deck Toggle ("M")
        let is_master = app.master_deck == Some(i);
        let m_color = if is_master { deck_color } else { Color32::from_gray(80) };
        if ui.selectable_label(is_master, RichText::new("M").strong().size(10.0).color(m_color)).clicked() {
             app.master_deck = Some(i);
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMasterDeck(deck_id_label)));
        }

        ui.add_space(4.0);

        // Sync toggle
        let is_sync = app.channel_sync[i];
        let sync_color = if is_sync { app.theme.accent } else { Color32::from_gray(80) };
        if ui.selectable_label(is_sync, RichText::new("S").strong().size(10.0).color(sync_color)).clicked() {
            app.channel_sync[i] = !is_sync;
        }

        ui.add_space(8.0);

        // Track metadata block
        let track_id = app.now_playing[i];
        let track = track_id.and_then(|id| app.library_db.get_track(id).ok().flatten());

        if let Some(ref t) = track {
            // Track Title & Artist
            let title_text = if t.title.len() > 20 {
                format!("{}...", &t.title[..18])
            } else {
                t.title.clone()
            };
            ui.label(RichText::new(title_text).strong().size(11.0).color(Color32::WHITE));

            let artist_text = if t.artist.len() > 15 {
                format!("by {}...", &t.artist[..13])
            } else {
                format!("by {}", t.artist)
            };
            ui.label(RichText::new(artist_text).size(9.0).color(Color32::from_gray(140)));

            ui.add_space(8.0);

            // Live BPM
            let playback_rate = telemetry.as_ref().map(|t| t.deck_playback_rates[i]).unwrap_or(1.0);
            let live_bpm = t.metadata.bpm * playback_rate;
            ui.label(RichText::new(format!("{:.1}", live_bpm)).monospace().strong().size(11.0).color(deck_color));
            ui.label(RichText::new("BPM").size(8.0).color(Color32::from_gray(100)));

            ui.add_space(8.0);

            // Native track key & genre
            let mut meta_text = String::new();
            if let Some(key) = t.metadata.root_key {
                meta_text.push_str(&format!("K:{:.0} ", key));
            }
            if !t.genre.is_empty() {
                meta_text.push_str(&format!("G:{}", t.genre));
            }
            ui.label(RichText::new(meta_text).size(8.0).color(Color32::from_gray(120)));

            // Time Display on the far right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0); // right padding
                let sample_rate = telemetry.as_ref().map(|t| t.sample_rate).unwrap_or(44100.0);
                let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[i]).unwrap_or(0);
                let total_samples = t.metadata.total_samples;

                let elapsed_str = widgets::format_duration(elapsed_samples, sample_rate);
                let remaining_samples = if total_samples >= elapsed_samples { total_samples - elapsed_samples } else { 0 };
                let remaining_str = widgets::format_duration(remaining_samples, sample_rate);

                widgets::render_time_display(ui, &elapsed_str, &remaining_str, deck_color);
            });
        } else {
            ui.label(RichText::new("NO TRACK LOADED").monospace().color(Color32::from_gray(60)).size(9.0));
        }
    });
}

fn render_channel_strip(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let deck_color = InspectorApp::deck_color(i);
    let is_focused = app.focused_deck == i;

    let bg_color = if is_focused {
        Color32::from_rgb(14, 14, 16)
    } else {
        Color32::from_rgb(10, 10, 12)
    };

    let stroke_color = if is_focused {
        deck_color
    } else {
        Color32::from_gray(25)
    };

    let border_thickness = if is_focused { 1.5 } else { 1.0 };

    ui.allocate_ui_with_layout(Vec2::new(140.0, ui.available_height()), egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.set_width(140.0);
        Frame::none()
            .fill(bg_color)
            .stroke(Stroke::new(border_thickness, stroke_color))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    // Header (Selectable title/indicator for focus)
                    let deck_id_label = (b'A' + i as u8) as char;
                    let label_text = RichText::new(format!("CH {}", deck_id_label)).strong().size(12.0).color(if is_focused { deck_color } else { Color32::from_gray(140) });
                    if ui.selectable_label(is_focused, label_text).clicked() {
                        app.focused_deck = i;
                    }
                    ui.add_space(4.0);

                    // 1. Hot cues (2x4)
                    performance::render_deck_performance(app, ui, i, telemetry);
                    ui.add_space(8.0);

                    // Divider (Native separator)
                    ui.separator();
                    ui.add_space(8.0);

                    // 2 & 3. EQ + Filter Column & Volume Row
                    mixer::render_deck_mixer(app, ui, i, deck_color);
                    ui.add_space(8.0);

                    // Divider (Native separator)
                    ui.separator();
                    ui.add_space(8.0);

                    // 4. Transport Row
                    transport::render_deck_transport(app, ui, i);
                    ui.add_space(8.0);

                    // 5. Collapsible DNA panel (collapsed by default)
                    egui::CollapsingHeader::new(RichText::new("DNA").small().color(Color32::from_gray(120)))
                        .default_open(false)
                        .show(ui, |ui| {
                            dna::render_deck_dna_panel(app, ui, i);
                        });
                });
            });
    });
}

fn render_master_section(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    Frame::group(ui.style())
        .fill(Color32::from_rgb(10, 10, 12))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                // Crossfader
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 140.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("CROSSFADER").small().color(Color32::from_gray(100)));
                        if widgets::render_horizontal_fader(ui, &mut app.crossfader_pos, 0.0..=1.0, Color32::WHITE, ui.available_width(), 32.0).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: app.get_node_id("master_crossfader") as u64,
                                param_id: 0,
                                value: app.crossfader_pos,
                                ramp_duration_samples: 0,
                            }));
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("A").small().color(Color32::from_gray(100)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new("B").small().color(Color32::from_gray(100)));
                            });
                        });
                    });
                });

                ui.add_space(20.0);

                // Master Out
                ui.vertical(|ui| {
                    ui.label(RichText::new("MASTER").strong().size(11.0).color(Color32::from_gray(180)));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let peak = (app.damped_master_peaks[0] + app.damped_master_peaks[1]) * 0.5;
                        widgets::render_vu_meter(ui, peak, app.master_peak_hold, Color32::WHITE, 120.0);
                        if widgets::render_fader(ui, &mut app.master_gain, 0.0..=1.5, Color32::WHITE, 120.0, 18.0).changed() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: app.get_node_id("master_sum") as u64,
                                param_id: 0,
                                value: app.master_gain,
                                ramp_duration_samples: 0,
                            }));
                        }
                    });
                });
            });
        });
}
