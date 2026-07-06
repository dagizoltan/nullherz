use egui::{Ui, Color32, RichText};
use crate::{InspectorApp, widgets};

pub fn render_deck_dna_panel(app: &mut InspectorApp, ui: &mut Ui, i: usize) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("DNA").small().color(Color32::from_gray(100)));
            ui.checkbox(&mut app.personality_macro_mode, "🔗");
        });

        let traits = [
            ("METALLIC", 0, "metallic"),
            ("ORGANIC", 1, "organic"),
            ("WARM", 2, "warm"),
            ("AGGRESSIVE", 3, "aggressive"),
        ];

        let deck_color = InspectorApp::deck_color(i);

        ui.horizontal_wrapped(|ui| {
            for (label, idx, feature) in traits {
                ui.vertical(|ui| {
                    ui.set_max_width(45.0);
                    let val = match idx {
                        0 => &mut app.channel_personality_metallic[i],
                        1 => &mut app.channel_personality_organic[i],
                        2 => &mut app.channel_personality_warm[i],
                        _ => &mut app.channel_personality_aggressive[i],
                    };

                    if widgets::render_knob(ui, val, 0.0..=1.0, "", deck_color).changed() {
                        let strength = *val;
                        emit_personality_mutation(app, i, idx, feature, strength);
                    }
                    ui.label(RichText::new(label).size(7.0).color(Color32::GRAY));
                });
                ui.add_space(2.0);
            }
        });
    });
}

fn emit_personality_mutation(app: &mut InspectorApp, deck_idx: usize, trait_idx: usize, feature: &str, strength: f32) {
    let mut targets = vec![];
    if app.personality_macro_mode {
        for i in 0..4 {
            if let Some(id) = app.now_playing[i] {
                targets.push(id);
                match trait_idx {
                    0 => app.channel_personality_metallic[i] = strength,
                    1 => app.channel_personality_organic[i] = strength,
                    2 => app.channel_personality_warm[i] = strength,
                    _ => app.channel_personality_aggressive[i] = strength,
                }
            }
        }
    } else if let Some(id) = app.now_playing[deck_idx] {
        targets.push(id);
    }

    for track_id in targets {
        let mut name = [0u8; 32];
        let bytes = feature.as_bytes();
        name[..bytes.len()].copy_from_slice(bytes);

        let cmd = nullherz_traits::Command::Resource(nullherz_traits::ResourceCommand::ApplyFeatureMutation {
            target_id: track_id,
            feature_name: name,
            strength,
        });
        let _ = app.command_sender.send(cmd);
    }
}
