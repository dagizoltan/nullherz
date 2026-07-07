use egui::{Ui, Color32};
use crate::{InspectorApp, widgets};

pub fn render_deck_mixer(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32) {
    ui.horizontal(|ui| {
        ui.set_min_width(140.0);
        let deck_id = (b'A' + i as u8) as char;

        // EQ Stack
        ui.vertical(|ui| {
            ui.set_max_width(40.0);
            let mut changed = false;
            if widgets::render_knob(ui, &mut app.channel_eq_high[i], 0.0..=2.0, "HI", deck_color).changed() { changed = true; }
            if widgets::render_knob(ui, &mut app.channel_eq_mid[i], 0.0..=2.0, "MID", deck_color).changed() { changed = true; }
            if widgets::render_knob(ui, &mut app.channel_eq_low[i], 0.0..=2.0, "LOW", deck_color).changed() { changed = true; }

            ui.add_space(8.0);
            if widgets::render_knob(ui, &mut app.channel_filter[i], 0.0..=1.0, "FLT", deck_color).changed() {
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::Filter, app.channel_filter[i]);
            }

            if changed {
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqHigh, app.channel_eq_high[i]);
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqMid, app.channel_eq_mid[i]);
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqLow, app.channel_eq_low[i]);
            }
        });

        ui.add_space(8.0);

        // Fader & Meter
        ui.horizontal(|ui| {
            let peak = app.damped_peaks[i];
            widgets::render_vu_meter(ui, peak, app.channel_peak_hold[i], deck_color, 160.0);
            if widgets::render_fader(ui, &mut app.channel_faders[i], 0.0..=1.0, deck_color, 160.0, 22.0).changed() {
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
