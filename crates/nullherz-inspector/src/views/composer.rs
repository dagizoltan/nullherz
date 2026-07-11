use egui::{Ui, ScrollArea, Color32, Vec2, Sense, RichText, Stroke, Frame, Rounding, Margin};
use crate::{InspectorApp, widgets};
use audio_core::Telemetry;
use nullherz_traits::{Command, PerformanceCommand};
pub use nullherz_conductor::pattern_manager::DnaSequencer;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    ui.horizontal(|ui| {
        ui.heading(RichText::new("SESSION VIEW (COMPOSER)").strong().color(theme.text_primary).size(theme.type_heading));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             ui.label(egui::RichText::new("QUANTIZED: 1 BAR").color(theme.accent).size(theme.type_caption));
        });
    });
    ui.add_space(theme.space_md);

    ui.horizontal(|ui| {
        let is_recording = app.record_automation;
        ui.toggle_value(&mut app.record_automation, RichText::new("🔴 RECORD AUTOMATION").color(if is_recording { theme.danger } else { theme.text_secondary }).size(theme.type_label));
        ui.add_space(theme.space_lg);
        if ui.button(RichText::new("STOP ALL CLIPS").size(theme.type_label)).clicked() {
            for i in 0..16 {
                 let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32 }));
                 app.sequencer_grid[i].fill(0.0);
            }
        }
    });
    ui.add_space(theme.space_sm);

    // Horizontal scroll area containing the Ableton-style vertical channel strips
    ScrollArea::horizontal().id_source("composer_scroll_h").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            // Render 16 vertical track strips
            for track_idx in 0..16 {
                render_vertical_track_strip(app, ui, track_idx, telemetry);
                ui.add_space(theme.space_xs); // Spacing between channels
            }

            // Vertical divider before the Master section
            let (line_rect, _) = ui.allocate_exact_size(Vec2::new(2.0, 480.0), Sense::hover());
            ui.painter().rect_filled(line_rect, Rounding::ZERO, theme.border);
            ui.add_space(theme.space_md);

            // Render Master section with scene launchers stacked vertically on the far right
            render_master_scene_strip(app, ui, telemetry);
        });
    });
}

