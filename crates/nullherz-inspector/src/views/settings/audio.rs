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
                ui.label("Output Device:");
                let devices = &app.settings.audio_devices;
                let mut selected = app.settings._selected_audio_device.clone();
                let combo_text = if selected.is_empty() {
                    devices.first().cloned().unwrap_or_else(|| "default".to_string())
                } else {
                    selected.clone()
                };

                egui::ComboBox::from_id_source("audio_output_device_select")
                    .selected_text(RichText::new(&combo_text).strong().color(theme.text_primary))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for dev in devices {
                            if ui.selectable_label(dev == &combo_text, dev).clicked() {
                                selected = dev.clone();
                                app.settings._selected_audio_device = dev.clone();

                                let clean_dev = nullherz_backends::alsa::device_id(dev);
                                let mut buf = [0u8; 64];
                                let bytes = clean_dev.as_bytes();
                                let len = bytes.len().min(64);
                                buf[..len].copy_from_slice(&bytes[..len]);

                                let _ = app.command_sender.send(nullherz_traits::Command::Core(
                                    nullherz_traits::CoreCommand::SetAudioOutputDevice(buf)
                                ));
                            }
                        }
                    });

                ui.label(
                    RichText::new("hardware output stream")
                        .size(theme.type_caption)
                        .color(theme.text_secondary),
                );
            });
        });

    ui.add_space(theme.space_md);
    ui.strong("Engine Performance Profile Presets");
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label(
                RichText::new("1-Click Hardware & Performance Presets: Instantly configure sample rate, buffer size, ALSA MMAP, and thread priority.")
                    .color(theme.text_secondary)
                    .size(theme.type_caption),
            );
            ui.add_space(theme.space_sm);

            let presets = [
                ("⚡ Ultra-Low Latency Live / Scratch", "192 kHz / 32 frames | 0.32 ms Latency", 192000.0, 32, true, AudioBackendType::Alsa),
                ("🎧 Stadium DJ & Arena Performance", "96 kHz / 32 frames | 0.48 ms Latency", 96000.0, 32, true, AudioBackendType::Alsa),
                ("🎛️ High-Density Studio Production", "48 kHz / 64 frames | 1.63 ms Latency", 48000.0, 64, true, AudioBackendType::Alsa),
                ("💻 Desktop Convenience & Multi-App", "PipeWire Auto / 256 frames | ~10 ms Latency", 48000.0, 256, false, AudioBackendType::Pipewire),
            ];

            for (title, desc, rate, block, direct_mmap, backend) in presets {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).strong().color(theme.text_primary));
                        ui.label(RichText::new(desc).size(theme.type_caption).color(theme.text_secondary));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Apply Preset").clicked() {
                            app.settings.sample_rate = rate;
                            app.settings.buffer_size = block;
                            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::ConfigureAudioEngine {
                                sample_rate: rate,
                                block_size: block,
                            }));
                            if direct_mmap {
                                let target_dev = if app.settings._selected_audio_device.is_empty() || app.settings._selected_audio_device == "default" {
                                    "hw:0,0".to_string()
                                } else {
                                    nullherz_backends::alsa::device_id(&app.settings._selected_audio_device).to_string()
                                };
                                unsafe {
                                    std::env::set_var("NULLHERZ_ALSA_DEVICE", &target_dev);
                                    std::env::set_var("NULLHERZ_ALSA_MMAP", "1");
                                    std::env::set_var("NULLHERZ_NO_PERIOD_WAKEUP", "1");
                                    std::env::set_var("NULLHERZ_RESERVE_DEVICE", "1");
                                }
                                app.settings.exclusive_performance_mode = true;
                                app.settings.active_backend = AudioBackendType::Alsa;
                                nullherz_backends::alsa::AlsaBackend::reserve_dbus_device(&target_dev);
                                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(AudioBackendType::Alsa)));
                            } else {
                                unsafe {
                                    std::env::remove_var("NULLHERZ_ALSA_DEVICE");
                                    std::env::remove_var("NULLHERZ_ALSA_MMAP");
                                    std::env::remove_var("NULLHERZ_NO_PERIOD_WAKEUP");
                                    std::env::remove_var("NULLHERZ_RESERVE_DEVICE");
                                }
                                app.settings.exclusive_performance_mode = false;
                                app.settings.active_backend = backend;
                                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(backend)));
                            }
                        }
                    });
                });
                ui.add_space(theme.space_xs);
            }

            ui.add_space(theme.space_sm);
            let optimal_profile = nullherz_backends::alsa::probe_optimal_profile();
            let auto_label = format!("🔍 AUTO-DETECT HARDWARE OPTIMAL ({})", optimal_profile.name);
            if ui.add_sized([ui.available_width(), 28.0], egui::Button::new(RichText::new(auto_label).strong().color(theme.accent))).clicked() {
                app.settings.sample_rate = optimal_profile.sample_rate;
                app.settings.buffer_size = optimal_profile.block_size;
                let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::ConfigureAudioEngine {
                    sample_rate: optimal_profile.sample_rate,
                    block_size: optimal_profile.block_size,
                }));
                if optimal_profile.mmap_direct {
                    let target_dev = if app.settings._selected_audio_device.is_empty() || app.settings._selected_audio_device == "default" {
                        "hw:0,0".to_string()
                    } else {
                        nullherz_backends::alsa::device_id(&app.settings._selected_audio_device).to_string()
                    };
                    unsafe {
                        std::env::set_var("NULLHERZ_ALSA_DEVICE", &target_dev);
                        std::env::set_var("NULLHERZ_ALSA_MMAP", "1");
                        std::env::set_var("NULLHERZ_NO_PERIOD_WAKEUP", "1");
                        std::env::set_var("NULLHERZ_RESERVE_DEVICE", "1");
                    }
                    app.settings.exclusive_performance_mode = true;
                    app.settings.active_backend = AudioBackendType::Alsa;
                    nullherz_backends::alsa::AlsaBackend::reserve_dbus_device(&target_dev);
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SwitchBackend(AudioBackendType::Alsa)));
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
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("⚡ FIX REAL-TIME PERMISSIONS (@audio rtprio 95)").strong().color(theme.accent)).clicked() {
                        let _ = ipc_layer::apply_realtime_limits_fix();
                    }
                    ui.label(
                        RichText::new("Generates /etc/security/limits.d/99-nullherz-realtime.conf for zero preemption risk across reboots. Note: Linux PAM requires a one-time logout/login or reboot for rtprio limits to take effect.")
                            .color(theme.text_secondary)
                            .size(theme.type_caption),
                    );
                });
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
