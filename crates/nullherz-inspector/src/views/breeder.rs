use egui::{Ui, Vec2, Color32, Stroke, Sense, RichText};
use nullherz_traits::{Command, DnaCommand};

pub struct BreederView {
    pub parent_a_id: Option<u64>,
    pub parent_b_id: Option<u64>,
    pub transfusion_bias_x: f32, // Spectral Bias
    pub transfusion_bias_y: f32, // Rhythmic Bias
    pub target_node_idx: u32,
}

impl BreederView {
    pub fn new() -> Self {
        Self {
            parent_a_id: None,
            parent_b_id: None,
            transfusion_bias_x: 0.5,
            transfusion_bias_y: 0.5,
            target_node_idx: 150, // PersonalityInheritanceProcessor default ID
        }
    }

    pub fn show(ui: &mut Ui, state: &mut BreederView, _app_telemetry: &Option<audio_core::Telemetry>, app: &mut crate::InspectorApp) {
        ui.heading("DNA Breeder");
        ui.separator();

        ui.horizontal(|ui| {
            // Parent A Selection
            ui.vertical(|ui| {
                ui.label("Parent A");
                let label = state.parent_a_id.and_then(|id| app.library_db.get_track(id).ok().flatten())
                    .map(|t| t.title).unwrap_or_else(|| "Select Sample".to_string());

                if ui.button(label).clicked() {
                    // In a real app, this would open a modal.
                    // For now, we use the library sidebar selection.
                }
            });

            ui.add_space(40.0);
            ui.label(RichText::new("X").size(20.0).strong());
            ui.add_space(40.0);

            // Parent B Selection
            ui.vertical(|ui| {
                ui.label("Parent B");
                let label = state.parent_b_id.and_then(|id| app.library_db.get_track(id).ok().flatten())
                    .map(|t| t.title).unwrap_or_else(|| "Select Sample".to_string());

                if ui.button(label).clicked() { }
            });
        });

        ui.add_space(30.0);

        ui.horizontal(|ui| {
            // 2D Transfusion Pad
            ui.vertical(|ui| {
                ui.label("Transfusion Pad (X: Spectral, Y: Rhythmic)");
                let (rect, response) = ui.allocate_at_least(Vec2::splat(250.0), Sense::drag());

                ui.painter().rect_filled(rect, 4.0, Color32::from_black_alpha(150));
                ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_rgb(0, 255, 200)));

                // Grid lines
                ui.painter().line_segment([rect.center_top(), rect.center_bottom()], Stroke::new(0.5, Color32::DARK_GRAY));
                ui.painter().line_segment([rect.left_center(), rect.right_center()], Stroke::new(0.5, Color32::DARK_GRAY));

                if response.dragged() {
                    let pos = response.interact_pointer_pos().unwrap();
                    state.transfusion_bias_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    state.transfusion_bias_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);

                    state.emit_dna_command(app);
                }

                let handle_pos = rect.left_top() + Vec2::new(state.transfusion_bias_x * rect.width(), (1.0 - state.transfusion_bias_y) * rect.height());
                ui.painter().circle_filled(handle_pos, 8.0, Color32::from_rgb(0, 255, 200));
                ui.painter().circle_stroke(handle_pos, 8.0, Stroke::new(2.0, Color32::WHITE));
            });

            ui.add_space(20.0);

            // Visualizers (Mockup - in real app, these use app.last_telemetry)
            ui.vertical(|ui| {
                ui.label("Real-time Evolution Monitor");
                ui.group(|ui| {
                    ui.allocate_at_least(Vec2::new(200.0, 100.0), Sense::hover());
                    ui.painter().text(ui.min_rect().center(), egui::Align2::CENTER_CENTER, "SPECTRUM", egui::FontId::proportional(12.0), Color32::GRAY);
                });
                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.allocate_at_least(Vec2::new(200.0, 100.0), Sense::hover());
                    ui.painter().text(ui.min_rect().center(), egui::Align2::CENTER_CENTER, "GONIOMETER", egui::FontId::proportional(12.0), Color32::GRAY);
                });
            });
        });

        ui.add_space(20.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Spectral Bias: {:.2}", state.transfusion_bias_x));
                ui.add_space(20.0);
                ui.label(format!("Rhythmic Bias: {:.2}", state.transfusion_bias_y));
            });
        });

        ui.add_space(20.0);
        if ui.button(RichText::new("🧬 EVOLVE PERMANENTLY").strong().size(16.0)).clicked() {
            // Commit the child DNA to the registry
        }
    }

    fn emit_dna_command(&self, app: &crate::InspectorApp) {
        if let (Some(id_a), Some(id_b)) = (self.parent_a_id, self.parent_b_id) {
            if let (Ok(Some(track_a)), Ok(Some(track_b))) = (app.library_db.get_track(id_a), app.library_db.get_track(id_b)) {

                // 1. Spectral Transfusion
                let mut payload = [0u8; 128];
                let mut energy_map = [0u8; 64];
                nullherz_dna::interpolate_energy_map(&mut energy_map, &track_a.metadata.dna.spectral.energy_map, &track_b.metadata.dna.spectral.energy_map, self.transfusion_bias_x);
                payload[0..64].copy_from_slice(&energy_map);

                // 2. Rhythmic Transfusion (Micro-timing)
                for i in 0..12 {
                    let val_a = track_a.metadata.dna.rhythmic.micro_timing[i] as f32;
                    let val_b = track_b.metadata.dna.rhythmic.micro_timing[i] as f32;
                    payload[64 + i] = (val_a * (1.0 - self.transfusion_bias_y) + val_b * self.transfusion_bias_y) as i8 as u8;
                }

                // 3. Rhythmic Transfusion (Onset Mask)
                for i in 0..4 {
                    let mask_a = track_a.metadata.dna.rhythmic.onset_mask[i];
                    let mask_b = track_b.metadata.dna.rhythmic.onset_mask[i];
                    // Pack into payload (bytes 76-107)
                    let res_mask = if self.transfusion_bias_y > 0.5 { mask_b } else { mask_a };
                    for j in 0..8 {
                        payload[76 + i * 8 + j] = ((res_mask >> (j * 8)) & 0xFF) as u8;
                    }
                }

                let cmd = Command::Dna(DnaCommand {
                    target_id: self.target_node_idx as u64,
                    layer_mask: 3, // Spectral + Rhythmic
                    bias: 1.0, // We already interpolated in the payload
                    payload,
                });

                let _ = app.command_sender.send(cmd);
            }
        }
    }
}
