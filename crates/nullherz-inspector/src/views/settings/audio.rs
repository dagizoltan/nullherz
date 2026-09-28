use egui::{Ui, Frame, RichText};
use crate::InspectorApp;
use nullherz_traits::AudioBackendType;

pub fn render_audio(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("Audio Engine Configuration");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Select Audio Backend").color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                let backends = [
                    (AudioBackendType::Alsa, "ALSA"),
                    (AudioBackendType::Jack, "JACK"),
                    (AudioBackendType::Pipewire, "Pipewire"),
                    (AudioBackendType::Threaded, "Threaded"),
                ];

                for (backend, label) in backends {
                    let is_active = app.settings.active_backend == backend;
                    let mut btn = egui::Button::new(label);
                    if is_active {
                        btn = btn.fill(theme.accent.linear_multiply(0.12))
                                 .stroke(egui::Stroke::new(1.0_f32, theme.accent));
                    }
                    if ui.add(btn).clicked() {
                        app.settings.active_backend = backend;
                        let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(backend)));
                    }
                }
            });

            ui.add_space(theme.space_md);
            ui.horizontal(|ui| {
                ui.label("Devices:");
                // DISPLAY ONLY — there is no device-selection command in the
                // protocol yet; the backend opens its default device. The old
                // dropdown let you "select" a device that nothing consumed,
                // and the "Scan" button was an admitted no-op (enumeration
                // already refreshes every tick via telemetry).
                ui.add_enabled_ui(false, |ui| {
                    egui::ComboBox::from_id_source("audio_device_select")
                        .selected_text(
                            app.settings.audio_devices.first().map(String::as_str).unwrap_or("(default)"),
                        )
                        .show_ui(ui, |_ui| {});
                });
                ui.label(
                    RichText::new("detected — output uses the backend default")
                        .size(theme.type_caption)
                        .color(theme.text_disabled),
                );
            });
        });

    ui.add_space(theme.space_md);
    ui.strong("Hardware Low-Latency Optimization");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(
                RichText::new("1-Click Exclusive Performance Mode: Acquires D-Bus ReserveDevice1, bypasses OS desktop sound server resampling, enables direct MMAP kernel buffer transfers and NO_PERIOD_WAKEUP.")
                    .color(theme.text_secondary)
                    .size(theme.type_caption),
            );
            ui.add_space(theme.space_sm);

            let is_exclusive = app.settings.exclusive_performance_mode;
            let btn_text = if is_exclusive {
                "⚡ EXCLUSIVE PERFORMANCE MODE (ACTIVE)"
            } else {
                "⚡ ENGAGE EXCLUSIVE PERFORMANCE MODE (ALSA DIRECT HW MMAP)"
            };

            let mut perf_btn = egui::Button::new(RichText::new(btn_text).strong().color(if is_exclusive { theme.success } else { theme.accent }));
            if is_exclusive {
                perf_btn = perf_btn.fill(theme.success.linear_multiply(0.12)).stroke(egui::Stroke::new(1.5, theme.success));
            }

            if ui.add_sized([ui.available_width(), 32.0], perf_btn).clicked() {
                app.settings.exclusive_performance_mode = !is_exclusive;
                if app.settings.exclusive_performance_mode {
                    unsafe {
                        std::env::set_var("NULLHERZ_ALSA_MMAP", "1");
                        std::env::set_var("NULLHERZ_NO_PERIOD_WAKEUP", "1");
                        std::env::set_var("NULLHERZ_RESERVE_DEVICE", "1");
                    }
                    app.settings.active_backend = AudioBackendType::Alsa;
                    nullherz_backends::alsa::AlsaBackend::reserve_dbus_device("hw:0,0");
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(AudioBackendType::Alsa)));
                } else {
                    unsafe {
                        std::env::remove_var("NULLHERZ_ALSA_MMAP");
                        std::env::remove_var("NULLHERZ_NO_PERIOD_WAKEUP");
                        std::env::remove_var("NULLHERZ_RESERVE_DEVICE");
                    }
                }
            }
        });

    ui.add_space(theme.space_md);
    ui.strong("Real-Time System Environment & Process Permissions");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(
                RichText::new("Verification of kernel thread scheduling, real-time priorities (RLIMIT_RTPRIO), and page memory locking (mlockall).")
                    .color(theme.text_secondary)
                    .size(theme.type_caption),
            );
            ui.add_space(theme.space_sm);

            let sched_status = ipc_layer::audio_thread_sched().unwrap_or_else(ipc_layer::SchedStatus::current);
            let rtprio = ipc_layer::rtprio_limit();
            let governor = ipc_layer::cpu_governor().unwrap_or_else(|| "unknown".into());
            let memlock = ipc_layer::memlock_limit();

            ui.horizontal(|ui| {
                ui.label(RichText::new("Audio Thread Policy:").strong());
                let policy_color = if sched_status.is_realtime() { theme.success } else { theme.danger };
                ui.label(RichText::new(format!("{}", sched_status)).strong().color(policy_color));
            });

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("RLIMIT_RTPRIO Limit:").strong());
                let rtprio_str = rtprio.map(|l| l.to_string()).unwrap_or_else(|| "unreadable".into());
                let rtprio_color = if rtprio.unwrap_or(0) > 0 { theme.text_primary } else { theme.danger };
                ui.label(RichText::new(rtprio_str).color(rtprio_color));

                ui.add_space(theme.space_md);
                ui.label(RichText::new("CPU Governor:").strong());
                ui.label(RichText::new(governor));
            });

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("RLIMIT_MEMLOCK Limit:").strong());
                let memlock_str = match memlock {
                    Some(u64::MAX) => "unlimited".to_string(),
                    Some(b) => format!("{} KiB", b / 1024),
                    None => "unreadable".to_string(),
                };
                ui.label(RichText::new(memlock_str));
            });

            ui.add_space(theme.space_sm);
            let active_warnings = ipc_layer::realtime_environment_warnings();
            if active_warnings.is_empty() {
                ui.label(
                    RichText::new("✔ Real-time environment is fully optimized. Zero preemption risks detected.")
                        .color(theme.success)
                        .strong(),
                );
            } else {
                ui.label(RichText::new("Active Real-time Warnings:").strong().color(theme.danger));
                for warning in &active_warnings {
                    ui.label(RichText::new(format!("• {}", warning)).color(theme.text_primary).size(theme.type_caption));
                }

                ui.add_space(theme.space_xs);
                ui.label(
                    RichText::new("Recommended Fix: Add the following lines to /etc/security/limits.d/99-nullherz-realtime.conf and ensure your user is in the 'audio' group:")
                        .color(theme.text_secondary)
                        .size(theme.type_caption),
                );
                Frame::none()
                    .fill(theme.bg_inset)
                    .rounding(theme.radius_sm)
                    .inner_margin(theme.space_xs)
                    .show(ui, |ui| {
                        ui.monospace("@audio - rtprio 95\n@audio - memlock unlimited");
                    });
            }
        });

    ui.add_space(theme.space_md);
    ui.strong("Soundcard Wiring Test");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(RichText::new("Verify the physical outputs and routing of your audio interface by outputting a test signal.").color(theme.text_secondary));
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                if ui.button("▶ PLAY TEST PREVIEW (TRACK A)").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: 1 }));
                }
                if ui.button("▶ PLAY TEST PREVIEW (TRACK B)").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::Preview { sample_id: 2 }));
                }
                if ui.button("⏹ STOP TEST").clicked() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Performance(nullherz_traits::PerformanceCommand::StopNode {
                        node_idx: nullherz_traits::NodeConventions::PREVIEW,
                    }));
                }
            });
        });
}