fn render_vertical_track_strip(app: &mut InspectorApp, ui: &mut Ui, i: usize, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let track_color = theme.accent;
    let is_muted = app.track_mutes[i];
    let is_soloed = app.track_solos[i];

    Frame::none()
        .fill(theme.bg_canvas)
        .stroke(Stroke::new(1.0, theme.border))
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_sm))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_width(90.0);

                // 1. Track Title Bar
                let header_bg = if is_muted {
                    theme.bg_surface
                } else {
                    track_color.gamma_multiply(0.2)
                };
                Frame::none()
                    .fill(header_bg)
                    .rounding(Rounding::same(theme.radius_sm))
                    .inner_margin(Margin::symmetric(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.set_width(80.0);
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new(format!("TRK {}", i + 1)).strong().size(theme.type_caption).color(theme.text_primary));
                        });
                    });

                ui.add_space(theme.space_sm);

                // 2. Vertically Stacked Clip Slots (8 slots visible)
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
                        theme.success
                    } else if is_starting {
                        theme.warning
                    } else if velocity > 0.0 {
                        theme.accent_muted.gamma_multiply(velocity.clamp(0.4, 1.0))
                    } else {
                        theme.bg_inset
                    };

                    if slot_idx == app.sequencer_active_step {
                        color = color.linear_multiply(1.4); // Highlight current playback beat
                    }

                    // Render clip capsule
                    ui.painter().rect_filled(rect, theme.radius_sm, color);
                    ui.painter().rect_stroke(rect, theme.radius_sm, Stroke::new(1.0, theme.border));

                    if is_playing {
                        // Play triangle indicator
                        let tri_p1 = rect.left_center() + Vec2::new(6.0, -5.0);
                        let tri_p2 = rect.left_center() + Vec2::new(6.0, 5.0);
                        let tri_p3 = rect.left_center() + Vec2::new(12.0, 0.0);
                        ui.painter().add(egui::Shape::convex_polygon(vec![tri_p1, tri_p2, tri_p3], theme.text_primary, Stroke::NONE));
                    }

                    if response.hovered() {
                        ui.painter().rect_stroke(rect, theme.radius_sm, Stroke::new(1.0, theme.text_primary));
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
                    ui.add_space(theme.space_xs);
                }

                ui.add_space(theme.space_sm);
                ui.separator();
                ui.add_space(theme.space_sm);

                // 3. Stop Clip button
                if ui.add_sized([76.0, 18.0], egui::Button::new(RichText::new("■ Stop").size(theme.type_caption)).fill(theme.bg_surface)).clicked() {
                    app.sequencer_grid[i].fill(0.0);
                    let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32 }));
                }

                ui.add_space(theme.space_sm);

                // 4. Send & Evolution Control
                ui.label(RichText::new("GENE EVOLVE").size(theme.type_caption).color(theme.text_secondary));
                if ui.add(egui::Slider::new(&mut app.evolution_strengths[i], 0.0..=1.0).show_value(false)).changed() {
                    let _ = app.command_sender.send(Command::Performance(PerformanceCommand::EvolvePattern {
                        node_idx: i as u32,
                        track_idx: 0,
                        mutation_strength: app.evolution_strengths[i],
                    }));
                }

                ui.add_space(theme.space_sm);

                // 5. Track Activator (On/Off Mute) & Solo
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    // Yellow when ON (unmuted), Dark when OFF (muted)
                    let activator_color = if !is_muted { theme.warning } else { theme.text_disabled };
                    if ui.add_sized([34.0, 18.0], egui::Button::new(RichText::new("ON").size(theme.type_caption).strong()).fill(activator_color)).clicked() {
                        app.track_mutes[i] = !is_muted;
                        let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackMute { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32, muted: app.track_mutes[i] }));
                    }

                    let solo_color = if is_soloed { theme.accent } else { theme.text_disabled };
                    if ui.add_sized([34.0, 18.0], egui::Button::new(RichText::new("S").size(theme.type_caption).strong()).fill(solo_color)).clicked() {
                        app.track_solos[i] = !is_soloed;
                        let _ = app.command_sender.send(Command::Performance(PerformanceCommand::SetTrackSolo { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32, soloed: app.track_solos[i] }));
                    }
                });

                ui.add_space(theme.space_sm);

                // 6. Track Volume Fader
                let mock_volume_color = if is_muted { theme.text_disabled } else { track_color };
                // Hardened: Decoupled track volume from sequencer_grid step 0 to prevent pattern corruption!
                widgets::render_fader(ui, &mut app.track_volumes[i], 0.0..=1.0, mock_volume_color, 70.0, 14.0);
            });
        });
}

fn render_master_scene_strip(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    Frame::none()
        .fill(theme.bg_canvas)
        .stroke(Stroke::new(1.0, theme.border))
        .rounding(Rounding::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_sm))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_width(90.0);

                // 1. Master Header
                Frame::none()
                    .fill(theme.bg_surface_raised)
                    .rounding(Rounding::same(theme.radius_sm))
                    .inner_margin(Margin::symmetric(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.set_width(80.0);
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new("MASTER").strong().size(theme.type_caption).color(theme.text_primary));
                        });
                    });

                ui.add_space(theme.space_sm);

                // 2. Vertically Stacked Scene Launchers (aligned with track slots!)
                for scene_idx in 0..8 {
                    let btn_text = format!("Scene {}", scene_idx + 1);
                    if ui.add_sized([76.0, 24.0], egui::Button::new(RichText::new(btn_text).size(theme.type_caption).strong()).fill(theme.bg_surface_raised)).clicked() {
                        let _ = app.command_sender.send(Command::Performance(PerformanceCommand::LaunchClip { row: 0xFF, col: scene_idx as u32 }));
                    }
                    ui.add_space(theme.space_xs);
                }

                ui.add_space(theme.space_sm);
                ui.separator();
                ui.add_space(theme.space_sm);

                // 3. Stop All Clips button
                if ui.add_sized([76.0, 18.0], egui::Button::new(RichText::new("■ Stop All").size(theme.type_caption)).fill(theme.danger.linear_multiply(0.2))).clicked() {
                    for i in 0..16 {
                        app.sequencer_grid[i].fill(0.0);
                        let _ = app.command_sender.send(Command::Performance(PerformanceCommand::ClearTrackPattern { node_idx: app.get_node_id("sequencer_node"), track_idx: i as u32 }));
                    }
                }

                ui.add_space(theme.space_md);
                ui.label(RichText::new("MST VOL").size(theme.type_caption).color(theme.text_secondary));

                // 4. Master Volume Fader
                widgets::render_fader(ui, &mut app.master_gain, 0.0..=1.5, theme.text_primary, 120.0, 16.0);
            });
        });
}
