//! Real-Time Interactive Organism & Mandala Profile Editor UI
//! Enables live parameter editing for 64-D genome weights, field vectors,
//! Padé activation rational coefficients, polar symmetry, petal harmonics,
//! and direct YAML export / preset saving.

use egui::{Color32, Frame, Margin, RichText, ScrollArea, Ui};
use super::organism_profile::OrganismProfile;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct OrganismEditorState {
    pub active_profile: OrganismProfile,
    pub is_open: bool,
    pub status_message: Option<String>,
    pub export_filename: String,
    // Live Mandala parameter overrides
    pub mandala_symmetry: u32,
    pub mandala_layers: u32,
    pub mandala_petal_harmonics: f32,
    pub mandala_rotational_speed: f32,
    pub pade_alpha: f32,
    pub pade_beta: f32,
}

impl Default for OrganismEditorState {
    fn default() -> Self {
        Self {
            active_profile: OrganismProfile::mycelial_bloom(),
            is_open: true,
            status_message: None,
            export_filename: "custom_organism.yaml".to_string(),
            mandala_symmetry: 12,
            mandala_layers: 8,
            mandala_petal_harmonics: 3.0,
            mandala_rotational_speed: 1.0,
            pade_alpha: 1.5,
            pade_beta: 0.5,
        }
    }
}

#[allow(dead_code)]
impl OrganismEditorState {
    pub fn render(&mut self, ui: &mut Ui) {
        Frame::none()
            .fill(Color32::from_rgb(18, 20, 26))
            .inner_margin(Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(
                        RichText::new("ORGANISM & MANDALA PROFILE EDITOR")
                            .color(Color32::from_rgb(0, 220, 255))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("EXPORT YAML").clicked() {
                            self.export_yaml();
                        }
                    });
                });

                if let Some(ref msg) = self.status_message {
                    ui.add_space(4.0);
                    ui.label(RichText::new(msg).color(Color32::from_rgb(100, 255, 150)));
                }

                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    // --- PROFILE IDENTITY ---
                    ui.collapsing(RichText::new("Identity & Family").strong(), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut self.active_profile.name);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Family:");
                            ui.text_edit_singleline(&mut self.active_profile.family);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Description:");
                            ui.text_edit_singleline(&mut self.active_profile.description);
                        });
                    });

                    // --- POLAR MANDALA & PADÉ APPROXIMANT PARAMETERS ---
                    ui.collapsing(RichText::new("Polar Symmetry & Padé SIMD Dynamics").strong().color(Color32::from_rgb(255, 180, 50)), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Mandala Symmetry Order:");
                            ui.add(egui::Slider::new(&mut self.mandala_symmetry, 2..=64));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Concentric Layer Count:");
                            ui.add(egui::Slider::new(&mut self.mandala_layers, 1..=32));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Petal Harmonics Ratio:");
                            ui.add(egui::Slider::new(&mut self.mandala_petal_harmonics, 0.5..=16.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Rotational Speed:");
                            ui.add(egui::Slider::new(&mut self.mandala_rotational_speed, -5.0..=5.0));
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Padé Activation Alpha α:");
                            ui.add(egui::Slider::new(&mut self.pade_alpha, 0.1..=5.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Padé Activation Beta β:");
                            ui.add(egui::Slider::new(&mut self.pade_beta, 0.0..=2.0));
                        });
                    });

                    // --- GENOME PARAMETERS ---
                    ui.collapsing(RichText::new("Genome Vectors (64-D Space)").strong(), |ui| {
                        let g = &mut self.active_profile.genome;
                        ui.horizontal(|ui| {
                            ui.label("Branching:");
                            ui.add(egui::Slider::new(&mut g.branching, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Cohesion:");
                            ui.add(egui::Slider::new(&mut g.cohesion, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Symmetry:");
                            ui.add(egui::Slider::new(&mut g.symmetry, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Mutation Rate:");
                            ui.add(egui::Slider::new(&mut g.mutation, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Persistence:");
                            ui.add(egui::Slider::new(&mut g.persistence, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Turbulence:");
                            ui.add(egui::Slider::new(&mut g.turbulence, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Rigidity:");
                            ui.add(egui::Slider::new(&mut g.rigidity, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Viscosity:");
                            ui.add(egui::Slider::new(&mut g.viscosity, 0.0..=1.0));
                        });
                    });

                    // --- MOTION & FIELD ---
                    ui.collapsing(RichText::new("Motion & Field Vectors").strong(), |ui| {
                        let m = &mut self.active_profile.motion;
                        ui.horizontal(|ui| {
                            ui.label("Inertia:");
                            ui.add(egui::Slider::new(&mut m.inertia, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Turbulence:");
                            ui.add(egui::Slider::new(&mut m.turbulence, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Collective Motion:");
                            ui.add(egui::Slider::new(&mut m.collective_motion, 0.0..=1.0));
                        });

                        let f = &mut self.active_profile.field;
                        ui.horizontal(|ui| {
                            ui.label("Field Type:");
                            ui.text_edit_singleline(&mut f.field_type);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Divergence:");
                            ui.add(egui::Slider::new(&mut f.divergence, 0.0..=1.0));
                        });
                    });

                    // --- RENDER SPEC ---
                    ui.collapsing(RichText::new("Render Specifications").strong(), |ui| {
                        let r = &mut self.active_profile.render;
                        ui.checkbox(&mut r.trails, "Render Trails");
                        ui.checkbox(&mut r.wireframe, "Wireframe Overlay");
                        ui.horizontal(|ui| {
                            ui.label("Feedback Mode:");
                            ui.text_edit_singleline(&mut r.feedback);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Bloom Style:");
                            ui.text_edit_singleline(&mut r.bloom);
                        });
                    });
                });
            });
    }

    fn export_yaml(&mut self) {
        let dir = "assets/organism_profiles";
        if let Err(e) = std::fs::create_dir_all(dir) {
            self.status_message = Some(format!("Directory Error: {}", e));
            return;
        }

        match serde_json::to_string_pretty(&self.active_profile) {
            Ok(json_str) => {
                let filename = format!("{}/{}.yaml", dir, self.active_profile.id);
                if let Err(e) = std::fs::write(&filename, json_str) {
                    self.status_message = Some(format!("Export Error: {}", e));
                } else {
                    self.status_message = Some(format!("Exported preset to {}", filename));
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Serialization Error: {}", e));
            }
        }
    }
}
