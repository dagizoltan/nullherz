use nullherz_dna::GeneticLibrary;
use egui::{Ui, Color32, RichText, Frame, Margin, Rounding, Stroke, ScrollArea, Vec2};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

use super::{mixer, dna, transport, performance, waveform};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ScrollArea::vertical().id_source("console_scroll").show(ui, |ui| {
        render_header(ui, telemetry, &theme);
        ui.add_space(8.0);

        // 4-Deck Accordion-style Vertical Stack
        ui.vertical(|ui| {
            for i in 0..4 {
                render_deck_card(app, ui, i, telemetry);
                ui.add_space(8.0); // Consistent spacing rhythm
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

fn render_deck_card(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let deck_color = InspectorApp::deck_color(i);
    let is_focused = app.focused_deck == i;

    // Soft elevation via tone shifts
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

    let response = Frame::none()
        .fill(bg_color)
        .stroke(Stroke::new(border_thickness, stroke_color))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(16.0, 10.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Deck Control Header
                render_deck_header(app, ui, i, deck_color, is_focused);
                ui.add_space(6.0);

                // Track & Time Information Block (OLED Display Style)
                let track_id = app.now_playing[i];
                let track = track_id.and_then(|id| app.library_db.get_track(id).ok().flatten());

                if let Some(ref t) = track {
                    Frame::none()
                        .fill(Color32::from_rgb(6, 6, 8))
                        .rounding(Rounding::same(4.0))
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    // Track Title
                                    let title_text = if t.title.len() > 32 {
                                        format!("{}...", &t.title[..30])
                                    } else {
                                        t.title.clone()
                                    };
                                    ui.label(RichText::new(title_text).strong().size(13.0).color(Color32::WHITE));

                                    // Artist
                                    ui.add_space(8.0);
                                    let artist_text = if t.artist.len() > 20 {
                                        format!("by {}...", &t.artist[..18])
                                    } else {
                                        format!("by {}", t.artist)
                                    };
                                    ui.label(RichText::new(artist_text).size(10.0).color(Color32::from_gray(140)));
                                });

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    // Live deck BPM
                                    let sample_rate = telemetry.as_ref().map(|t| t.sample_rate).unwrap_or(44100.0);
                                    let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[i]).unwrap_or(0);
                                    let total_samples = t.metadata.total_samples;

                                    let elapsed_str = widgets::format_duration(elapsed_samples, sample_rate);
                                    let remaining_samples = if total_samples >= elapsed_samples { total_samples - elapsed_samples } else { 0 };
                                    let remaining_str = widgets::format_duration(remaining_samples, sample_rate);

                                    let playback_rate = telemetry.as_ref().map(|t| t.deck_playback_rates[i]).unwrap_or(1.0);
                                    let live_bpm = t.metadata.bpm * playback_rate;

                                    ui.label(RichText::new(format!("{:.1}", live_bpm)).monospace().strong().size(14.0).color(deck_color));
                                    ui.label(RichText::new("LIVE BPM").small().color(Color32::from_gray(100)));

                                    ui.add_space(12.0);

                                    // Native track key & genre
                                    let mut meta_text = String::new();
                                    if let Some(key) = t.metadata.root_key {
                                        meta_text.push_str(&format!("KEY: {:.0}  ", key));
                                    }
                                    if !t.genre.is_empty() {
                                        meta_text.push_str(&format!("GENRE: {}", t.genre));
                                    }
                                    ui.label(RichText::new(meta_text).size(9.0).color(Color32::from_gray(120)));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        widgets::render_time_display(ui, &elapsed_str, &remaining_str, deck_color);
                                    });
                                });
                            });
                        });
                } else {
                    // Empty deck indicator screen
                    Frame::none()
                        .fill(Color32::from_rgb(6, 6, 8))
                        .stroke(Stroke::new(1.0, Color32::from_gray(20)))
                        .rounding(Rounding::same(4.0))
                        .inner_margin(Margin::symmetric(12.0, 12.0))
                        .show(ui, |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label(RichText::new("NO TRACK LOADED").monospace().color(Color32::from_gray(60)).size(10.0));
                            });
                        });
                }

                ui.add_space(6.0);

                // Waveform Zone
                waveform::render_deck_waveform_zone(app, ui, i, telemetry, deck_color);

                // ACCORDION CONTROL: Only render complete functional controls if the deck is OPEN/FOCUSED
                if is_focused {
                    ui.add_space(10.0);

                    // Horizontal Divider
                    let (line_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
                    ui.painter().rect_filled(line_rect, Rounding::ZERO, Color32::from_rgb(22, 22, 26));

                    ui.add_space(10.0);

                    // Hardware Controls Grid (Transport -> Performance -> Mixer -> DNA)
                    ui.horizontal_top(|ui| {
                        // 1. Transport Control Section
                        transport::render_deck_transport(app, ui, i);
                        ui.add_space(12.0);

                        // Vertical Divider
                        let (line_rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 140.0), egui::Sense::hover());
                        ui.painter().rect_filled(line_rect, Rounding::ZERO, Color32::from_rgb(22, 22, 26));
                        ui.add_space(12.0);

                        // 2. Performance (Hot cues)
                        performance::render_deck_performance(app, ui, i, telemetry);
                        ui.add_space(12.0);

                        // Vertical Divider
                        let (line_rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 140.0), egui::Sense::hover());
                        ui.painter().rect_filled(line_rect, Rounding::ZERO, Color32::from_rgb(22, 22, 26));
                        ui.add_space(12.0);

                        // 3. Mixer (Faders / EQs / Filters)
                        mixer::render_deck_mixer(app, ui, i, deck_color);
                        ui.add_space(12.0);

                        // Vertical Divider
                        let (line_rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 140.0), egui::Sense::hover());
                        ui.painter().rect_filled(line_rect, Rounding::ZERO, Color32::from_rgb(22, 22, 26));
                        ui.add_space(12.0);

                        // 4. DNA panel
                        dna::render_deck_dna_panel(app, ui, i);
                    });
                }
            });
        });

    // Draw left vertical accent bar inside the card bounds
    let rect = response.response.rect;
    let bar_width = if is_focused { 4.0 } else { 1.5 };
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, rect.top() + 1.0),
        egui::pos2(rect.left() + 1.0 + bar_width, rect.bottom() - 1.0)
    );
    let bar_color = if is_focused { deck_color } else { Color32::from_gray(30) };
    ui.painter().rect_filled(bar_rect, Rounding::ZERO, bar_color);

    // Accordion Interaction: clicking anywhere on a closed card focuses and expands it
    if !is_focused {
        let card_clicked = ui.interact(rect, ui.id().with(i), egui::Sense::click()).clicked();
        if card_clicked {
            app.focused_deck = i;
        }
    }
}

