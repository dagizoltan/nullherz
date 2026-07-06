use egui::{Ui, Color32, RichText, Frame, Margin, Rounding, Stroke};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;

use super::{mixer, dna, transport, performance, waveform};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ScrollArea::vertical().show(ui, |ui| {
        render_header(ui, telemetry);
        ui.add_space(15.0);

        // 4-Deck Modular Grid
        ui.columns(2, |cols| {
            render_deck_card(app, &mut cols[0], 0, telemetry);
            render_deck_card(app, &mut cols[1], 1, telemetry);
        });

        ui.add_space(10.0);

        ui.columns(2, |cols| {
            render_deck_card(app, &mut cols[0], 2, telemetry);
            render_deck_card(app, &mut cols[1], 3, telemetry);
        });

        ui.add_space(20.0);
        render_master_section(app, ui, telemetry);
    });
}

fn render_header(ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        Frame::none()
            .fill(Color32::from_rgb(20, 20, 25))
            .rounding(Rounding::same(4.0))
            .inner_margin(Margin::same(8.0))
            .show(ui, |ui| {
                ui.heading(RichText::new("LIVE CONSOLE").strong().color(Color32::WHITE).size(18.0));
                ui.add_space(20.0);
                if let Some(t) = telemetry {
                    ui.label(RichText::new(format!("{:.1}", t.bpm)).monospace().color(Color32::from_rgb(0, 255, 200)).size(16.0));
                    ui.label(RichText::new("BPM").small().color(Color32::GRAY));
                }
            });
    });
}

fn render_deck_card(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let deck_color = InspectorApp::deck_color(i);
    let is_focused = app.focused_deck == i;

    let frame = Frame::group(ui.style())
        .fill(Color32::from_rgb(15, 15, 20))
        .stroke(Stroke::new(1.0, if is_focused { deck_color } else { Color32::from_gray(30) }))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::same(10.0));

    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            render_deck_header(app, ui, i, deck_color, is_focused);
            ui.add_space(8.0);
            waveform::render_deck_waveform_zone(app, ui, i, telemetry, deck_color);
            ui.add_space(12.0);

            ui.horizontal_top(|ui| {
                ui.scope(|ui| {
                    ui.set_min_height(180.0);
                    transport::render_deck_transport(app, ui, i);
                    ui.add_space(10.0);
                    performance::render_deck_performance(app, ui, i);
                    ui.add_space(10.0);
                    dna::render_deck_dna_panel(app, ui, i);
                    ui.add_space(10.0);
                    mixer::render_deck_mixer(app, ui, i, deck_color);
                });
            });
        });
    });
}

fn render_deck_header(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32, is_focused: bool) {
    ui.horizontal(|ui| {
        let deck_id_label = (b'A' + i as u8) as char;
        if ui.selectable_label(is_focused, RichText::new(format!("DECK {}", deck_id_label)).strong().size(14.0).color(deck_color)).clicked() {
            app.focused_deck = i;
        }

        // Master Deck Toggle
        let is_master = app.master_deck == Some(i);
        if ui.selectable_label(is_master, RichText::new("M").strong().color(if is_master { deck_color } else { Color32::GRAY })).clicked() {
             app.master_deck = Some(i);
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMasterDeck(deck_id_label)));
        }

        // Display BPM and Key from metadata if track is loaded
        if let Some(track_id) = app.now_playing[i] {
            if let Ok(Some(track)) = app.library_db.get_track(track_id) {
                ui.add_space(10.0);
                ui.label(RichText::new(format!("{:.1} BPM", track.metadata.bpm)).small().color(Color32::GRAY));
                if let Some(key) = track.metadata.root_key {
                    ui.add_space(5.0);
                    ui.label(RichText::new(format!("Key: {:.0}", key)).small().color(deck_color.gamma_multiply(0.7)));
                }
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.selectable_label(app.channel_sync[i], "SYNC").clicked() {
                app.channel_sync[i] = !app.channel_sync[i];
            }
        });
    });
}



fn render_master_section(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    Frame::group(ui.style())
        .fill(Color32::from_rgb(20, 20, 25))
        .inner_margin(Margin::same(15.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("MASTER").strong());
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        let peak = (app.damped_master_peaks[0] + app.damped_master_peaks[1]) * 0.5;
                        widgets::render_vu_meter(ui, peak, app.master_peak_hold, Color32::WHITE, 140.0);
                        widgets::render_fader(ui, &mut app.master_gain, 0.0..=1.5, Color32::WHITE, 140.0, 20.0);
                    });
                });

                ui.add_space(30.0);

                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width() - 50.0);
                    ui.centered_and_justified(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("CROSSFADER").small().color(Color32::GRAY));
                            if widgets::render_horizontal_fader(ui, &mut app.crossfader_pos, 0.0..=1.0, Color32::WHITE, ui.available_width(), 35.0).changed() {
                                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: 20,
                                    param_id: 0,
                                    value: app.crossfader_pos,
                                    ramp_duration_samples: 0,
                                }));
                            }
                        });
                    });
                });
            });
        });
}


use egui::ScrollArea;
