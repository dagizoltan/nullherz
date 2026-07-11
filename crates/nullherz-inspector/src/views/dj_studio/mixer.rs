use egui::{Ui, Color32};
use crate::{InspectorApp, widgets};

pub fn render_deck_mixer(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32) {
    let deck_id = (b'A' + i as u8) as char;
    ui.vertical(|ui| {
        // EQ & Filter Row with adjacent sub-columns
        ui.horizontal(|ui| {
            // Column A: HI / MID / LOW knobs stacked with adjacent touch KILL buttons
            ui.vertical(|ui| {
                ui.set_max_width(64.0);
                let mut changed = false;

                ui.horizontal(|ui| {
                    if widgets::render_knob(ui, &mut app.channel_eq_high[i], 0.0..=2.0, "HI", deck_color).changed() { changed = true; }
                    let is_killed = app.channel_eq_high[i] == 0.0;
                    let k_color = if is_killed { Color32::RED } else { Color32::from_gray(50) };
                    let btn = egui::Button::new(egui::RichText::new("K").size(9.0).strong()).fill(k_color);
                    if ui.add_sized([20.0, 16.0], btn).clicked() {
                        app.channel_eq_high[i] = if is_killed { 1.0 } else { 0.0 };
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    if widgets::render_knob(ui, &mut app.channel_eq_mid[i], 0.0..=2.0, "MID", deck_color).changed() { changed = true; }
                    let is_killed = app.channel_eq_mid[i] == 0.0;
                    let k_color = if is_killed { Color32::RED } else { Color32::from_gray(50) };
                    let btn = egui::Button::new(egui::RichText::new("K").size(9.0).strong()).fill(k_color);
                    if ui.add_sized([20.0, 16.0], btn).clicked() {
                        app.channel_eq_mid[i] = if is_killed { 1.0 } else { 0.0 };
                        changed = true;
                    }
                });
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    if widgets::render_knob(ui, &mut app.channel_eq_low[i], 0.0..=2.0, "LOW", deck_color).changed() { changed = true; }
                    let is_killed = app.channel_eq_low[i] == 0.0;
                    let k_color = if is_killed { Color32::RED } else { Color32::from_gray(50) };
                    let btn = egui::Button::new(egui::RichText::new("K").size(9.0).strong()).fill(k_color);
                    if ui.add_sized([20.0, 16.0], btn).clicked() {
                        app.channel_eq_low[i] = if is_killed { 1.0 } else { 0.0 };
                        changed = true;
                    }
                });

                if changed {
                    send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqHigh, app.channel_eq_high[i]);
                    send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqMid, app.channel_eq_mid[i]);
                    send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqLow, app.channel_eq_low[i]);
                }
            });

            ui.add_space(4.0);

            // Column B: Filter (Bi-polar styling)
            let mut changed_filter = None;
            ui.vertical(|ui| {
                ui.set_max_width(44.0);
                // Style filter based on High-Pass (>0.5 cyan) or Low-Pass (<0.5 pinkish-red)
                let val = app.channel_filter[i];
                let filter_color = if val > 0.52 {
                    Color32::from_rgb(0, 255, 255) // Cyan HPF
                } else if val < 0.48 {
                    Color32::from_rgb(255, 50, 100) // Pinkish-red LPF
                } else {
                    deck_color
                };

                if widgets::render_knob(ui, &mut app.channel_filter[i], 0.0..=1.0, "FLT", filter_color).changed() {
                    changed_filter = Some((nullherz_traits::DeckParamType::Filter, app.channel_filter[i]));
                }
            });
            if let Some((param_type, val)) = changed_filter {
                send_deck_param(app, deck_id, param_type, val);
            }
        });

        ui.add_space(8.0);

        // Volume: Channel fader + VU meter pair beneath the EQ/Filter row (Parallel, side-by-side)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let peak = app.damped_peaks[i];
            widgets::render_vu_meter(ui, peak, app.channel_peak_hold[i], deck_color, 100.0);
            if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.0, deck_color, 100.0, 18.0).changed() {
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Gain, app.channel_faders[i]);
            }
        });

        // Crossfader Assign Switch (A / THRU / B)
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.set_max_width(100.0);
            let mut assign_val = if i < 2 { 0 } else { 1 }; // Simple mock state for visual completeness
            ui.selectable_value(&mut assign_val, 0, egui::RichText::new("A").size(8.5).strong());
            ui.selectable_value(&mut assign_val, 2, egui::RichText::new("THRU").size(8.5).strong());
            ui.selectable_value(&mut assign_val, 1, egui::RichText::new("B").size(8.5).strong());
        });
    });
}

fn send_deck_param(app: &InspectorApp, deck_id: char, param_type: nullherz_traits::DeckParamType, value: f32) {
    let clamped_value = value.clamp(0.0, 2.0);
    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetDeckParam {
        deck_id,
        param_type,
        value: clamped_value,
    }));
}