fn render_deck_header(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32, is_focused: bool) {
    ui.horizontal(|ui| {
        let deck_id_label = (b'A' + i as u8) as char;

        // Active/Focused Selection
        if ui.selectable_label(is_focused, RichText::new(format!("DECK {}", deck_id_label)).strong().size(13.0).color(if is_focused { deck_color } else { Color32::from_gray(140) })).clicked() {
            app.focused_deck = i;
        }

        ui.add_space(12.0);

        // Master Deck Toggle ("M")
        let is_master = app.master_deck == Some(i);
        let m_color = if is_master { deck_color } else { Color32::from_gray(80) };
        if ui.selectable_label(is_master, RichText::new("MASTER").strong().size(10.0).color(m_color)).clicked() {
             app.master_deck = Some(i);
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMasterDeck(deck_id_label)));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_sync = app.channel_sync[i];
            let sync_color = if is_sync { app.theme.accent } else { Color32::from_gray(80) };
            if ui.selectable_label(is_sync, RichText::new("SYNC").size(10.0).color(sync_color)).clicked() {
                app.channel_sync[i] = !is_sync;
            }
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
                        widgets::render_fader(ui, &mut app.master_gain, 0.0..=1.5, Color32::WHITE, 120.0, 18.0);
                    });
                });
            });
        });
}
