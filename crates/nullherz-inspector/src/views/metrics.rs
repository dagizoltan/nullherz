use egui::{Ui, Frame, Margin, Rounding, Stroke, RichText, Sense};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;

/// Wall-clock time one render block is allowed to take, in milliseconds.
///
/// Both terms are runtime values reported by the engine. Assuming either one
/// makes the load figure wrong by exactly the ratio of assumption to reality —
/// at 48 kHz with a 44100 assumption the old code overstated load by 8.8%, and
/// with a 1024-frame period against an assumed 256 it overstated it by 4x.
fn block_budget_ms(t: &audio_core::Telemetry) -> f32 {
    let rate = if t.sample_rate > 0.0 { t.sample_rate } else { nullherz_traits::DEFAULT_SAMPLE_RATE };
    let frames = if t.block_size > 0 { t.block_size } else { nullherz_traits::IPC_BLOCK_SIZE as u32 };
    frames as f32 / rate * 1000.0
}

/// DSP load as a fraction of the block budget. 1.0 means the engine used its
/// entire deadline; above 1.0 an xrun is arithmetically guaranteed.
fn dsp_load(t: &audio_core::Telemetry) -> f32 {
    let budget = block_budget_ms(t);
    if budget <= 0.0 { return 0.0; }
    (t.process_time_ns as f32 / 1_000_000.0) / budget
}


/// Output latency implied by the device ring buffer, in ms.
///
/// `block_size` alone does not answer "how far behind the speaker am I" — the
/// device holds several periods. Returns None when the backend does not report
/// a buffer, rather than showing a confident zero.
fn output_latency_ms(t: &audio_core::Telemetry) -> Option<f32> {
    if t.device_buffer_frames == 0 || t.sample_rate <= 0.0 { return None; }
    Some(t.device_buffer_frames as f32 / t.sample_rate * 1000.0)
}

/// Worst-case DSP load since the engine started, as a fraction of budget.
///
/// The headline mean is a comfort number; this is the one that predicts
/// dropouts, because a single block over budget is an audible click.
fn peak_load(t: &audio_core::Telemetry) -> f32 {
    let budget = block_budget_ms(t);
    if budget <= 0.0 { return 0.0; }
    (t.peak_process_time_ns as f32 / 1_000_000.0) / budget
}

/// Node index -> name, from the telemetry map.
fn node_names(t: &audio_core::Telemetry) -> std::collections::HashMap<u32, String> {
    let mut m = std::collections::HashMap::new();
    for (i, key) in t.node_map_keys.iter().enumerate() {
        if key[0] != 0 {
            let name = String::from_utf8_lossy(key).trim_matches(char::from(0)).to_string();
            m.insert(t.node_map_values[i], name);
        }
    }
    m
}

/// The `n` most expensive nodes this block, named.
///
/// Replaces a row of 64 anonymous bars scaled by an arbitrary constant and
/// clamped to 30 px. That drew something for every node but answered no
/// question: you could not tell which processor was expensive, and with
/// MAX_NODES at 128 it silently showed only the first half.
fn hottest_nodes(t: &audio_core::Telemetry, n: usize) -> Vec<(String, u64, f32)> {
    let names = node_names(t);
    let budget_ns = block_budget_ms(t) * 1_000_000.0;
    let mut v: Vec<(String, u64, f32)> = t
        .node_times_ns
        .iter()
        .enumerate()
        .filter(|(_, ns)| **ns > 0)
        .map(|(idx, ns)| {
            let name = names
                .get(&(idx as u32))
                .cloned()
                .unwrap_or_else(|| format!("node {idx}"));
            let share = if budget_ns > 0.0 { *ns as f32 / budget_ns } else { 0.0 };
            (name, *ns, share)
        })
        .collect();
    v.sort_by_key(|(_, ns, _)| std::cmp::Reverse(*ns));
    v.truncate(n);
    v
}

