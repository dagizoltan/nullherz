//! Visual Mixer View and Neural Visual Surface rendering.
//! Supports multi-channel visual generators, interactive surface controls,
//! and visual post-processing insert rack chaining on PixelFeedbackEngine framebuffers.

use eframe::egui;
use crate::{state, InspectorApp, Telemetry};

/// Apply a chain of visual post-processing insert sidecars onto the PixelFeedbackEngine framebuffer.
pub fn apply_visual_insert_chain(
    engine: &mut state::PixelFeedbackEngine,
    visual_inserts: &[String],
    color_shift: f32,
    time: f32,
) {
    let w = engine.width;
    let h = engine.height;
    if w == 0 || h == 0 {
        return;
    }

    for insert_id in visual_inserts {
        match insert_id.as_str() {
            "phase-goniometer-2d" => {
                // 2D Phase Goniometer: Draw stereo vector phase trajectory onto framebuffer
                let center_x = w as f32 * 0.5;
                let center_y = h as f32 * 0.5;
                let radius = (w.min(h) as f32) * 0.35;
                let points = 32;
                for i in 0..points {
                    let phase = (i as f32 / points as f32) * std::f32::consts::TAU;
                    let l = (phase + time * 3.0).sin();
                    let r = (phase * 1.5 + time * 2.0).cos();
                    let x = center_x + (l - r) * radius * 0.5;
                    let y = center_y - (l + r) * radius * 0.5;
                    let px = x.clamp(0.0, (w - 1) as f32) as usize;
                    let py = y.clamp(0.0, (h - 1) as f32) as usize;
                    let idx = py * w + px;
                    let current = engine.back_buffer[idx];
                    engine.back_buffer[idx] = [
                        current[0].saturating_add(40),
                        current[1].saturating_add(220),
                        current[2].saturating_add(180),
                        255,
                    ];
                }
            }
            "neural-saturation" | "neural-saturator-v" => {
                // Non-linear Padé SIMD color saturation & contrast boost
                let sat_factor = 1.4 + color_shift * 0.6;
                for pixel in engine.back_buffer.iter_mut() {
                    let r = pixel[0] as f32 / 255.0;
                    let g = pixel[1] as f32 / 255.0;
                    let b = pixel[2] as f32 / 255.0;
                    let gray = 0.299 * r + 0.587 * g + 0.114 * b;
                    let r_sat = (gray + (r - gray) * sat_factor).clamp(0.0, 1.0);
                    let g_sat = (gray + (g - gray) * sat_factor).clamp(0.0, 1.0);
                    let b_sat = (gray + (b - gray) * sat_factor).clamp(0.0, 1.0);
                    pixel[0] = (r_sat * 255.0) as u8;
                    pixel[1] = (g_sat * 255.0) as u8;
                    pixel[2] = (b_sat * 255.0) as u8;
                }
            }
            "bloom-filter" => {
                // Anamorphic Bloom Filter: High-pass thresholding & glow diffusion
                let mut glow_buffer = vec![[0u8; 4]; w * h];
                let threshold = 140u8;
                for i in 0..(w * h) {
                    let pix = engine.back_buffer[i];
                    let brightness = (pix[0] as u32 + pix[1] as u32 + pix[2] as u32) / 3;
                    if brightness > threshold as u32 {
                        glow_buffer[i] = [
                            (pix[0] as f32 * 0.6) as u8,
                            (pix[1] as f32 * 0.6) as u8,
                            (pix[2] as f32 * 0.6) as u8,
                            255,
                        ];
                    }
                }
                // Additive 1D horizontal blur pass for anamorphic streak
                for y in 0..h {
                    for x in 1..(w - 1) {
                        let idx = y * w + x;
                        let left = glow_buffer[idx - 1];
                        let right = glow_buffer[idx + 1];
                        let curr = &mut engine.back_buffer[idx];
                        curr[0] = curr[0].saturating_add((left[0] as u16 / 4 + right[0] as u16 / 4) as u8);
                        curr[1] = curr[1].saturating_add((left[1] as u16 / 4 + right[1] as u16 / 4) as u8);
                        curr[2] = curr[2].saturating_add((left[2] as u16 / 4 + right[2] as u16 / 4) as u8);
                    }
                }
            }
            "fft-spectrum-mesh" => {
                // FFT Spectrum Mesh: Render bottom frequency bar overlay
                let num_bars = 16.min(w);
                let bar_width = w / num_bars;
                for b in 0..num_bars {
                    let height = (((b as f32 * 0.5 + time * 4.0).sin() * 0.5 + 0.5) * (h as f32 * 0.3)) as usize;
                    let x_start = b * bar_width;
                    let x_end = ((b + 1) * bar_width).min(w);
                    for y in (h.saturating_sub(height))..h {
                        for x in x_start..x_end {
                            let idx = y * w + x;
                            let current = engine.back_buffer[idx];
                            engine.back_buffer[idx] = [
                                current[0].saturating_add(30),
                                current[1].saturating_add(80),
                                current[2].saturating_add(200),
                                255,
                            ];
                        }
                    }
                }
            }
            "reaction-diffusion-nn" => {
                // Reaction Diffusion Neural Network: Discrete Laplacian feedback step
                for y in 1..(h - 1) {
                    for x in 1..(w - 1) {
                        let idx = y * w + x;
                        let neighbor_sum = engine.back_buffer[(y - 1) * w + x][0] as i32
                            + engine.back_buffer[(y + 1) * w + x][0] as i32
                            + engine.back_buffer[y * w + (x - 1)][0] as i32
                            + engine.back_buffer[y * w + (x + 1)][0] as i32;
                        let laplacian = neighbor_sum / 4 - engine.back_buffer[idx][0] as i32;
                        let diff = (laplacian as f32 * 0.15) as i32;
                        let current = engine.back_buffer[idx];
                        engine.back_buffer[idx] = [
                            (current[0] as i32 + diff).clamp(0, 255) as u8,
                            (current[1] as i32 + diff / 2).clamp(0, 255) as u8,
                            (current[2] as i32 - diff / 2).clamp(0, 255) as u8,
                            255,
                        ];
                    }
                }
            }
            _ => {
                // Generic visual post-processing tint shift
                let shift = (time * 2.0).sin() * 20.0;
                for pixel in engine.back_buffer.iter_mut() {
                    pixel[0] = (pixel[0] as i16 + shift as i16).clamp(0, 255) as u8;
                }
            }
        }
    }

    // Sync back_buffer updates to front_buffer for continuous feedback
    engine.front_buffer.copy_from_slice(&engine.back_buffer);
}

