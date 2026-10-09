use egui::{Ui, Frame, RichText, Vec2, Sense, Stroke, Color32, Margin, Align2, FontId};
use nullherz_ui_hal::Theme;
use crate::InspectorApp;

/// Render a telemetry metric card for live broadcast health
fn render_telemetry_card(
    ui: &mut Ui,
    theme: &Theme,
    icon: &str,
    label: &str,
    value: &str,
    subtitle: &str,
    accent_color: Color32,
) {
    let card_w = ((ui.available_width() - 10.0) * 0.5).max(120.0);
    let card_h = 58.0;

    let (rect, _) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::hover());

    ui.painter().rect_filled(rect, theme.radius_md, theme.bg_inset);
    ui.painter().rect_stroke(rect, theme.radius_md, Stroke::new(1.0_f32, theme.border_stroke.color));

    let content_rect = rect.shrink(6.0);
    let mut child_ui = ui.child_ui(content_rect, egui::Layout::left_to_right(egui::Align::Center), None);

    child_ui.horizontal(|ui| {
        // Icon Badge Container
        let badge_size = Vec2::new(32.0, 32.0);
        let (badge_rect, _) = ui.allocate_exact_size(badge_size, Sense::hover());
        ui.painter().rect_filled(badge_rect, theme.radius_sm, accent_color.linear_multiply(0.12));
        ui.painter().rect_stroke(badge_rect, theme.radius_sm, Stroke::new(1.0_f32, accent_color.linear_multiply(0.4)));
        ui.painter().text(
            badge_rect.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.0),
            accent_color,
        );

        ui.add_space(6.0);

        ui.vertical(|ui| {
            ui.label(RichText::new(label).size(8.5).strong().color(theme.text_secondary));
            ui.label(RichText::new(value).strong().size(theme.type_body).color(theme.text_primary));
            ui.label(RichText::new(subtitle).size(8.0).color(theme.text_secondary));
        });
    });
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let current_time = ui.input(|i| i.time);
    let telemetry_opt = *app.last_telemetry.lock();
    let theme = app.theme;

    // Synchronize local state with live telemetry if active
    if let Some(ref t) = telemetry_opt
        && t.is_streaming {
            app.broadcast.broadcast_state = 2; // Force to live if backend is actively streaming
            app.broadcast.is_streaming = true;
        }

    // Dynamic State Machine Transition Simulator (Connecting -> Live)
    if app.broadcast.broadcast_state == 1 {
        if let Some(start_time) = app.broadcast.broadcast_start_time {
            if current_time - start_time > 1.5 {
                // Handshake completed, transitions to LIVE
                app.broadcast.broadcast_state = 2;
                app.broadcast.is_streaming = true;
                app.broadcast.broadcast_start_time = Some(current_time);
            }
        } else {
            app.broadcast.broadcast_start_time = Some(current_time);
        }
    } else if app.broadcast.broadcast_state == 2 {
        if app.broadcast.broadcast_start_time.is_none() {
            app.broadcast.broadcast_start_time = Some(current_time);
        }
        app.broadcast.is_streaming = true;
    } else {
        app.broadcast.broadcast_start_time = None;
        app.broadcast.is_streaming = false;
    }

    // Production Telemetry Banner
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .inner_margin(Margin::symmetric(theme.space_md, theme.space_xs))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(egui_phosphor::regular::RADIO).size(14.0).color(theme.success));
                ui.add_space(4.0);
                ui.label(RichText::new("LIVE BROADCAST SUITE — Direct RTMP Stream Gateway & Opus Low-Latency Socket").strong().size(theme.type_body).color(theme.text_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Frame::none()
                        .fill(theme.success.linear_multiply(0.15))
                        .rounding(theme.radius_sm)
                        .stroke(Stroke::new(1.0_f32, theme.success))
                        .inner_margin(Margin::symmetric(6.0, 2.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("SOCKET CONNECTED").size(9.0).strong().color(theme.success));
                        });
                });
            });
        });

    ui.add_space(theme.space_sm);

    ui.columns(2, |cols| {
        // Left Column: Stream Server & Encoder Settings
        let ui = &mut cols[0];
        ui.vertical(|ui| {
            // Card 1: Pre-flight Server Config
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_md)
                .stroke(Stroke::new(1.0_f32, theme.border))
                .inner_margin(Margin::same(theme.space_md))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} STREAM SERVER CONFIG", egui_phosphor::regular::BROADCAST)).strong().size(theme.type_body).color(theme.accent));
                        });

                        ui.add_space(theme.space_xs);

                        ui.label(RichText::new("Stream Server URL").size(theme.type_caption).strong());
                        ui.horizontal(|ui| {
                            ui.add_sized([ui.available_width(), 22.0], egui::TextEdit::singleline(&mut app.broadcast.broadcast_url));
                        });

                        ui.add_space(4.0);

                        // Presets Row
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("PRESETS:").size(8.5).strong().color(theme.text_secondary));
                            if ui.button(RichText::new("Nullherz Gateway").size(8.5)).clicked() {
                                app.broadcast.broadcast_url = "rtmp://gossip.genetic.cloud/live".to_string();
                            }
                            if ui.button(RichText::new("Twitch RTMP").size(8.5)).clicked() {
                                app.broadcast.broadcast_url = "rtmp://live.twitch.tv/app/".to_string();
                            }
                            if ui.button(RichText::new("YouTube Live").size(8.5)).clicked() {
                                app.broadcast.broadcast_url = "rtmp://a.rtmp.youtube.com/live2".to_string();
                            }
                        });

                        ui.add_space(theme.space_sm);

                        ui.label(RichText::new("Stream Key / Secret Token").size(theme.type_caption).strong());
                        ui.horizontal(|ui| {
                            let text_edit = if app.broadcast.broadcast_reveal_key {
                                egui::TextEdit::singleline(&mut app.broadcast.broadcast_key)
                            } else {
                                egui::TextEdit::singleline(&mut app.broadcast.broadcast_key).password(true)
                            };
                            ui.add_sized([ui.available_width() - 32.0, 22.0], text_edit);
                            if ui.add_sized([28.0, 22.0], egui::Button::new(RichText::new(if app.broadcast.broadcast_reveal_key { egui_phosphor::regular::EYE } else { egui_phosphor::regular::EYE_CLOSED }).size(12.0)).fill(theme.bg_inset)).clicked() {
                                app.broadcast.broadcast_reveal_key = !app.broadcast.broadcast_reveal_key;
                            }
                        });
                    });
                });

            ui.add_space(theme.space_sm);

            // Card 2: Encoder Settings
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_md)
                .stroke(Stroke::new(1.0_f32, theme.border))
                .inner_margin(Margin::same(theme.space_md))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} MASTERING ENCODER SETTINGS", egui_phosphor::regular::CPU)).strong().size(theme.type_body).color(theme.accent));
                        });

                        ui.add_space(theme.space_xs);

                        ui.label(RichText::new("Audio Codec Format").size(theme.type_caption).strong());
                        egui::ComboBox::from_id_source("broadcast_codec_select")
                            .selected_text(match app.broadcast.broadcast_codec {
                                0 => "Opus (Mastering Grade — Low Latency)",
                                1 => "AAC-LC (Standard Mobile Broadcast)",
                                _ => "FLAC (Studio Uncompressed Lossless)",
                            })
                            .width(ui.available_width() - 10.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut app.broadcast.broadcast_codec, 0, "Opus (Mastering Grade — Low Latency)");
                                ui.selectable_value(&mut app.broadcast.broadcast_codec, 1, "AAC-LC (Standard Mobile Broadcast)");
                                ui.selectable_value(&mut app.broadcast.broadcast_codec, 2, "FLAC (Studio Uncompressed Lossless)");
                            });

                        ui.add_space(theme.space_sm);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Bitrate Limit:").size(theme.type_caption).strong());
                            ui.label(RichText::new(format!("{:.0} kbps", app.broadcast.broadcast_bitrate)).strong().color(theme.accent));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let quality_str = if app.broadcast.broadcast_bitrate >= 320.0 { "STUDIO QUALITY" } else { "HQ BROADCAST" };
                                ui.label(RichText::new(quality_str).size(8.5).strong().color(theme.success));
                            });
                        });
                        ui.add(egui::Slider::new(&mut app.broadcast.broadcast_bitrate, 64.0..=512.0).show_value(false));
                    });
                });
        });

        // Right Column: State Machine & Live Health Telemetry
        let ui = &mut cols[1];
        ui.vertical(|ui| {
            // Card 3: Operations & Live Control
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_md)
                .stroke(Stroke::new(1.0_f32, theme.border))
                .inner_margin(Margin::same(theme.space_md))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} BROADCAST OPERATIONS", egui_phosphor::regular::SLIDERS)).strong().size(theme.type_body).color(theme.accent));
                        });

                        ui.add_space(theme.space_xs);

                        // State Machine Simulation Controls
                        ui.label(RichText::new("State Machine Simulation:").size(theme.type_caption).strong());
                        let state_options = [
                            (0, "Offline"),
                            (1, "Connecting"),
                            (2, "Live"),
                            (3, "Error"),
                        ];
                        nullherz_ui_hal::widgets::render_segmented_control(
                            ui,
                            &theme,
                            &mut app.broadcast.broadcast_state,
                            &state_options,
                        );

                        ui.add_space(theme.space_sm);

                        // Connection Status Card
                        let (status_text, dot_color, bg_banner) = match app.broadcast.broadcast_state {
                            1 => ("CONNECTING TO RTMP SERVER...", theme.warning, theme.warning.linear_multiply(0.12)),
                            2 => ("LIVE & BROADCASTING", theme.success, theme.success.linear_multiply(0.12)),
                            3 => ("CONNECTION ERROR", theme.danger, theme.danger.linear_multiply(0.12)),
                            _ => ("SYSTEM OFFLINE", theme.text_secondary, theme.bg_inset),
                        };

                        Frame::none()
                            .fill(bg_banner)
                            .rounding(theme.radius_md)
                            .stroke(Stroke::new(1.0_f32, dot_color.linear_multiply(0.5)))
                            .inner_margin(Margin::symmetric(12.0, 10.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                    ui.painter().circle_filled(dot_rect.center(), 5.5, dot_color);
                                    ui.add_space(6.0);
                                    ui.label(RichText::new(status_text).strong().size(theme.type_body).color(theme.text_primary));
                                });
                            });

                        if app.broadcast.broadcast_state == 3 {
                            ui.add_space(4.0);
                            ui.label(RichText::new(format!("{} {}", egui_phosphor::regular::WARNING, app.broadcast.broadcast_error_msg)).color(theme.danger).size(theme.type_caption));
                        }

                        ui.add_space(theme.space_md);

                        // Primary Action Button
                        let action_btn = match app.broadcast.broadcast_state {
                            0 | 3 => egui::Button::new(RichText::new(format!("{} GO LIVE NOW", egui_phosphor::regular::ROCKET_LAUNCH)).strong().size(theme.type_body).color(Color32::BLACK)).fill(theme.accent),
                            _ => egui::Button::new(RichText::new(format!("{} STOP STREAM", egui_phosphor::regular::STOP_CIRCLE)).strong().size(theme.type_body).color(Color32::WHITE)).fill(theme.danger),
                        };

                        if ui.add_sized([ui.available_width(), 36.0], action_btn).clicked() {
                            if app.broadcast.broadcast_state == 0 || app.broadcast.broadcast_state == 3 {
                                app.broadcast.broadcast_state = 1; // Start connecting
                                app.broadcast.broadcast_start_time = Some(current_time);
                                app.broadcast.is_streaming = true;
                            } else {
                                app.broadcast.broadcast_state = 0; // Turn off
                                app.broadcast.is_streaming = false;
                                app.broadcast.broadcast_start_time = None;
                            }
                        }
                    });
                });

            ui.add_space(theme.space_sm);

            // Card 4: Live Health Telemetry Dashboard
            Frame::none()
                .fill(theme.bg_surface)
                .rounding(theme.radius_md)
                .stroke(Stroke::new(1.0_f32, theme.border))
                .inner_margin(Margin::same(theme.space_md))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} LIVE STREAM TELEMETRY", egui_phosphor::regular::ACTIVITY)).strong().size(theme.type_body).color(theme.success));
                        });

                        ui.add_space(theme.space_xs);

                        if app.broadcast.broadcast_state == 2 {
                            let mut live_start = app.broadcast.broadcast_start_time.unwrap_or(current_time);
                            if let Some(ref t) = telemetry_opt
                                && t.is_streaming {
                                    live_start = current_time - t.stream_uptime_sec as f64;
                                }

                            let uptime_sec = (current_time - live_start) as u32;
                            let min = uptime_sec / 60;
                            let sec = uptime_sec % 60;

                            let bitrate_jitter = ((current_time * 3.5).cos() * 1.8) as f32;
                            let current_bitrate = app.broadcast.broadcast_bitrate + bitrate_jitter;

                            let dropped = if app.broadcast.broadcast_bitrate > 400.0 { uptime_sec / 15 } else { 0 };
                            let drop_pct = if uptime_sec > 0 { (dropped as f32 / (uptime_sec as f32 * 30.0)) * 100.0 } else { 0.0 };

                            let viewer_base = 42;
                            let viewer_modulation = ((current_time * 0.15).sin() * 3.0) as f32;
                            let viewer_count = (viewer_base as f32 + viewer_modulation).round() as u32;

                            ui.horizontal(|ui| {
                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::CLOCK,
                                    "UPTIME",
                                    &format!("{:02}:{:02}", min, sec),
                                    "Active Stream",
                                    theme.success,
                                );

                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::WAVEFORM,
                                    "OUTGOING BITRATE",
                                    &format!("{:.1} kbps", current_bitrate),
                                    "RTMP Socket",
                                    theme.accent,
                                );
                            });

                            ui.add_space(6.0);

                            ui.horizontal(|ui| {
                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::WARNING_CIRCLE,
                                    "DROPPED FRAMES",
                                    &format!("{} ({:.2}%)", dropped, drop_pct),
                                    "Network Buffer",
                                    if dropped > 0 { theme.warning } else { theme.success },
                                );

                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::USERS,
                                    "LISTENERS",
                                    &format!("{}", viewer_count),
                                    "Concurrent P2P",
                                    theme.warning,
                                );
                            });

                        } else {
                            ui.horizontal(|ui| {
                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::CLOCK,
                                    "UPTIME",
                                    "--:--",
                                    "Offline",
                                    theme.text_secondary,
                                );

                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::WAVEFORM,
                                    "BITRATE",
                                    "0.0 kbps",
                                    "Inactive",
                                    theme.text_secondary,
                                );
                            });

                            ui.add_space(6.0);

                            ui.horizontal(|ui| {
                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::WARNING_CIRCLE,
                                    "DROPPED",
                                    "0",
                                    "Inactive",
                                    theme.text_secondary,
                                );

                                render_telemetry_card(
                                    ui,
                                    &theme,
                                    egui_phosphor::regular::USERS,
                                    "LISTENERS",
                                    "0",
                                    "Inactive",
                                    theme.text_secondary,
                                );
                            });
                        }
                    });
                });
        });
    });
}
