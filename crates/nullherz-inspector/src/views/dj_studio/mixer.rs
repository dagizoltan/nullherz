use egui::{Ui, Color32, RichText};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;

/// Dial diameter in the channel strip. Six of these plus a fader and a meter
/// have to coexist in one column, so it is smaller than the 36 px default.
const KNOB: f32 = 30.0;

/// Channel strip controls: a 3x2 rotary grid over a vertical volume fader,
/// with the level meter alongside.
///
/// Six rotaries, every one of them wired to something the engine actually
/// applies:
///
/// | dial | engine target                      |
/// |------|------------------------------------|
/// | HI   | isolator param 2                   |
/// | MID  | isolator param 1                   |
/// | LOW  | isolator param 0                   |
/// | FLT  | filter param 0                     |
/// | BAL  | stereo util param 0 (pan)          |
/// | WID  | stereo util param 1 (width)        |
///
/// BAL and WID were already handled by the mixer orchestrator and simply had
/// no control bound to them.
///
/// There is deliberately NO separate gain dial. A deck has exactly one gain
/// node, which the volume fader drives; a second control writing the same
/// parameter would fight the fader and make both of them lie about the level.
/// A real trim stage needs a second gain node in the deck chain — see the
/// note in the layout review.
pub fn render_deck_mixer(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32, fader_h: f32) {
    let deck_id = (b'A' + i as u8) as char;
    let theme = app.theme;

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space_xs;

        // --- level meter, full strip height, left of the controls -----------
        // ONE meter, not two.
        //
        // This used to draw `render_vu_meter` twice with the SAME peak value,
        // which looks like a stereo pair and is not one — both bars moved
        // identically because both read `damped_peaks[i]`. A meter that implies
        // a measurement it is not making is worse than a narrower honest one.
        // True L/R needs the deck's two output buffer peaks separately.
        ui.vertical(|ui| {
            widgets::render_vu_meter(
                ui,
                app.viz.damped_peaks[i],
                app.mixer.channel_peak_hold[i],
                deck_color,
                fader_h + KNOB * 2.0 + 24.0,
            );
        });

        ui.vertical(|ui| {
            // --- 3 x 2 rotary grid ------------------------------------------
            let mut eq_changed = false;
            let mut settled = false;
            let mut other: Option<(nullherz_traits::DeckParamType, f32)> = None;

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.space_xs;
                for (label, idx) in [("HI", 2usize), ("MID", 1), ("LOW", 0)] {
                    let val = match idx {
                        2 => &mut app.mixer.channel_eq_high[i],
                        1 => &mut app.mixer.channel_eq_mid[i],
                        _ => &mut app.mixer.channel_eq_low[i],
                    };
                    let r = widgets::render_knob_sized(ui, val, 0.0..=2.0, label, deck_color, KNOB);
                    if r.changed() { eq_changed = true; }
                    if r.drag_stopped() || r.lost_focus() { settled = true; }
                }
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.space_xs;

                let r = widgets::render_knob_sized(ui, &mut app.mixer.channel_filter[i], 0.0..=1.0, "FLT", deck_color, KNOB);
                if r.changed() { other = Some((nullherz_traits::DeckParamType::Filter, app.mixer.channel_filter[i])); }
                if r.drag_stopped() || r.lost_focus() { settled = true; }

                let r = widgets::render_knob_sized(ui, &mut app.mixer.channel_balance[i], 0.0..=1.0, "BAL", deck_color, KNOB);
                if r.changed() { other = Some((nullherz_traits::DeckParamType::Pan, app.mixer.channel_balance[i])); }
                if r.drag_stopped() || r.lost_focus() { settled = true; }

                let r = widgets::render_knob_sized(ui, &mut app.mixer.channel_width[i], 0.0..=2.0, "WID", deck_color, KNOB);
                if r.changed() { other = Some((nullherz_traits::DeckParamType::Width, app.mixer.channel_width[i])); }
                if r.drag_stopped() || r.lost_focus() { settled = true; }
            });

            if eq_changed {
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqHigh, app.mixer.channel_eq_high[i]);
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqMid, app.mixer.channel_eq_mid[i]);
                send_deck_param(app, deck_id, nullherz_traits::DeckParamType::EqLow, app.mixer.channel_eq_low[i]);
            }
            if let Some((param_type, val)) = other {
                send_deck_param(app, deck_id, param_type, val);
            }

            ui.add_space(theme.space_xs);

            // --- volume fader ------------------------------------------------
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