/// Render detached interactive visual surface
pub fn render_detached_interactive_surface(
    app: &mut InspectorApp,
    channel_idx: usize,
    _ctx: &egui::Context,
    ui: &mut egui::Ui,
    telemetry: &Option<Telemetry>,
) {
    let target_idx = channel_idx.min(app.viz.channels.len().saturating_sub(1));

    if let Some(channel) = app.viz.channels.get_mut(target_idx) {
        let available_size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available_size.max(egui::vec2(200.0, 200.0)), egui::Sense::drag());

        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(10, 12, 18));

        let time = ui.input(|i| i.time) * channel.param_speed as f64;

        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let norm_y = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                channel.param_neural_temp = norm_x * 2.5;
                channel.param_color_shift = norm_y;
            }
        }

        let mut _input_energy = 0.0f32;
        let mut stereo_imbalance = 0.0f32;
        if let Some(t) = telemetry {
            _input_energy = t.peak_levels.first().copied().unwrap_or(0.0) * channel.gain_sensitivity;
            stereo_imbalance = app.viz.damped_goniometer.iter().sum::<f32>() / 128.0;
        }

        let low_energy = app.viz.damped_spectrum[0..16].iter().sum::<f32>() / 16.0 * channel.gain_sensitivity;
        let mid_energy = app.viz.damped_spectrum[16..64].iter().sum::<f32>() / 48.0 * channel.gain_sensitivity;
        let high_energy = app.viz.damped_spectrum[64..128].iter().sum::<f32>() / 64.0 * channel.gain_sensitivity;

        let un_damped_peak = telemetry.as_ref().and_then(|t| t.peak_levels.first().copied()).unwrap_or(0.0) * channel.gain_sensitivity;
        let fast_attack = (un_damped_peak - channel.nervous_system.short_term_envelope).max(0.0) * 3.0;

        channel.nervous_system.rms_energy = low_energy * 0.5 + mid_energy * 0.3 + high_energy * 0.2;
        channel.nervous_system.spectral_centroid = (mid_energy * 1000.0 + high_energy * 4000.0) / (low_energy + mid_energy + high_energy + 0.001);
        channel.nervous_system.spectral_flux = (high_energy - low_energy).abs();
        channel.nervous_system.low_band = low_energy;
        channel.nervous_system.mid_band = mid_energy;
        channel.nervous_system.high_band = high_energy;
        channel.nervous_system.transient_density = (high_energy * 2.5 + fast_attack).clamp(0.0, 1.0);
        channel.nervous_system.onset_strength = (low_energy * 3.0 + fast_attack * 2.0).clamp(0.0, 1.0);
        channel.nervous_system.fast_transient_spike = fast_attack.clamp(0.0, 2.0);
        channel.nervous_system.bpm = telemetry.as_ref().map(|t| t.bpm as f32).unwrap_or(120.0);
        channel.nervous_system.beat_phase = telemetry.as_ref().map(|t| t.beat_position as f32 % 1.0).unwrap_or(0.0);
        channel.nervous_system.sub_beat_phase = (channel.nervous_system.beat_phase * 4.0).fract();
        for i in 0..12 {
            channel.nervous_system.pitch_chroma[i] = app.viz.damped_spectrum[i * 10 % 128];
        }
        channel.nervous_system.harmonicity = (mid_energy / (high_energy + 0.001)).clamp(0.0, 1.0);
        channel.nervous_system.noisiness = (high_energy / (low_energy + 0.001)).clamp(0.0, 1.0);
        channel.nervous_system.stereo_width = (channel.stereo_width * (1.0 + stereo_imbalance.abs())).clamp(0.0, 2.0);
        channel.nervous_system.stereo_asymmetry = stereo_imbalance;
        channel.nervous_system.long_term_envelope = app.viz.damped_peaks.iter().sum::<f32>() / 16.0;
        channel.nervous_system.short_term_envelope = un_damped_peak;
        channel.nervous_system.spectral_entropy = (low_energy * mid_energy * high_energy).powf(0.33);
        channel.nervous_system.zero_crossing_rate = high_energy * 0.8;

        let current_time = ui.input(|i| i.time);
        channel.mapper.map(&channel.nervous_system, &mut channel.genome);
        channel.memory.push_snapshot(channel.nervous_system.rms_energy, channel.nervous_system.spectral_centroid, current_time);
        channel.mutation.handle_event(&channel.nervous_system, &channel.memory);

        let mut audio_inputs = [0.0f32; 64];
        for i in 0..32 {
            audio_inputs[i] = app.viz.damped_spectrum[i * 4 % 128] * channel.gain_sensitivity;
        }
        for i in 0..16 {
            audio_inputs[32 + i] = app.viz.damped_goniometer[i * 8 % 128] * channel.gain_sensitivity;
        }
        for i in 0..12 {
            audio_inputs[48 + i] = app.viz.damped_latent[i % 16];
        }
        audio_inputs[60] = low_energy;
        audio_inputs[61] = mid_energy;
        audio_inputs[62] = high_energy;
        audio_inputs[63] = stereo_imbalance;

        let frame_dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.050);
        channel.neuron_net.step(&audio_inputs, frame_dt, channel.param_neural_temp, channel.param_feedback);

        let motor = channel.neuron_net.motor_outputs;

        let zoom = 0.98 + (channel.nervous_system.low_band * 0.08) + channel.nervous_system.fast_transient_spike * 0.05;
        let rot = (time as f32 * 0.2 * channel.param_speed).sin() * 0.02 + motor[1] * 0.04;
        let warp_freq = 4.0 + motor[2] * 4.0;
        let decay = (0.88 + channel.param_feedback * 0.10).clamp(0.70, 0.98);

        channel.feedback_engine.step_feedback_warp(
            zoom,
            rot,
            warp_freq,
            decay,
            time as f32,
            &motor,
        );

        let fb_w = channel.feedback_engine.width as f32;
        let fb_h = channel.feedback_engine.height as f32;
        let fb_center_x = fb_w * 0.5;
        let fb_center_y = fb_h * 0.5;

        let num_fb_pts = 48;
        for i in 0..num_fb_pts - 1 {
            let t1 = (i as f32 / num_fb_pts as f32) * std::f32::consts::TAU;
            let t2 = ((i + 1) as f32 / num_fb_pts as f32) * std::f32::consts::TAU;

            let r1 = (fb_h * 0.38) * (1.0 + (channel.neuron_net.v[i % 64] + 65.0) / 100.0);
            let r2 = (fb_h * 0.38) * (1.0 + (channel.neuron_net.v[(i + 1) % 64] + 65.0) / 100.0);

            let x0 = fb_center_x + (t1 * 3.0 + time as f32).sin() * r1;
            let y0 = fb_center_y + (t1 * 2.0 + time as f32).cos() * r1;
            let x1 = fb_center_x + (t2 * 3.0 + time as f32).sin() * r2;
            let y1 = fb_center_y + (t2 * 2.0 + time as f32).cos() * r2;

            let color = if channel.neuron_net.spikes[i % 64] {
                [255, 220, 80]
            } else {
                [
                    ((i * 12) % 255) as u8,
                    ((255 - i * 8) % 255) as u8,
                    220,
                ]
            };

            channel.feedback_engine.draw_line_additive(x0, y0, x1, y1, color);
        }

        // Chain visual post-processing inserts onto PixelFeedbackEngine framebuffers
        apply_visual_insert_chain(
            &mut channel.feedback_engine,
            &channel.visual_inserts,
            channel.param_color_shift,
            time as f32,
        );

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [channel.feedback_engine.width, channel.feedback_engine.height],
            channel.feedback_engine.front_buffer.as_flattened(),
        );

        let texture_handle = ui.ctx().load_texture(
            format!("fb_tex_{}", target_idx),
            color_image,
            egui::TextureOptions::LINEAR,
        );

        ui.painter().image(
            texture_handle.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        use crate::views::visual_engines::NeuralVisualEngine;

        let nervous = &channel.nervous_system;
        let genome = &channel.genome;

        match channel.generator {
            state::VisualGenerator::RadialMandala => channel.engine_radial_mandala.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::LiquidSurface => channel.engine_liquid_surface.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::SpectralLandscape => channel.engine_spectral_landscape.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::HyperAttractor => channel.engine_hyper_attractor.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::ReactionDiffusion => channel.engine_reaction_diffusion.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::NeuralRaymarcher => channel.engine_neural_raymarcher.render(ui, rect, nervous, genome, telemetry, time as f32),
            state::VisualGenerator::NeuralNcaMesh => channel.engine_neural_nca_mesh.render(ui, rect, nervous, genome, telemetry, time as f32),
        }
    }
}

