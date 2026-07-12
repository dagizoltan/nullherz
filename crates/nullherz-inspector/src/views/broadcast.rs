use egui::{Ui, Frame, RichText, Color32, Vec2, Sense};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Global Broadcast Console");
    ui.add_space(10.0);

    // Explicit Documenting Note on Real-Time Streaming Telemetry
    ui.horizontal(|ui| {
        ui.label(RichText::new("ℹ NOTE: Streaming Telemetry is currently running in Mock/Simulated mode. Actual RT live-stream state/telemetry tracking is not yet fully exposed by the underlying streaming_manager.rs backend.").size(9.0).color(app.theme.text_secondary));
    });
    ui.add_space(15.0);

    ui.columns(2, |cols| {
        // Left Column: Stream Configuration & Encoder
        let ui = &mut cols[0];
        ui.vertical(|ui| {
            ui.strong("Pre-flight Config");
            ui.add_space(4.0);
            Frame::none()
                .fill(app.theme.bg_surface)
                .rounding(app.theme.radius_md)
                .stroke(app.theme.border_stroke)
                .inner_margin(app.theme.space_md)
                .show(ui, |ui| {
                    ui.label("Stream Server URL");
                    ui.text_edit_singleline(&mut app.broadcast_url);
                    ui.add_space(8.0);

                    ui.label("Stream Key / Secret");
                    ui.horizontal(|ui| {
                        let text_edit = if app.broadcast_reveal_key {
                            egui::TextEdit::singleline(&mut app.broadcast_key)
                        } else {
                            egui::TextEdit::singleline(&mut app.broadcast_key).password(true)
                        };
                        ui.add_sized([ui.available_width() - 32.0, 18.0], text_edit);
                        if ui.button(if app.broadcast_reveal_key { "👁" } else { "🙈" }).clicked() {
                            app.broadcast_reveal_key = !app.broadcast_reveal_key;
                        }
                    });
                });

            ui.add_space(15.0);

            ui.strong("Encoder Settings");
            ui.add_space(4.0);
            Frame::none()
                .fill(app.theme.bg_surface)
                .rounding(app.theme.radius_md)
                .stroke(app.theme.border_stroke)
                .inner_margin(app.theme.space_md)
                .show(ui, |ui| {
                    ui.label("Codec Selection");
                    egui::ComboBox::from_id_source("broadcast_codec_select")
                        .selected_text(match app.broadcast_codec {
                            0 => "Opus (Mastering Grade)",
                            1 => "AAC-LC (Standard Mobile)",
                            _ => "FLAC (Lossless Studio)",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.broadcast_codec, 0, "Opus (Mastering Grade)");
                            ui.selectable_value(&mut app.broadcast_codec, 1, "AAC-LC (Standard Mobile)");
                            ui.selectable_value(&mut app.broadcast_codec, 2, "FLAC (Lossless Studio)");
                        });
                    ui.add_space(8.0);

                    ui.label(format!("Bitrate limit: {:.0} kbps", app.broadcast_bitrate));
                    ui.add(egui::Slider::new(&mut app.broadcast_bitrate, 64.0..=512.0).show_value(false));
                });
        });

        // Right Column: State Machine & Live Health Telemetry
        let ui = &mut cols[1];
        ui.vertical(|ui| {
            ui.strong("Broadcasting Operations");
            ui.add_space(4.0);
            Frame::none()
                .fill(app.theme.bg_surface)
                .rounding(app.theme.radius_md)
                .stroke(app.theme.border_stroke)
                .inner_margin(app.theme.space_md)
                .show(ui, |ui| {
                    // Pre-flight State Machine Selection
                    ui.label("Simulate State Machine:");
                    let state_options = [
                        (0, "Offline"),
                        (1, "Connecting"),
                        (2, "Live"),
                        (3, "Error"),
                    ];
                    nullherz_ui_hal::widgets::render_segmented_control(
                        ui,
                        &app.theme,
                        &mut app.broadcast_state,
                        &state_options,
                    );
                    ui.add_space(12.0);

                    // Connection Dot / Banner based on current state
                    let (status_text, dot_color, bg_banner) = match app.broadcast_state {
                        1 => ("CONNECTING...", app.theme.warning, app.theme.warning.linear_multiply(0.12)),
                        2 => ("LIVE & STREAMING", app.theme.success, app.theme.success.linear_multiply(0.12)),
                        3 => ("ERROR", app.theme.danger, app.theme.danger.linear_multiply(0.12)),
                        _ => ("OFFLINE", app.theme.text_secondary, app.theme.bg_inset),
                    };

                    Frame::none()
                        .fill(bg_banner)
                        .rounding(app.theme.radius_md)
                        .stroke(app.theme.border_stroke)
                        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                ui.painter().circle_filled(dot_rect.center(), 5.5, dot_color);
                                ui.add_space(6.0);
                                ui.label(RichText::new(status_text).strong().size(13.0).color(app.theme.text_primary));
                            });
                        });

                    if app.broadcast_state == 3 {
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("⚠ {}", app.broadcast_error_msg)).color(app.theme.danger).small());
                    }

                    ui.add_space(15.0);

                    // Action Button based on state
                    let action_btn = match app.broadcast_state {
                        0 | 3 => egui::Button::new(RichText::new("🚀 GO LIVE").strong().color(app.theme.text_primary)).fill(app.theme.accent.linear_multiply(0.4)),
                        _ => egui::Button::new(RichText::new("🛑 STOP STREAM").strong().color(app.theme.text_primary)).fill(app.theme.danger),
                    };

                    if ui.add_sized([ui.available_width(), 32.0], action_btn).clicked() {
                        if app.broadcast_state == 0 || app.broadcast_state == 3 {
                            app.broadcast_state = 1; // Start connecting
                            app.is_streaming = true;
                        } else {
                            app.broadcast_state = 0; // Turn off
                            app.is_streaming = false;
                        }
                    }
                });

            ui.add_space(15.0);

            ui.strong("Live Health Telemetry");
            ui.add_space(4.0);
            Frame::none()
                .fill(app.theme.bg_surface)
                .rounding(app.theme.radius_md)
                .stroke(app.theme.border_stroke)
                .inner_margin(app.theme.space_md)
                .show(ui, |ui| {
                    if app.broadcast_state == 2 {
                        let time = ui.input(|i| i.time);
                        let uptime_sec = (time % 3600.0) as u32;
                        let min = uptime_sec / 60;
                        let sec = uptime_sec % 60;

                        ui.label(format!("Uptime: {:02}:{:02}", min, sec));
                        ui.label(format!("Outgoing Bitrate: {:.1} kbps", app.broadcast_bitrate - (time as f32 * 0.1).sin() * 5.0));
                        ui.label("Dropped Frames: 0 (0.00%)");
                        ui.label("Viewer Count: 14 concurrent listeners");
                    } else if app.broadcast_state == 1 {
                        ui.label("Uptime: --:--");
                        ui.label("Outgoing Bitrate: Negotiating...");
                        ui.label("Dropped Frames: --");
                        ui.label("Viewer Count: --");
                    } else {
                        ui.label("Uptime: --:--");
                        ui.label("Outgoing Bitrate: 0.0 kbps");
                        ui.label("Dropped Frames: 0");
                        ui.label("Viewer Count: 0");
                    }
                });
        });
    });
}
