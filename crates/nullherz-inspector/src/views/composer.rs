use egui::{Ui, ScrollArea, Vec2, Sense, RichText, Stroke, Frame, Rounding, Margin};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;
use nullherz_traits::{Command, PerformanceCommand};
pub use nullherz_conductor::pattern_manager::DnaSequencer;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading(RichText::new("SESSION VIEW (COMPOSER)").strong().color(app.theme.text_primary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             ui.label(egui::RichText::new("QUANTIZED: 1 BAR").color(app.theme.accent).size(app.theme.type_caption));
        });
    });
    ui.add_space(app.theme.space_sm);

    ui.horizontal(|ui| {
        let is_recording = app.record_automation;
        ui.toggle_value(&mut app.record_automation, RichText::new("🔴 RECORD AUTOMATION").color(if is_recording { app.theme.danger } else { app.theme.text_secondary }));
        ui.add_space(app.theme.space_md);

        if ui.button("STOP ALL CLIPS").clicked() {
            for i in 0..16 {
                 let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32 }));
                 app.sequencer_grid[i].fill(0.0);
            }
        }

        ui.add_space(app.theme.space_md);
        ui.label(RichText::new("MASTER VOL").size(app.theme.type_caption).color(app.theme.text_secondary));
        // Relocated Master Volume fader to top control bar as a horizontal control
        widgets::render_horizontal_fader(ui, &mut app.master_gain, 0.0..=1.5, app.theme.text_primary, 100.0, 12.0);
    });
    ui.add_space(app.theme.space_sm);

    // Continuous surface frame wrapping pinned scene-launcher header + scrollable track rows
    Frame::none()
        .fill(app.theme.bg_dark)
        .stroke(app.theme.border_stroke)
        .rounding(Rounding::same(app.theme.radius_md))
        .inner_margin(Margin::same(app.theme.space_sm))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // 1. Pinned Scene Launchers Header Row (stationary at top)
                render_scene_launcher_header(app, ui);
                ui.add_space(app.theme.space_sm);

                // Thin horizontal divider below pinned header
                let (line_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(line_rect, Rounding::ZERO, app.theme.border);
                ui.add_space(app.theme.space_sm);

                // 2. Vertically scrollable track list
                ScrollArea::vertical()
                    .id_source("composer_scroll_v")
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            for track_idx in 0..16 {
                                render_horizontal_track_row(app, ui, track_idx, telemetry);
                                if track_idx < 15 {
                                    ui.add_space(app.theme.space_xs);
                                    // Thin 1px horizontal divider between track rows
                                    let (line_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                                    ui.painter().rect_filled(line_rect, Rounding::ZERO, app.theme.border);
                                    ui.add_space(app.theme.space_xs);
                                }
                            }
                        });
                    });
            });
        });
}

fn render_scene_launcher_header(app: &mut InspectorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // Align precisely with the step columns (skip past the track name column)
        ui.add_space(100.0);
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

        for scene_idx in 0..8 {
            let btn_text = format!("SCENE {}", scene_idx + 1);
            if ui.add_sized([76.0, 20.0], egui::Button::new(RichText::new(btn_text).size(app.theme.type_caption).strong()).fill(app.theme.accent.linear_multiply(0.12))).clicked() {
                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::LaunchClip { row: 0xFF, col: scene_idx as u32 }));
            }
        }
    });
}