/// Render the Visual Mixer View page
pub fn render_visuals_view(app: &mut InspectorApp, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
    const VIZ_STRIP_W: f32 = 140.0;
    const VIZ_FADER_H: f32 = 120.0;

    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("VISUAL MIXER").strong().color(app.theme.text_primary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!("{} + Add Visual Channel", egui_phosphor::regular::PLUS)).clicked() {
                let count = app.viz.channels.len() + 1;
                app.viz.channels.push(state::VisualChannel::new(
                    &format!("VIZ {}", count),
                    state::VisualGenerator::RadialMandala,
                    vec![state::VisualInputSource::MasterMix],
                ));
            }
        });
    });
    ui.separator();
    ui.add_space(app.theme.space_xs);

    ui.label(
        egui::RichText::new("Visual Mixer matching system channel strip architecture. Attach stereo audio/MIDI input sources, load neural/algorithmic visual generators, adjust parametric controls, and detach surface windows.")
            .size(app.theme.type_caption)
            .color(app.theme.text_secondary),
    );
    ui.add_space(app.theme.space_sm);

    let theme = app.theme.clone();

    egui::ScrollArea::horizontal().id_source("visual_mixer_scroll").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            let num_channels = app.viz.channels.len();
            let mut channel_to_remove = None;

            for c_idx in 0..num_channels {
                let channel = &mut app.viz.channels[c_idx];
                let is_selected = app.viz.selected_channel_idx == c_idx;
                let channel_color = theme.deck_colors[c_idx % 4];

                egui::Frame::none()
                    .fill(theme.bg_surface)
                    .rounding(egui::Rounding::same(theme.radius_md))
                    .inner_margin(egui::Margin::same(theme.space_md))
                    .stroke(egui::Stroke::new(1.0, if is_selected { theme.accent } else { theme.border }))
                    .show(ui, |ui| {
                        ui.set_width(VIZ_STRIP_W);
                        ui.vertical(|ui| {
                            // Strip Header
                            ui.horizontal(|ui| {
                                ui.add_space((VIZ_STRIP_W - 50.0).max(0.0) / 2.0);
                                if ui.button(egui::RichText::new(&channel.name).strong().size(theme.type_body).color(channel_color)).clicked() {
                                    app.viz.selected_channel_idx = c_idx;
                                }
                                if num_channels > 1 {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button(egui_phosphor::regular::X).on_hover_text("Remove Strip").clicked() {
                                            channel_to_remove = Some(c_idx);
                                        }
                                    });
                                }
                            });
                            ui.add_space(theme.space_xs);

                            // Generator Selector
                            egui::ComboBox::from_id_source(format!("gen_combo_{}", c_idx))
                                .selected_text(channel.generator.name())
                                .show_ui(ui, |ui| {
                                    for generator_item in state::VisualGenerator::all() {
                                        ui.selectable_value(&mut channel.generator, generator_item.clone(), generator_item.name());
                                    }
                                });

                            if channel.generator == state::VisualGenerator::RadialMandala {
                                ui.add_space(2.0);
                                egui::ComboBox::from_id_source(format!("mandala_style_combo_{}", c_idx))
                                    .selected_text(channel.engine_radial_mandala.style.name())
                                    .show_ui(ui, |ui| {
                                        for style_item in crate::views::visual_engines::radial_mandala::MandalaStyle::all() {
                                            ui.selectable_value(&mut channel.engine_radial_mandala.style, *style_item, style_item.name());
                                        }
                                    });
                            } else if channel.generator == state::VisualGenerator::NeuralNcaMesh {
                                ui.add_space(2.0);
                                egui::ComboBox::from_id_source(format!("nca_topology_combo_{}", c_idx))
                                    .selected_text(channel.engine_neural_nca_mesh.topology.name())
                                    .show_ui(ui, |ui| {
                                        for topo_item in crate::views::visual_engines::neural_nca_mesh::NcaMeshTopology::all() {
                                            if ui.selectable_value(&mut channel.engine_neural_nca_mesh.topology, *topo_item, topo_item.name()).clicked() {
                                                channel.engine_neural_nca_mesh.rebuild_mesh();
                                            }
                                        }
                                    });
                            }

                            ui.add_space(2.0);
                            egui::ComboBox::from_id_source(format!("organism_profile_combo_{}", c_idx))
                                .selected_text(&channel.organism_profile.name)
                                .show_ui(ui, |ui| {
                                    for default_prof in crate::views::organism_profile::OrganismProfile::all_defaults() {
                                        if ui.selectable_label(channel.organism_profile.id == default_prof.id, &default_prof.name).clicked() {
                                            channel.organism_profile = default_prof;
                                        }
                                    }
                                });

                            ui.add_space(4.0);

                            // Attached Input Badges / Multi-selection
                            ui.group(|ui| {
                                ui.set_width(VIZ_STRIP_W - 12.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(egui::RichText::new("INPUT SOURCES").size(9.0).strong().color(theme.accent));
                                    for src in state::VisualInputSource::all() {
                                        let is_attached = channel.attached_inputs.contains(src);
                                        let mut check_state = is_attached;
                                        if ui.checkbox(&mut check_state, egui::RichText::new(src.name()).size(9.0)).changed() {
                                            if check_state && !is_attached {
                                                channel.attached_inputs.push(src.clone());
                                            } else if !check_state {
                                                channel.attached_inputs.retain(|s| s != src);
                                            }
                                        }
                                    }
                                });
                            });

                            ui.add_space(4.0);

                            // VISUAL INSERTS RACK (Dynamic Visual Insert Sidecars Chaining)
                            ui.group(|ui| {
                                ui.set_width(VIZ_STRIP_W - 12.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(egui::RichText::new("VISUAL INSERTS RACK").size(theme.type_caption).strong().color(theme.text_secondary));
                                    ui.add_space(2.0);

                                    egui::Frame::none()
                                        .fill(theme.bg_inset)
                                        .rounding(egui::Rounding::same(theme.radius_sm))
                                        .inner_margin(egui::Margin::same(4.0))
                                        .stroke(egui::Stroke::new(1.0, theme.border_stroke.color))
                                        .show(ui, |ui| {
                                            ui.set_width(VIZ_STRIP_W - 20.0);
                                            ui.vertical_centered(|ui| {
                                                ui.label(egui::RichText::new("PARAM CONTROLS").size(9.0).strong().color(theme.accent));
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 2.0;
                                                    nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_speed, 0.1..=4.0, "SPD", channel_color, 24.0);
                                                    nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_neural_temp, 0.0..=2.0, "TMP", channel_color, 24.0);
                                                    nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_feedback, 0.0..=1.0, "FB", channel_color, 24.0);
                                                });
                                            });
                                        });

                                    ui.add_space(4.0);

                                    // Display list of chained visual insert sidecar slots
                                    let mut insert_to_remove = None;
                                    for (ins_idx, ins_id) in channel.visual_inserts.iter().enumerate() {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 2.0;
                                            ui.label(
                                                egui::RichText::new(format!("{}: {}", ins_idx + 1, ins_id))
                                                    .size(8.5)
                                                    .strong()
                                                    .color(theme.text_secondary),
                                            );
                                            if ui.button(egui_phosphor::regular::X).on_hover_text("Remove Insert").clicked() {
                                                insert_to_remove = Some(ins_idx);
                                            }
                                        });
                                    }

                                    if let Some(rem_idx) = insert_to_remove {
                                        channel.visual_inserts.remove(rem_idx);
                                    }

                                    ui.add_space(2.0);

                                    if ui.add_sized([VIZ_STRIP_W - 20.0, 18.0], egui::Button::new(egui::RichText::new("+ VISUAL FX").size(9.0).strong()).fill(theme.bg_inset)).clicked() {
                                        app.active_right_tab = Some(crate::RightTab::Store);
                                        app.store.active_tag_filter = Some("visual".to_string());
                                    }
                                });
                            });

                            ui.add_space(theme.space_sm);

                            // Sensitivity Fader & Dual Stereo Reactivity VU Meters
                            ui.horizontal(|ui| {
                                nullherz_ui_hal::widgets::render_fader(ui, &mut channel.gain_sensitivity, 0.0..=2.0, channel_color, VIZ_FADER_H, 26.0);
                                ui.add_space(4.0);

                                let mut lvl_l = 0.0f32;
                                let mut lvl_r = 0.0f32;
                                if let Some(t) = telemetry {
                                    lvl_l = t.peak_levels.first().copied().unwrap_or(0.0) * channel.gain_sensitivity;
                                    lvl_r = t.peak_levels.get(1).copied().unwrap_or(lvl_l) * channel.gain_sensitivity;
                                }

                                nullherz_ui_hal::widgets::render_vu_meter(ui, lvl_l, lvl_r, channel_color, VIZ_FADER_H);
                            });

                            ui.add_space(theme.space_sm);

                            // Mute / Solo / Detached Window buttons
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if ui.add_sized([30.0, 20.0], egui::SelectableLabel::new(channel.is_muted, "M")).clicked() {
                                    channel.is_muted = !channel.is_muted;
                                }
                                if ui.add_sized([30.0, 20.0], egui::SelectableLabel::new(channel.is_solo, "S")).clicked() {
                                    channel.is_solo = !channel.is_solo;
                                }

                                let is_detached = app.viz.detached_channel == Some(c_idx);
                                let detach_icon = if is_detached {
                                    egui_phosphor::regular::ARROWS_IN_SIMPLE
                                } else {
                                    egui_phosphor::regular::ARROWS_OUT_SIMPLE
                                };

                                if ui.button(detach_icon).on_hover_text("Detach into Surface Window").clicked() {
                                    if is_detached {
                                        app.viz.detached_channel = None;
                                    } else {
                                        app.viz.detached_channel = Some(c_idx);
                                    }
                                }
                            });
                        });
                    });

                ui.add_space(theme.space_md);
            }

            if let Some(idx_to_remove) = channel_to_remove {
                app.viz.channels.remove(idx_to_remove);
                if app.viz.selected_channel_idx >= app.viz.channels.len() {
                    app.viz.selected_channel_idx = app.viz.channels.len().saturating_sub(1);
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visual_channel_inserts_initialization() {
        let channel = state::VisualChannel::new(
            "VIZ TEST",
            state::VisualGenerator::RadialMandala,
            vec![state::VisualInputSource::MasterMix],
        );

        assert!(!channel.visual_inserts.is_empty());
        assert!(channel.visual_inserts.contains(&"phase-goniometer-2d".to_string()));
        assert!(channel.visual_inserts.contains(&"fft-spectrum-mesh".to_string()));
    }

    #[test]
    fn test_apply_visual_insert_chain_framebuffer_processing() {
        let mut engine = state::PixelFeedbackEngine::new(32, 32);
        let initial_pixels = engine.back_buffer.clone();

        let inserts = vec![
            "phase-goniometer-2d".to_string(),
            "neural-saturation".to_string(),
            "bloom-filter".to_string(),
            "fft-spectrum-mesh".to_string(),
            "reaction-diffusion-nn".to_string(),
        ];

        apply_visual_insert_chain(&mut engine, &inserts, 0.5, 1.0);

        // Framebuffer pixels must be modified by post-processing inserts
        assert_ne!(engine.back_buffer, initial_pixels);
        assert_eq!(engine.front_buffer, engine.back_buffer);
    }
}