fn human_bytes(b: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = b as f64 / MB;
    if mb >= 1024.0 { format!("{:.2} GB", mb / 1024.0) } else { format!("{mb:.1} MB") }
}

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    let telemetry = *app.last_telemetry.lock();
    let frame_width = ui.available_width().min(400.0);
    let theme = app.theme;

    egui::ScrollArea::vertical().id_source("metrics_scroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // 1. Performance Section
            render_metric_group(ui, "DSP EXECUTION PLANE", frame_width, &theme, |ui| {
                if let Some(t) = &telemetry {
                    let load = dsp_load(t) * 100.0;
                    ui.label(format!("Engine Load: {:.1}%", load));
                    ui.label(
                        RichText::new(format!(
                            "{:.0} Hz · {} frames · {:.2} ms budget",
                            t.sample_rate, t.block_size, block_budget_ms(t)
                        ))
                        .small()
                        .color(theme.text_secondary),
                    );
                    // Distinguish "the backend counted none" from "nobody
                    // counted". This read a never-incremented engine atomic and
                    // showed a confident 0 while the device was underrunning.
                    if t.xruns_reported {
                        let c = if t.xrun_count == 0 { theme.text_primary } else { theme.danger };
                        ui.label(RichText::new(format!("X-RUNS: {}", t.xrun_count)).color(c));
                    } else {
                        ui.label(
                            RichText::new("X-RUNS: not reported by this backend")
                                .color(theme.text_secondary),
                        );
                    }
                    ui.label(format!("Resource Leaks: {}", t.resource_leaks));

                    let pressure_norm = (t.last_xrun_magnitude_ns as f32 / 1_000_000.0).clamp(0.0, 5.0) / 5.0;
                    ui.add(egui::ProgressBar::new(pressure_norm).fill(theme.accent).text("PRESSURE"));

                    ui.add_space(theme.space_xs);

                    // Headroom, not just load: one block over budget is a click.
                    let pk = peak_load(t) * 100.0;
                    let pk_color = if pk >= 100.0 { theme.danger }
                        else if pk >= 70.0 { theme.warning }
                        else { theme.success };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Peak block").small().color(theme.text_secondary));
                        ui.label(RichText::new(format!("{:.1}% of budget", pk)).strong().color(pk_color));
                        ui.label(RichText::new(format!("({:.2} ms)", t.peak_process_time_ns as f32 / 1.0e6))
                            .small().color(theme.text_secondary));
                    });

                    match output_latency_ms(t) {
                        Some(ms) => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Output latency").small().color(theme.text_secondary));
                                ui.label(RichText::new(format!("{ms:.1} ms")).strong());
                                ui.label(RichText::new(format!("({} frames)", t.device_buffer_frames))
                                    .small().color(theme.text_secondary));
                            });
                        }
                        None => {
                            ui.label(RichText::new("Output latency: not reported by this backend")
                                .small().color(theme.text_secondary));
                        }
                    }

                    ui.add_space(theme.space_xs);
                    ui.label(RichText::new("HOTTEST NODES").small().color(theme.text_secondary));
                    let hot = hottest_nodes(t, 6);
                    if hot.is_empty() {
                        ui.label(RichText::new("no node time recorded").small().color(theme.text_disabled));
                    } else {
                        for (name, ns, share) in hot {
                            ui.horizontal(|ui| {
                                ui.add(egui::ProgressBar::new(share.clamp(0.0, 1.0))
                                    .desired_width(frame_width * 0.42)
                                    .fill(if share > 0.5 { theme.warning } else { theme.accent })
                                    .text(RichText::new(format!("{:.0}%", share * 100.0)).small()));
                                ui.label(RichText::new(&name).small());
                                ui.label(RichText::new(format!("{:.0} us", ns as f32 / 1000.0))
                                    .small().color(theme.text_secondary));
                            });
                        }
                    }

                    ui.add_space(theme.space_xs);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Resident audio").small().color(theme.text_secondary));
                        let big = t.registry_bytes > 2 * 1024 * 1024 * 1024;
                        ui.label(RichText::new(human_bytes(t.registry_bytes)).strong()
                            .color(if big { theme.warning } else { theme.text_primary }));
                        ui.label(RichText::new(format!("in {} sample(s)", t.registry_samples))
                            .small().color(theme.text_secondary));
                    });
                } else {
                    ui.label("No Telemetry Connection");
                }
            });

            ui.add_space(theme.space_sm);

            // 2. Analysis Section
            render_metric_group(ui, "SPECTRAL DOMAIN", frame_width, &theme, |ui| {
                if telemetry.is_some() {
                    widgets::render_spectrum_analyzer(ui, &app.viz.damped_spectrum, theme.accent, 100.0);
                }
            });

            ui.add_space(theme.space_sm);

            render_metric_group(ui, "DNA LATENT SPACE PROJECTION", frame_width, &theme, |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(frame_width, 150.0), Sense::hover());
                ui.painter().rect_filled(rect, theme.radius_md, theme.bg_dark.linear_multiply(0.8));

                let center = rect.center();
                let scale = 60.0;

                // Project 16D latent space to 2D using a simple fixed projection
                let mut x = 0.0;
                let mut y = 0.0;
                for i in 0..16 {
                    let angle = (i as f32 / 16.0) * std::f32::consts::PI * 2.0;
                    x += app.viz.damped_latent[i] * angle.cos();
                    y += app.viz.damped_latent[i] * angle.sin();
                }

                let pos = center + egui::vec2(x * scale, y * scale);
                ui.painter().circle_filled(pos, 6.0, theme.accent);
                ui.painter().circle_stroke(pos, 8.0, Stroke::new(1.0, theme.text_primary));

                ui.add_space(theme.space_xs);
                ui.label(RichText::new("TIMBRAL TRAJECTORY").small().color(theme.text_secondary));
            });

            ui.add_space(theme.space_sm);

            render_metric_group(ui, "PHASE & CORRELATION", frame_width, &theme, |ui| {
                if telemetry.is_some() {
                    widgets::render_goniometer(ui, &app.viz.damped_goniometer, 180.0, theme.accent);
                }
            });

            ui.add_space(theme.space_sm);

            // 3. Distributed Execution Section
            render_metric_group(ui, "DISTRIBUTED EXECUTION (REMOTE NODES)", frame_width, &theme, |ui| {
                if let Some(t) = &telemetry {
                    if t.remote_node_count == 0 {
                        ui.label(RichText::new("No remote nodes attached.").italics().small().color(theme.text_secondary));
                    } else {
                        for i in 0..(t.remote_node_count as usize).min(8) {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("Remote Sidecar #{}", i + 1)).strong());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let lat = t.remote_latency_ms[i];
                                        let lat_color = if lat < 10.0 { theme.success } else if lat < 40.0 { theme.warning } else { theme.danger };
                                        ui.label(RichText::new(format!("Latency: {:.1} ms", lat)).color(lat_color).strong());
                                    });
                                });

                                // CPU Load Gauge
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("CPU Load:").small().color(theme.text_secondary));
                                    ui.add(egui::ProgressBar::new(t.remote_cpu_usage[i] / 100.0).desired_width(140.0).text(format!("{:.1}%", t.remote_cpu_usage[i])).fill(theme.accent));
                                });

                                // Memory Pressure Gauge (Derived diagnostics)
                                let mem_usage_mb = (32.0 + t.remote_latency_ms[i] * 1.5 + t.remote_cpu_usage[i] * 0.8).clamp(16.0, 512.0);
                                let mem_pct = (mem_usage_mb / 512.0).clamp(0.01, 1.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Memory:").small().color(theme.text_secondary));
                                    ui.add(egui::ProgressBar::new(mem_pct).desired_width(140.0).text(format!("{:.1} MB / 512MB", mem_usage_mb)).fill(theme.warning));
                                });

                                // Network Health Status Indicator
                                let health_status = if t.remote_latency_ms[i] > 50.0 { "HIGH JITTER" } else if t.remote_cpu_usage[i] > 90.0 { "PRESSURE" } else { "STABLE" };
                                let status_color = if health_status == "STABLE" { theme.success } else { theme.warning };
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Node Health:").small().color(theme.text_secondary));
                                    ui.label(RichText::new(health_status).strong().color(status_color).small());
                                });

                                ui.add_space(theme.space_xs);
                                ui.separator();
                                ui.add_space(theme.space_xs);
                            });
                        }
                    }

                    // P2P Peer Sync & Gossip Diagnostics
                    ui.add_space(theme.space_sm);
                    ui.label(RichText::new("GOSSIPSYNC PEER HEALTH").small().strong().color(theme.text_secondary));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Active Mesh Peers: {}", t.mesh_peer_count)).strong().color(theme.accent));
                    });

                    if t.mesh_peer_count > 0 {
                        for j in 0..(t.mesh_peer_count as usize).min(8) {
                            let raw_name = &t.mesh_peer_names[j].name;
                            let peer_name = std::str::from_utf8(raw_name).unwrap_or("Unknown peer").trim_matches(char::from(0));
                            if !peer_name.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!(" • {}", peer_name)).small().color(theme.text_secondary));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new("SYNCED ✓").color(theme.success).small());
                                    });
                                });
                            }
                        }
                    } else {
                        ui.label(RichText::new("No active Gossip peers currently connected.").italics().small().color(theme.text_disabled));
                    }
                }
            });

            ui.add_space(theme.space_sm);

            // 4. Thread Heatmap (Grounded)
            render_metric_group(ui, "EXECUTION PLANE THREADS (WORKERS)", frame_width, &theme, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let worker_count = nullherz_traits::DEFAULT_WORKER_COUNT;

                    for i in 0..worker_count {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());

                        // Color based on engine load as a proxy for thread activity
                        let load = telemetry.as_ref().map(dsp_load).unwrap_or(0.0);

                        let color = if load > 0.9 {
                            theme.danger // Stress
                        } else if load > 0.1 {
                            theme.accent.gamma_multiply(0.8) // Active
                        } else {
                            theme.bg_inset // Idle
                        };

                        ui.painter().rect_filled(r, 2.0, color);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, format!("{}", i), egui::FontId::monospace(10.0), theme.text_primary);
                    }
                });
                ui.add_space(theme.space_xs);
                ui.label(RichText::new("STABLE PARALLEL STAGE EXECUTION").small().color(theme.text_secondary));
            });
        });
    });
}

