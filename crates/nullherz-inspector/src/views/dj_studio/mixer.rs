use egui::{Ui, Color32, RichText};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;

/// Dial diameter in the channel strip.
const KNOB: f32 = 30.0;

/// Channel strip controls: a 2x2 rotary grid over a vertical volume & pitch fader,
/// with the level meter alongside.
///
/// Standard DJ channel rotaries:
/// - HI: Isolator High EQ
/// - MID: Isolator Mid EQ
/// - LOW: Isolator Low EQ
/// - FLT: Biquad Color Filter (bipolar LPF / HPF)
pub fn render_deck_mixer(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32, fader_h: f32) {
    let deck_id = (b'A' + i as u8) as char;
    let theme = app.theme;

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space_xs;

        // --- Level meter ---
        ui.vertical(|ui| {
            widgets::render_vu_meter(
                ui,
                app.viz.damped_peaks[i],
                app.mixer.channel_peak_hold[i],
                deck_color,
                fader_h + KNOB * 2.0 + 12.0,
            );
        });

        ui.vertical(|ui| {
            // --- 2 x 2 rotary grid (HI, MID / LOW, FLT) --------------------
            let mut eq_changed = false;
            let mut filter_changed = false;
            let mut settled = false;

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.space_xs;

                let r_hi = widgets::render_knob_sized(ui, &mut app.mixer.channel_eq_high[i], 0.0..=2.0, "HI", deck_color, KNOB);
                if r_hi.changed() { eq_changed = true; }
                if r_hi.drag_stopped() || r_hi.lost_focus() { settled = true; }

                let r_mid = widgets::render_knob_sized(ui, &mut app.mixer.channel_eq_mid[i], 0.0..=2.0, "MID", deck_color, KNOB);
                if r_mid.changed() { eq_changed = true; }
                if r_mid.drag_stopped() || r_mid.lost_focus() { settled = true; }
            });

            ui.add_space(2.0);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.space_xs;

                let r_low = widgets::render_knob_sized(ui, &mut app.mixer.channel_eq_low[i], 0.0..=2.0, "LOW", deck_color, KNOB);
                if r_low.changed() { eq_changed = true; }
                if r_low.drag_stopped() || r_low.lost_focus() { settled = true; }

                let r_flt = widgets::render_knob_sized(ui, &mut app.mixer.channel_filter[i], 0.0..=1.0, "FLT", deck_color, KNOB);
                if r_flt.changed() { filter_changed = true; }
                if r_flt.drag_stopped() || r_flt.lost_focus() { settled = true; }
            });

            if eq_changed {
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqHigh, app.mixer.channel_eq_high[i]);
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqMid, app.mixer.channel_eq_mid[i]);
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqLow, app.mixer.channel_eq_low[i]);
            }
            if filter_changed {
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Filter, app.mixer.channel_filter[i]);
            }

            ui.add_space(theme.space_xs);

            // --- Volume and Pitch faders ---
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.space_sm;

                ui.vertical_centered(|ui| {
                    let r_fader = widgets::render_fader(
                        ui,
                        &mut app.mixer.channel_faders[i],
                        0.0..=1.0,
                        deck_color,
                        fader_h,
                        16.0,
                    );
                    if r_fader.changed() {
                        send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Gain, app.mixer.channel_faders[i]);
                    }
                    if r_fader.drag_stopped() || r_fader.lost_focus() { settled = true; }
                    ui.label(
                        RichText::new("VOL")
                            .size(theme.type_caption)
                            .color(theme.text_secondary),
                    );
                });

                ui.vertical_centered(|ui| {
                    let span = app.mixer.pitch_range_pct[i] / 100.0;
                    let r_pitch = widgets::render_fader(
                        ui,
                        &mut app.mixer.channel_pitch[i],
                        (1.0 - span)..=(1.0 + span),
                        deck_color,
                        fader_h,
                        16.0,
                    );
                    if r_pitch.changed() {
                        send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Pitch, app.mixer.channel_pitch[i]);
                    }
                    if r_pitch.drag_stopped() || r_pitch.lost_focus() { settled = true; }

                    if r_pitch.double_clicked() {
                        app.mixer.channel_pitch[i] = 1.0;
                        send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Pitch, 1.0);
                        settled = true;
                    }

                    let pct = (app.mixer.channel_pitch[i] - 1.0) * 100.0;
                    ui.label(
                        RichText::new(format!("{pct:+.1}%"))
                            .size(theme.type_caption)
                            .color(if pct.abs() < 0.05 { theme.text_secondary } else { deck_color })
                            .strong(),
                    );

                    let range_label = format!("±{:.0}", app.mixer.pitch_range_pct[i]);
                    if ui
                        .add(egui::Button::new(
                            RichText::new(range_label)
                                .size(theme.type_caption)
                                .color(theme.text_secondary),
                        ))
                        .on_hover_text("Pitch fader travel. Double-click the fader to return to 0%.")
                        .clicked()
                    {
                        app.mixer.pitch_range_pct[i] = match app.mixer.pitch_range_pct[i] as i32 {
                            8 => 16.0,
                            16 => 50.0,
                            _ => 8.0,
                        };
                        let span = app.mixer.pitch_range_pct[i] / 100.0;
                        app.mixer.channel_pitch[i] = app.mixer.channel_pitch[i].clamp(1.0 - span, 1.0 + span);
                        send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Pitch, app.mixer.channel_pitch[i]);
                    }
                });
            });

            if settled {
                let _ = app.command_sender.send(nullherz_traits::Command::Core(
                    nullherz_traits::CoreCommand::CheckpointParameterEdit,
                ));
            }
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