fn render_horizontal_track_row(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    // Utilize per-track color identity from theme
    let track_color = app.theme.track_colors[i];
    let is_muted = app.track_mutes[i];
    let is_soloed = app.track_solos[i];

    ui.horizontal(|ui| {
        // 1. Track Name & Swatch/Chip Column (fixed-width 100.0 via outer frame)
        let header_bg = if is_muted {
            app.theme.bg_inset
        } else {
            track_color.gamma_multiply(0.25)
        };
        Frame::none()
            .fill(header_bg)
            .rounding(Rounding::same(app.theme.radius_sm))
            .inner_margin(Margin::symmetric(app.theme.space_xs, app.theme.space_xs))
            .show(ui, |ui| {
                ui.set_width(90.0); // 90px plus inner margin (~100px total)
                ui.horizontal(|ui| {
                    ui.add_space(app.theme.space_xs);
                    // 8x8px color swatch/chip
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                    ui.painter().rect_filled(rect, Rounding::same(1.5), track_color);
                    ui.add_space(app.theme.space_xs);
                    ui.label(RichText::new(format!("TRK {}", i + 1)).strong().size(app.theme.type_body).color(app.theme.text_primary));
                });
            });

        ui.add_space(4.0);

        // 2. Middle: 8 step-cells laid out left-to-right
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

            for slot_idx in 0..8 {
                let (rect, response) = ui.allocate_exact_size(Vec2::new(76.0, 24.0), Sense::click());

                // Check playback status of this step/clip from telemetry
                let mut is_playing = false;
                let mut is_starting = false;
                if let Some(t) = telemetry {
                    is_playing = t.active_clips[i] == slot_idx as u8;
                    is_starting = (t.starting_clips_mask[i] >> slot_idx) & 1 == 1;
                }

                let velocity = app.sequencer_grid[i][slot_idx];
                let mut color = if is_playing {
                    app.theme.success // Vibrant play green (semantic override)
                } else if is_starting {
                    app.theme.warning // Quantizing warning yellow (semantic override)
                } else if velocity > 0.0 {
                    track_color.gamma_multiply(velocity.clamp(0.5, 1.0)) // Track-specific active slot hue
                } else {
                    // Subtle track-specific passive tint for idle empty slots
                    track_color.gamma_multiply(0.04)
                };

                if slot_idx == app.sequencer_active_step {
                    color = color.linear_multiply(1.4); // Highlight current playback beat
                }

                // Render clip capsule
                ui.painter().rect_filled(rect, app.theme.radius_sm, color);
                ui.painter().rect_stroke(rect, app.theme.radius_sm, app.theme.border_stroke);

                if is_playing {
                    // Play triangle indicator
                    let tri_p1 = rect.left_center() + Vec2::new(6.0, -5.0);
                    let tri_p2 = rect.left_center() + Vec2::new(6.0, 5.0);
                    let tri_p3 = rect.left_center() + Vec2::new(12.0, 0.0);
                    ui.painter().add(egui::Shape::convex_polygon(vec![tri_p1, tri_p2, tri_p3], app.theme.text_primary, Stroke::NONE));
                }

                if response.hovered() {
                    ui.painter().rect_stroke(rect, app.theme.radius_sm, Stroke::new(1.0, app.theme.text_primary));
                }

                if response.clicked() {
                    let is_on = app.sequencer_grid[i][slot_idx] == 0.0;
                    let val = if is_on { 1.0 } else { 0.0 };
                    app.sequencer_grid[i][slot_idx] = val;
                    let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetSequencerStep {
                        node_idx: app.get_node_id("sequencer_node"),
                        track: i as u32,
                        step: slot_idx as u32,
                        value: val,
                    }));
                }
            }
        });

        ui.add_space(4.0);

        // 3. Compact row controls
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

            // Stop Clip button (compact)
            let stop_btn = egui::Button::new(RichText::new("■").size(app.theme.type_caption).strong()).fill(app.theme.bg_inset);
            if ui.add_sized([18.0, 18.0], stop_btn).on_hover_text("Stop clip").clicked() {
                app.sequencer_grid[i].fill(0.0);
                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32 }));
            }

            // Activator ON/OFF Mute
            let activator_color = if !is_muted { app.theme.warning } else { app.theme.bg_inset };
            if ui.add_sized([22.0, 18.0], egui::Button::new(RichText::new("ON").size(app.theme.type_caption).strong()).fill(activator_color)).clicked() {
                app.track_mutes[i] = !is_muted;
                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackMute { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32, muted: app.track_mutes[i] }));
            }

            // Solo Button
            let solo_color = if is_soloed { app.theme.track_colors[1] } else { app.theme.bg_inset };
            if ui.add_sized([18.0, 18.0], egui::Button::new(RichText::new("S").size(app.theme.type_caption).strong()).fill(solo_color)).clicked() {
                app.track_solos[i] = !is_soloed;
                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackSolo { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32, soloed: app.track_solos[i] }));
            }

            ui.add_space(4.0);

            // Volume horizontal fader
            let mock_volume_color = if is_muted { app.theme.bg_inset } else { track_color };
            widgets::render_horizontal_fader(ui, &mut app.track_volumes[i], 0.0..=1.0, mock_volume_color, 60.0, 10.0)
                .on_hover_text("VOLUME");

            ui.add_space(4.0);

            // Evolve horizontal fader
            let mut val = app.evolution_strengths[i];
            if widgets::render_horizontal_fader(ui, &mut val, 0.0..=1.0, app.theme.accent, 60.0, 10.0)
                .on_hover_text("GENE EVOLVE")
                .changed()
            {
                app.evolution_strengths[i] = val;
                let _ = app.command_sender.send(Command::Performance(PerformanceCommand::EvolvePattern {
                    node_idx: i as u32,
                    track_idx: 0,
                    mutation_strength: val,
                }));
            }
        });
    });
}