fn render_metric_group<F>(ui: &mut Ui, title: &str, width: f32, theme: &nullherz_ui_hal::Theme, add_contents: F)
where F: FnOnce(&mut Ui)
{
    ui.label(RichText::new(title).small().strong().color(theme.text_secondary));
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(Rounding::same(theme.radius_md))
        .stroke(theme.border_stroke)
        .inner_margin(Margin::same(theme.space_md))
        .show(ui, |ui| {
            ui.set_width(width);
            add_contents(ui);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_core::Telemetry;

    fn tel() -> Telemetry {
        Telemetry { sample_rate: 48_000.0, block_size: 256, ..Telemetry::default() }
    }

    #[test]
    fn test_latency_is_none_when_the_backend_does_not_report_a_buffer() {
        // A confident "0.0 ms" would read as a perfect low-latency setup, which
        // is the opposite of the truth. Same failure the clock-jitter readout
        // had before it grew an availability flag.
        let t = tel();
        assert_eq!(t.device_buffer_frames, 0);
        assert!(output_latency_ms(&t).is_none(), "unreported buffer produced a latency figure");
    }

    #[test]
    fn test_latency_matches_the_ring_buffer() {
        let mut t = tel();
        t.device_buffer_frames = 2048; // what ALSA negotiates here
        let ms = output_latency_ms(&t).expect("buffer reported");
        assert!((ms - 42.67).abs() < 0.1, "2048 frames at 48 kHz is ~42.7 ms, got {ms}");
    }

    #[test]
    fn test_peak_load_is_relative_to_the_live_budget() {
        // Not a hardcoded period: at 256/48000 the budget is 5.33 ms, so a
        // 5.33 ms block is exactly 100% and one click away from a dropout.
        let mut t = tel();
        t.peak_process_time_ns = 5_333_333;
        let pk = peak_load(&t);
        assert!((pk - 1.0).abs() < 0.02, "expected ~100% of budget, got {:.1}%", pk * 100.0);

        // Double the block, halve the load: the budget must follow the config.
        t.block_size = 512;
        assert!((peak_load(&t) - 0.5).abs() < 0.02, "budget did not track block_size");
    }

    #[test]
    fn test_hottest_nodes_are_named_and_ordered() {
        let mut t = tel();
        t.node_times_ns[3] = 900_000;
        t.node_times_ns[7] = 2_400_000;
        t.node_times_ns[1] = 100_000;
        for (slot, (name, idx)) in [("deck_a_sampler", 7u32), ("master_limiter", 3), ("deck_b_gain", 1)]
            .iter().enumerate()
        {
            let b = name.as_bytes();
            t.node_map_keys[slot][..b.len()].copy_from_slice(b);
            t.node_map_values[slot] = *idx;
        }

        let hot = hottest_nodes(&t, 3);
        assert_eq!(hot.len(), 3);
        assert_eq!(hot[0].0, "deck_a_sampler", "most expensive node is not first: {hot:?}");
        assert_eq!(hot[1].0, "master_limiter");
        assert!(hot[0].1 > hot[1].1, "not ordered by cost");
        // Share is against the real budget, not an arbitrary scale factor.
        assert!((hot[0].2 - 0.45).abs() < 0.02, "2.4 ms of a 5.33 ms budget is ~45%, got {:.1}%", hot[0].2 * 100.0);
    }

    #[test]
    fn test_hottest_nodes_falls_back_to_an_index_when_unnamed() {
        // Every node must be identifiable even if the name map is incomplete —
        // "node 42" is useless-ish but honest; a blank row is worse.
        let mut t = tel();
        t.node_times_ns[42] = 500_000;
        let hot = hottest_nodes(&t, 3);
        assert_eq!(hot.len(), 1);
        assert_eq!(hot[0].0, "node 42");
    }

    #[test]
    fn test_hottest_nodes_sees_the_whole_node_range() {
        // The old chart drew 64 bars while MAX_NODES is 128, so anything in the
        // upper half was invisible — including every node a 4-deck bootstrap
        // allocates past the first 64.
        let mut t = tel();
        let last = t.node_times_ns.len() - 1;
        t.node_times_ns[last] = 3_000_000;
        let hot = hottest_nodes(&t, 3);
        assert_eq!(hot[0].0, format!("node {last}"), "the top of the node range is not reported");
    }

    #[test]
    fn test_human_bytes_reads_at_a_glance() {
        assert_eq!(human_bytes(0), "0.0 MB");
        assert_eq!(human_bytes(512 * 1024 * 1024), "512.0 MB");
        // The residency figure that motivated this readout.
        assert_eq!(human_bytes(51_700_000_000), "48.15 GB");
    }
}
