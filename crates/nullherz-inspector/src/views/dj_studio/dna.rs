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

        egui::Grid::new(format!("dna_grid_{}", i)).num_columns(2).spacing([4.0, 4.0]).show(ui, |ui| {
            for (label, idx, feature) in traits {
                ui.vertical(|ui| {
                    ui.set_max_width(32.0);
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
                    ui.label(RichText::new(label).size(6.0).color(Color32::from_gray(100)));
                });
                if idx % 2 == 1 { ui.end_row(); }
            }
        });

        // Interactive 2D Vector Coordinate Pad representing DNA blend
        ui.add_space(6.0);
        ui.label(RichText::new("GENETIC COORDINATES").size(7.5).color(Color32::from_gray(120)));
        let pad_size = 54.0;
        let (rect, response) = ui.allocate_exact_size(egui::Vec2::new(pad_size, pad_size), egui::Sense::drag());

        // Draw vector pad background
        ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(15, 15, 18));
        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_gray(40)));

        // Grid axis lines
        ui.painter().line_segment(
            [egui::pos2(rect.center().x, rect.min.y), egui::pos2(rect.center().x, rect.max.y)],
            egui::Stroke::new(0.5, Color32::from_gray(30))
        );
        ui.painter().line_segment(
            [egui::pos2(rect.min.x, rect.center().y), egui::pos2(rect.max.x, rect.center().y)],
            egui::Stroke::new(0.5, Color32::from_gray(30))
        );

        // Map metallic vs organic on X-axis, warm vs aggressive on Y-axis
        let mut x = (app.channel_personality_metallic[i] - app.channel_personality_organic[i] + 1.0) * 0.5;
        let mut y = (app.channel_personality_warm[i] - app.channel_personality_aggressive[i] + 1.0) * 0.5;

        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                x = ((pos.x - rect.min.x) / pad_size).clamp(0.0, 1.0);
                y = ((pos.y - rect.min.y) / pad_size).clamp(0.0, 1.0);

                // Derive individual trait strengths from 2D coordinates
                app.channel_personality_metallic[i] = (x * 2.0 - 1.0).max(0.0);
                app.channel_personality_organic[i] = ((1.0 - x) * 2.0 - 1.0).max(0.0);
                app.channel_personality_warm[i] = (y * 2.0 - 1.0).max(0.0);
                app.channel_personality_aggressive[i] = ((1.0 - y) * 2.0 - 1.0).max(0.0);

                emit_personality_mutation(app, i, 0, "metallic", app.channel_personality_metallic[i]);
                emit_personality_mutation(app, i, 1, "organic", app.channel_personality_organic[i]);
                emit_personality_mutation(app, i, 2, "warm", app.channel_personality_warm[i]);
                emit_personality_mutation(app, i, 3, "aggressive", app.channel_personality_aggressive[i]);
            }
        }

        // Target locator dot
        let dot_pos = egui::pos2(
            rect.min.x + x * pad_size,
            rect.min.y + y * pad_size
        );
        ui.painter().circle_filled(dot_pos, 3.0, deck_color);
        ui.painter().circle_stroke(dot_pos, 5.0, egui::Stroke::new(1.0, deck_color.linear_multiply(0.4)));
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
