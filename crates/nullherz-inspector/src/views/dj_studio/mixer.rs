use egui::{Ui, Color32};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;

pub fn render_deck_mixer(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32) {
    let deck_id = (b'A' + i as u8) as char;
    let theme = app.theme;
    ui.vertical(|ui| {
        // EQ & Filter Row with adjacent sub-columns
        ui.horizontal(|ui| {
            // Column A: HI / MID / LOW knobs stacked
            ui.vertical(|ui| {
                ui.set_max_width(40.0);
                let mut changed = false;
                if widgets::render_knob(ui, &mut app.channel_eq_high[i], 0.0..=2.0, "HI", deck_color).changed() { changed = true; }
                if widgets::render_knob(ui, &mut app.channel_eq_mid[i], 0.0..=2.0, "MID", deck_color).changed() { changed = true; }
                if widgets::render_knob(ui, &mut app.channel_eq_low[i], 0.0..=2.0, "LOW", deck_color).changed() { changed = true; }

                if changed {
                    send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqHigh, app.channel_eq_high[i]);
                    send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqMid, app.channel_eq_mid[i]);
                    send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqLow, app.channel_eq_low[i]);
                }
            });

            ui.add_space(theme.space_sm);

            // Column B: Filter (Vec-driven column for extensibility)
            let mut changed_filter = None;
            ui.vertical(|ui| {
                ui.set_max_width(40.0);
                let filter_params = vec![
                    ("FLT", &mut app.channel_filter[i], nullherz_traits::DeckParamType::Filter, 0.0..=1.0),
                ];
                for (label, val, param_type, range) in filter_params {
                    if widgets::render_knob(ui, val, range, label, deck_color).changed() {
                        changed_filter = Some((param_type, *val));
                    }
                }
            });
            if let Some((param_type, val)) = changed_filter {
                send_deck_param(app, deck_id, param_type, val);
            }
        });

        ui.add_space(theme.space_sm);

        // Volume: Channel fader + Stereo VU meter pair beneath the EQ/Filter row
        ui.horizontal(|ui| {
            let peak = app.damped_peaks[i];
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                widgets::render_vu_meter(ui, peak, app.channel_peak_hold[i], deck_color, 100.0);
                widgets::render_vu_meter(ui, peak, app.channel_peak_hold[i], deck_color, 100.0);
            });
            if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.0, deck_color, 100.0, 18.0).changed() {
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Gain, app.channel_faders[i]);
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
