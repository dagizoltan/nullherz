// Non-RT plane (UI-side conductor thread and test sync): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use serde::{Deserialize, Serialize};
use eframe::egui;
use std::sync::Arc;
use parking_lot::Mutex;
use audio_core::Telemetry;
use nullherz_traits::Command;
use std::sync::mpsc;
use nullherz_dna::GeneticLibrary;

mod views;
#[cfg(test)]
mod ui_harness;
pub(crate) mod state;

pub fn default_coordinate() -> f32 {
    -1.0
}

pub fn key_to_semitone(key: egui::Key) -> Option<u8> {
    match key {
        // Lower row (C4..D5 base)
        egui::Key::Z => Some(0),  // C
        egui::Key::S => Some(1),  // C#
        egui::Key::X => Some(2),  // D
        egui::Key::D => Some(3),  // D#
        egui::Key::C => Some(4),  // E
        egui::Key::V => Some(5),  // F
        egui::Key::G => Some(6),  // F#
        egui::Key::B => Some(7),  // G
        egui::Key::H => Some(8),  // G#
        egui::Key::N => Some(9),  // A
        egui::Key::J => Some(10), // A#
        egui::Key::M => Some(11), // B
        egui::Key::Comma => Some(12), // C +1
        egui::Key::L => Some(13), // C# +1
        egui::Key::Period => Some(14), // D +1

        // Upper row (+1 Octave)
        egui::Key::Q => Some(12), // C
        egui::Key::Num2 => Some(13), // C#
        egui::Key::W => Some(14), // D
        egui::Key::Num3 => Some(15), // D#
        egui::Key::E => Some(16), // E
        egui::Key::R => Some(17), // F
        egui::Key::Num5 => Some(18), // F#
        egui::Key::T => Some(19), // G
        egui::Key::Num6 => Some(20), // G#
        egui::Key::Y => Some(21), // A
        egui::Key::Num7 => Some(22), // A#
        egui::Key::U => Some(23), // B
        egui::Key::I => Some(24), // C +2
        _ => None,
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeJson {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
    pub name: String,
    #[serde(default = "default_coordinate")]
    pub x: f32,
    #[serde(default = "default_coordinate")]
    pub y: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EdgeJson {
    pub from: u32,
    pub to: u32,
    pub input_idx: u32,
    #[serde(default)]
    pub output_idx: u32,
    #[serde(default)]
    pub buffer_idx: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphJson {
    pub nodes: Vec<NodeJson>,
    pub edges: Vec<EdgeJson>,
    pub node_assignments: nullherz_traits::NodeAssignmentArray,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub enum View {
    Player,
    Console,
    Mixer,
    Composer,
    Library,
    Editor,
    Sampler,
    Breeder,
    Broadcast,
    Topology,
    Account,
    Settings,
    Store,
    Mastering,
    Modulation,
    Visuals,
    // Secondary/Legacy Views
    Tools,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum RightTab {
    Library,
    Store,
    Metrics,
    Notifications,
    GeneticCloud,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum SettingsTab {
    General,
    Audio,
    Midi,
    Network,
    Calibration,
    Preferences,
}


pub struct Track {
    pub title: String,
    pub artist: String,
}

pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
}

pub struct InspectorApp {
    pub(crate) graph: GraphJson,
    pub(crate) command_sender: mpsc::Sender<Command>,
    pub(crate) last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    pub(crate) active_view: View,
    pub(crate) detached_views: std::collections::HashSet<View>,
    // Per-domain view state (see state.rs)
    pub(crate) mixer: state::MixerState,
    pub(crate) decks: state::DeckState,
    pub(crate) library: state::LibraryState,
    pub(crate) store: state::StoreState,
    pub(crate) composer: state::ComposerState,
    pub(crate) sampler: state::SamplerState,
    pub(crate) editor: state::EditorState,
    pub(crate) broadcast: state::BroadcastState,
    pub(crate) settings: state::SettingsState,
    pub(crate) viz: state::VizState,
    pub(crate) topo: state::TopologyViewState,
    pub(crate) library_db: SharedLibraryDb,
    pub(crate) active_right_tab: Option<RightTab>,
    pub(crate) breeding_view: views::breeder::BreederView,
    pub(crate) wgpu_renderer: Option<Arc<Mutex<nullherz_ui_hal::render::wgpu_backend::WgpuRenderer>>>,
    pub(crate) waveform_renderer: Option<Arc<Mutex<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>>>,
    pub(crate) deck_waveform_renderers: [Option<Arc<Mutex<nullherz_ui_hal::render::waveform_renderer::WaveformRenderer>>>; 4],
    pub(crate) discovered_sidecars: Vec<nullherz_traits::SidecarManifest>,
    // --- Broadcast Settings State ---
    pub(crate) p2p_sync_success_toast: Option<f64>,
    pub(crate) export_passport_success_toast: Option<f64>,
    pub(crate) export_passport_error_toast: Option<(f64, String)>,
    pub(crate) theme: nullherz_ui_hal::Theme,
    pub(crate) last_update_time: f64,
    pub(crate) _conductor_thread: Option<std::thread::JoinHandle<()>>,
}

impl InspectorApp {
    pub fn get_cached_track(&self, id: u64) -> Option<nullherz_dna::LibraryTrack> {
        self.library.cached_library_raw.iter().find(|t| t.id == id).cloned()
    }

    pub fn trigger_library_refresh(&mut self) {
        self.library.library_needs_refresh = true;
        let db = self.library_db.clone();
        let crate_name = self.library.active_crate.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.library.bg_library_loader = Some(rx);

        std::thread::spawn(move || {
            let crate_tracks = if let Some(ref name) = crate_name {
                db.get_tracks_in_crate(name).unwrap_or_default()
            } else {
                db.list_tracks().unwrap_or_default()
            };
            let all_tracks = db.list_tracks().unwrap_or_default();
            let crates = db.list_crates().unwrap_or_default();
            let smart_crates = db.list_smart_crates().unwrap_or_default();

            let _ = tx.send(crate::state::LibraryRefreshPayload {
                crate_tracks,
                all_tracks,
                crates,
                smart_crates,
            });
        });
    }

    /// Resolve a node name from the live telemetry node map.
    ///
    /// Returns None when the name is not (yet) published. Callers MUST skip
    /// their command in that case — the old `unwrap_or(0)` fallback silently
    /// redirected every unresolved control to node 0, deck A's sampler
    /// (the crossfader and master gain both did exactly that).
    pub fn render_view_content(&mut self, view: View, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        match view {
            View::Console => views::dj_studio::render(self, ui, telemetry),
            View::Player => views::player::render(self, ui, telemetry),
            View::Sampler => views::sampler::render(self, ui, telemetry),
            View::Mixer => views::mixer::render(self, ui, telemetry),
            View::Library => views::library::render(self, ui),
            View::Topology => views::topology::render(self, ui, telemetry),
            View::Modulation => views::modulation::render(self, ui, telemetry),
            View::Composer => views::composer::render(self, ui, telemetry),
            View::Editor => views::editor::render(self, ui),
            View::Account => views::account::render(self, ui),
            View::Breeder => {
                let mut view_state = std::mem::replace(&mut self.breeding_view, views::breeder::BreederView::new());
                views::breeder::BreederView::show(ui, &mut view_state, telemetry, self);
                self.breeding_view = view_state;
            }
            View::Mastering => views::mastering::render(self, ui, telemetry),
            View::Visuals => self.render_visuals_view(ui, telemetry),
            View::Broadcast => views::broadcast::render(self, ui),
            View::Settings => views::settings::render(self, ui),
            View::Store => views::store::render(self, ui),
            _ => { ui.label("View coming soon..."); }
        }
    }

    pub fn render_detached_interactive_surface(&mut self, channel_idx: usize, _ctx: &egui::Context, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let theme = self.theme.clone();
        let target_idx = channel_idx.min(self.viz.channels.len().saturating_sub(1));

        if let Some(channel) = self.viz.channels.get_mut(target_idx) {
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new(format!("SURFACE — {}", channel.name)).strong().color(theme.accent));
                ui.add_space(20.0);
                ui.label(egui::RichText::new(format!("Generator: {}", channel.generator.name())).size(theme.type_caption).color(theme.text_secondary));
            });
            ui.separator();
            ui.add_space(8.0);

            // Interactive Neural / Algorithmic Visual Surface Render Area
            let available_size = ui.available_size() - egui::vec2(0.0, 160.0);
            let (rect, response) = ui.allocate_exact_size(available_size.max(egui::vec2(300.0, 300.0)), egui::Sense::drag());

            // Background Fill
            ui.painter().rect_filled(rect, theme.radius_md, theme.bg_inset);

            // Respond dynamically to music telemetry (amplitude, frequency, stereo imbalance, DNA latent)
            let time = ui.input(|i| i.time) * channel.param_speed as f64;
            let center = rect.center();

            // Handle user mouse drag on interactive surface to modulate Neural Temperature & Color Shift
            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let norm_y = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                    channel.param_neural_temp = norm_x * 2.0;
                    channel.param_color_shift = norm_y;
                }
            }

            // Execute Pure Native Rust Zero-Allocation Neural Inference Layer
            // Padé SIMD Rational Activation: tanh_approx(x) = x * (27 + x^2) / (27 + 9*x^2)
            let pade_tanh = |x: f32| -> f32 {
                let x2 = x * x;
                (x * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0)
            };

            // Compute Real-time Audio Input Energy / Reactivity
            let mut input_energy = 0.0f32;
            let mut stereo_imbalance = 0.0f32;
            if let Some(t) = telemetry {
                input_energy = t.peak_levels.first().copied().unwrap_or(0.0) * channel.gain_sensitivity;
                stereo_imbalance = self.viz.damped_goniometer.iter().sum::<f32>() / 128.0;
            }

            // Draw Native Rust Neural Network driven Visual Elements
            match channel.generator {
                state::VisualGenerator::NeuralLatentManifold => {
                    let radius = (rect.height() * 0.45).min(rect.width() * 0.45);
                    let num_nodes = 16;
                    let mut node_coords = Vec::with_capacity(16);

                    // Compute 2-layer Neural Feedforward Transformations
                    for i in 0..num_nodes {
                        let raw_latent = self.viz.damped_latent[i];
                        let angle = (i as f32 / num_nodes as f32) * std::f32::consts::TAU + (time as f32 * channel.param_speed * 0.2);

                        // Layer 1 Neural Coupling
                        let h1 = pade_tanh(raw_latent * channel.param_neural_temp + input_energy * 2.0);
                        // Layer 2 Recurrent Feedback
                        let h2 = pade_tanh(h1 * (1.0 + channel.param_feedback) + stereo_imbalance);

                        let r = radius * (0.2 + 0.8 * h2.abs());
                        let pt = egui::pos2(center.x + angle.cos() * r, center.y + angle.sin() * r);
                        node_coords.push(pt);

                        let color = if h2 >= 0.0 {
                            theme.accent.linear_multiply(0.5 + 0.5 * h2.abs())
                        } else {
                            theme.danger.linear_multiply(0.5 + 0.5 * h2.abs())
                        };
                        ui.painter().circle_filled(pt, 5.0 + 3.0 * input_energy, color);
                    }

                    // Render Inter-Layer Neural Synapse Links
                    for i in 0..num_nodes {
                        let next = (i + 1) % num_nodes;
                        ui.painter().line_segment(
                            [node_coords[i], node_coords[next]],
                            egui::Stroke::new(1.5, theme.accent.linear_multiply(0.2 + 0.8 * channel.param_feedback)),
                        );
                    }
                }
                state::VisualGenerator::PhaseGoniometer2D => {
                    let num_pts = 128;
                    for i in 0..num_pts {
                        let g_val = self.viz.damped_goniometer[i];
                        let activated = pade_tanh(g_val * channel.gain_sensitivity * channel.param_neural_temp);
                        let x = rect.left() + (i as f32 / num_pts as f32) * rect.width();
                        let y = rect.center().y - activated * (rect.height() * 0.45);
                        let pt = egui::pos2(x, y);
                        ui.painter().circle_filled(pt, 2.5 + 2.0 * input_energy, theme.success);
                    }
                }
                state::VisualGenerator::FftSpectrumMesh => {
                    let num_bars = 128;
                    let bar_w = (rect.width() / num_bars as f32).max(1.0);
                    for i in 0..num_bars {
                        let spec = self.viz.damped_spectrum[i];
                        let activated = pade_tanh(spec * channel.gain_sensitivity * channel.param_neural_temp);
                        let bar_h = activated.abs() * rect.height();
                        let bar_rect = egui::Rect::from_min_max(
                            egui::pos2(rect.left() + i as f32 * bar_w, rect.bottom() - bar_h),
                            egui::pos2(rect.left() + (i + 1) as f32 * bar_w - 1.0, rect.bottom()),
                        );
                        ui.painter().rect_filled(bar_rect, 1.0, theme.accent.linear_multiply(0.3 + 0.7 * activated));
                    }
                }
                state::VisualGenerator::ReactionDiffusionNN => {
                    let num_rings = 24;
                    for i in 0..num_rings {
                        let wave = pade_tanh(((i as f32 * 0.5 + time as f32 * channel.param_speed) * channel.param_neural_temp).sin());
                        let r = (i as f32 * 12.0 + (time * 30.0) as f32) % (rect.height() * 0.48);
                        ui.painter().circle_stroke(
                            center,
                            r,
                            egui::Stroke::new(1.5 + wave.abs() * 2.0, theme.accent.linear_multiply(1.0 - r / (rect.height() * 0.48))),
                        );
                    }
                }
                state::VisualGenerator::ShaderParticleSwarm => {
                    let count = (channel.param_particle_density * 120.0) as usize;
                    for i in 0..count {
                        let phase = i as f64 * 0.2 + time * channel.param_speed as f64;
                        let raw_x = phase.sin() as f32 * channel.param_neural_temp;
                        let raw_y = (phase * 1.4).cos() as f32 * channel.param_neural_temp;

                        let norm_x = pade_tanh(raw_x + input_energy) * 0.5 + 0.5;
                        let norm_y = pade_tanh(raw_y + stereo_imbalance) * 0.5 + 0.5;

                        let x = rect.left() + norm_x * rect.width();
                        let y = rect.top() + norm_y * rect.height();
                        ui.painter().circle_filled(egui::pos2(x, y), 3.0 + 2.0 * input_energy, theme.success);
                    }
                }
            }

            ui.add_space(8.0);

            // Parametric Controls Bar at the bottom of the detached surface window
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("PARAMETRIC CONTROLS:").strong().color(theme.accent));
                    ui.add_space(10.0);
                    ui.label("Speed:");
                    ui.add(egui::Slider::new(&mut channel.param_speed, 0.1..=4.0));
                    ui.add_space(10.0);
                    ui.label("Neural Temp:");
                    ui.add(egui::Slider::new(&mut channel.param_neural_temp, 0.0..=2.0));
                    ui.add_space(10.0);
                    ui.label("Feedback:");
                    ui.add(egui::Slider::new(&mut channel.param_feedback, 0.0..=1.0));
                    ui.add_space(10.0);
                    ui.label("Sensitivity:");
                    ui.add(egui::Slider::new(&mut channel.gain_sensitivity, 0.0..=2.0));
                });
            });
        }
    }

    pub fn render_visuals_view(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        const VIZ_STRIP_W: f32 = 140.0;
        const VIZ_FADER_H: f32 = 120.0;

        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("VISUAL MIXER").strong().color(self.theme.text_primary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{} + Add Visual Channel", egui_phosphor::regular::PLUS)).clicked() {
                    let count = self.viz.channels.len() + 1;
                    self.viz.channels.push(state::VisualChannel::new(
                        &format!("VIZ {}", count),
                        state::VisualGenerator::ShaderParticleSwarm,
                        vec![state::VisualInputSource::MasterMix],
                    ));
                }
            });
        });
        ui.separator();
        ui.add_space(self.theme.space_xs);

        ui.label(
            egui::RichText::new("Visual Mixer matching system channel strip architecture. Attach stereo audio/MIDI input sources, load neural/algorithmic visual generators, adjust parametric controls, and detach surface windows.")
                .size(self.theme.type_caption)
                .color(self.theme.text_secondary),
        );
        ui.add_space(self.theme.space_sm);

        let theme = self.theme.clone();

        egui::ScrollArea::horizontal().id_source("visual_mixer_scroll").show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let num_channels = self.viz.channels.len();
                let mut channel_to_remove = None;

                for c_idx in 0..num_channels {
                    let channel = &mut self.viz.channels[c_idx];
                    let is_selected = self.viz.selected_channel_idx == c_idx;
                    let channel_color = self.theme.deck_colors[c_idx % 4];

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
                                        self.viz.selected_channel_idx = c_idx;
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

                                // INSERTS RACK (3-Band EQ & FX)
                                ui.group(|ui| {
                                    ui.set_width(VIZ_STRIP_W - 12.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(egui::RichText::new("VISUAL INSERTS").size(theme.type_caption).strong().color(theme.text_secondary));
                                        ui.add_space(2.0);

                                        egui::Frame::none()
                                            .fill(theme.bg_inset)
                                            .rounding(egui::Rounding::same(theme.radius_sm))
                                            .inner_margin(egui::Margin::same(4.0))
                                            .stroke(egui::Stroke::new(1.0, theme.border_stroke.color))
                                            .show(ui, |ui| {
                                                ui.set_width(VIZ_STRIP_W - 20.0);
                                                ui.vertical_centered(|ui| {
                                                    ui.label(egui::RichText::new("1: PARAM EQ").size(9.0).strong().color(theme.accent));
                                                    ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 2.0;
                                                        nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_speed, 0.1..=4.0, "SPD", channel_color, 24.0);
                                                        nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_neural_temp, 0.0..=2.0, "TMP", channel_color, 24.0);
                                                        nullherz_ui_hal::widgets::render_knob_sized(ui, &mut channel.param_feedback, 0.0..=1.0, "FB", channel_color, 24.0);
                                                    });
                                                });
                                            });

                                        ui.add_space(2.0);

                                        if ui.add_sized([VIZ_STRIP_W - 20.0, 18.0], egui::Button::new(egui::RichText::new("+ FX").size(9.0).strong()).fill(theme.bg_inset)).clicked() {
                                            self.active_right_tab = Some(crate::RightTab::Store);
                                            self.store.active_tag_filter = Some("real-time".to_string());
                                        }
                                    });
                                });

                                ui.add_space(theme.space_sm);

                                // Sensitivity Fader & Dual Stereo Reactivity VU Meters
                                ui.horizontal(|ui| {
                                    nullherz_ui_hal::widgets::render_fader(ui, &mut channel.gain_sensitivity, 0.0..=2.0, channel_color, VIZ_FADER_H, 26.0);
                                    ui.add_space(4.0);

                                    // Stereo reactivity meters calculated from attached sources
                                    let mut lvl_l = 0.0f32;
                                    let mut lvl_r = 0.0f32;
                                    if let Some(t) = telemetry {
                                        for src in &channel.attached_inputs {
                                            match src {
                                                state::VisualInputSource::MasterMix => {
                                                    lvl_l = lvl_l.max(self.viz.damped_master_peaks[0]);
                                                    lvl_r = lvl_r.max(self.viz.damped_master_peaks[1]);
                                                }
                                                state::VisualInputSource::DeckA => {
                                                    lvl_l = lvl_l.max(self.viz.damped_peaks[0]);
                                                    lvl_r = lvl_r.max(self.viz.damped_peaks[0] * 0.9);
                                                }
                                                state::VisualInputSource::DeckB => {
                                                    lvl_l = lvl_l.max(self.viz.damped_peaks[1]);
                                                    lvl_r = lvl_r.max(self.viz.damped_peaks[1] * 0.9);
                                                }
                                                state::VisualInputSource::DeckC => {
                                                    lvl_l = lvl_l.max(self.viz.damped_peaks[2]);
                                                    lvl_r = lvl_r.max(self.viz.damped_peaks[2] * 0.9);
                                                }
                                                state::VisualInputSource::DeckD => {
                                                    lvl_l = lvl_l.max(self.viz.damped_peaks[3]);
                                                    lvl_r = lvl_r.max(self.viz.damped_peaks[3] * 0.9);
                                                }
                                                state::VisualInputSource::MicInput => {
                                                    lvl_l = lvl_l.max(t.peak_levels.first().copied().unwrap_or(0.0));
                                                    lvl_r = lvl_r.max(t.peak_levels.first().copied().unwrap_or(0.0));
                                                }
                                                state::VisualInputSource::MidiTriggerBus => {
                                                    lvl_l = lvl_l.max(0.5);
                                                    lvl_r = lvl_r.max(0.5);
                                                }
                                            }
                                        }
                                    }

                                    lvl_l *= channel.gain_sensitivity;
                                    lvl_r *= channel.gain_sensitivity;

                                    nullherz_ui_hal::widgets::render_vu_meter(ui, lvl_l.clamp(0.0, 1.0), lvl_l.clamp(0.0, 1.0), channel_color, VIZ_FADER_H);
                                    ui.add_space(2.0);
                                    nullherz_ui_hal::widgets::render_vu_meter(ui, lvl_r.clamp(0.0, 1.0), lvl_r.clamp(0.0, 1.0), channel_color, VIZ_FADER_H);
                                });

                                ui.add_space(4.0);

                                // Detach Surface Window Button
                                if ui.add_sized([VIZ_STRIP_W - 12.0, 22.0], egui::Button::new(egui::RichText::new(format!("{} Detach Surface", egui_phosphor::regular::ARROW_SQUARE_OUT)).size(9.0).strong()).fill(theme.bg_inset)).clicked() {
                                    self.viz.selected_channel_idx = c_idx;
                                    self.viz.detached_channel = Some(c_idx);
                                }
                            });
                        });

                    ui.add_space(theme.space_sm);
                }

                if let Some(idx) = channel_to_remove {
                    self.viz.channels.remove(idx);
                    self.viz.selected_channel_idx = self.viz.selected_channel_idx.saturating_sub(1);
                }
            });
        });
    }

    pub fn get_node_id(&self, name: &str) -> Option<u32> {
        self.topo.node_map.get(name).copied()
    }

    pub(crate) fn node_names(&self) -> Vec<(String, u32)> {
        // NOTE: We don't try to filter this down to "instrument-only" nodes yet — there's no
        // processor-type metadata exposed to the UI to do that reliably right now.
        // Routing to a non-instrument node just won't produce sound; it won't crash anything.
        self.topo.node_map.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }

    pub fn new(graph: GraphJson, cc: &eframe::CreationContext<'_>) -> Self {
        let theme = nullherz_ui_hal::Theme::default();
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = theme.bg_canvas;
        visuals.window_fill = theme.bg_surface;
        visuals.extreme_bg_color = theme.bg_inset;
        visuals.override_text_color = Some(theme.text_primary);
        visuals.widgets.noninteractive.bg_fill = theme.bg_surface;
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(theme.radius_md);
        visuals.widgets.inactive.bg_fill = theme.bg_inset;
        visuals.widgets.inactive.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.widgets.hovered.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.widgets.active.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.widgets.open.rounding = egui::Rounding::same(theme.radius_sm);
        visuals.window_rounding = egui::Rounding::same(theme.radius_lg);
        cc.egui_ctx.set_visuals(visuals);

        let mut fonts = egui::FontDefinitions::default();

        // Load Inter-Regular
        let inter_reg_bytes = include_bytes!("../assets/fonts/Inter-Regular.ttf");
        fonts.font_data.insert(
            "Inter-Regular".to_owned(),
            egui::FontData::from_static(inter_reg_bytes),
        );

        // Load Inter-Medium
        let inter_med_bytes = include_bytes!("../assets/fonts/Inter-Medium.ttf");
        fonts.font_data.insert(
            "Inter-Medium".to_owned(),
            egui::FontData::from_static(inter_med_bytes),
        );

        // Insert Inter-Regular at the first position for the Proportional font family
        fonts.families.entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Inter-Regular".to_owned());

        // Add egui-phosphor icon font
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

        cc.egui_ctx.set_fonts(fonts);

        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let last_telemetry = Arc::new(Mutex::new(None));

        let raw_db = nullherz_dna::LibraryDatabase::load("library.redb").unwrap_or_else(|e| {
            eprintln!("Warning: Failed to load library.redb ({}). Using transient storage.", e);
            nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase")
        });
        // NOTE: demo tracks are NOT seeded with synthetic metadata here.
        // The folder monitor + analysis worker scan tracks/ and produce REAL
        // bpm / peaks / mip / channel-layout data. The old "repair" block
        // fabricated 3-minute mono metadata for the ~8s stereo demo loops,
        // which post-planar walked the sampler playhead off the buffer and
        // painted waveforms that matched nothing.

        let db_arc = Arc::new(parking_lot::Mutex::new(raw_db));
        let library_db_wrapper = SharedLibraryDb(db_arc.clone());

        let (conductor_thread, _conductor) = start_in_process_conductor(cmd_rx, last_telemetry.clone(), db_arc, None);

        let default_view = View::Console;
        let mut app = Self {
            graph,
            command_sender: cmd_tx,
            last_telemetry,
            _conductor_thread: Some(conductor_thread),
            active_view: default_view,
            detached_views: std::collections::HashSet::new(),
            mixer: Default::default(),
            decks: Default::default(),
            library: Default::default(),
            store: Default::default(),
            composer: Default::default(),
            sampler: Default::default(),
            editor: Default::default(),
            broadcast: Default::default(),
            settings: Default::default(),
            viz: Default::default(),
            topo: Default::default(),
            library_db: library_db_wrapper,
            active_right_tab: Some(RightTab::Library),
            breeding_view: views::breeder::BreederView::new(),
            wgpu_renderer: None,
            waveform_renderer: None,
            deck_waveform_renderers: [None, None, None, None],
            discovered_sidecars: vec![],
            p2p_sync_success_toast: None,
            export_passport_success_toast: None,
            export_passport_error_toast: None,
            theme: nullherz_ui_hal::Theme::default(),
            last_update_time: 0.0,
        };
        app.trigger_library_refresh();

        // Load persisted preferences if they exist
        if let Some(prefs) = std::fs::read_to_string("preferences.json")
            .ok()
            .and_then(|c| serde_json::from_str::<PersistedPreferences>(&c).ok())
        {
            app.settings.restore_last_session = prefs.restore_last_session;
            app.settings.default_view_on_launch = string_to_view(&prefs.default_view_on_launch);
            app.settings.autosave_enabled = prefs.autosave_enabled;
            app.settings.autosave_interval_mins = prefs.autosave_interval_mins;
            app.settings.shortcuts_enabled = prefs.shortcuts_enabled;
            app.active_view = app.settings.default_view_on_launch;

            if let Some(accent) = prefs.theme_accent {
                app.theme.accent = egui::Color32::from_rgba_unmultiplied(accent[0], accent[1], accent[2], accent[3]);
            }
            if let Some(success) = prefs.theme_success {
                app.theme.success = egui::Color32::from_rgba_unmultiplied(success[0], success[1], success[2], success[3]);
            }
            if let Some(danger) = prefs.theme_danger {
                app.theme.danger = egui::Color32::from_rgba_unmultiplied(danger[0], danger[1], danger[2], danger[3]);
            }
        }

        app
    }

    pub fn save_preferences(&self) {
        let prefs = PersistedPreferences {
            restore_last_session: self.settings.restore_last_session,
            default_view_on_launch: view_to_string(self.settings.default_view_on_launch),
            autosave_enabled: self.settings.autosave_enabled,
            autosave_interval_mins: self.settings.autosave_interval_mins,
            shortcuts_enabled: self.settings.shortcuts_enabled,
            theme_accent: Some(self.theme.accent.to_array()),
            theme_success: Some(self.theme.success.to_array()),
            theme_danger: Some(self.theme.danger.to_array()),
        };
        if let Ok(serialized) = serde_json::to_string_pretty(&prefs) {
            let _ = std::fs::write("preferences.json", serialized);
        }
    }

    pub fn deck_color(theme: &nullherz_ui_hal::Theme, i: usize) -> egui::Color32 {
        theme.deck_colors[i % 4]
    }

    fn render_left_sidebar(&mut self, ctx: &egui::Context, active_view: &mut View, id_prefix: &str) {
        egui::SidePanel::left(format!("{}_left_sidebar", id_prefix))
            .resizable(false)
            .default_width(70.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Ω").size(24.0).color(self.theme.accent));
                    ui.add_space(20.0);

                    let top_nav = [
                        (View::Player, egui_phosphor::regular::DISC, "MEDIA PLAYER"),
                        (View::Console, egui_phosphor::regular::RADIO, "DJ CONSOLE"),
                        (View::Mixer, egui_phosphor::regular::SLIDERS, "MIXER STRIPS"),
                        (View::Composer, egui_phosphor::regular::PIANO_KEYS, "COMPOSER"),
                        (View::Library, egui_phosphor::regular::BOOKS, "TRACK LIBRARY"),
                        (View::Editor, egui_phosphor::regular::SCISSORS, "EDITOR"),
                        (View::Sampler, egui_phosphor::regular::MICROPHONE, "SAMPLER"),
                        (View::Breeder, egui_phosphor::regular::DNA, "DNA BREEDER"),
                        (View::Visuals, egui_phosphor::regular::EYE, "NEURAL VISUALS"),
                        (View::Store, egui_phosphor::regular::SHOPPING_BAG, "SIDECAR STORE"),
                        (View::Broadcast, egui_phosphor::regular::BROADCAST, "BROADCAST"),
                    ];

                    let bottom_nav = [
                        (View::Mastering, egui_phosphor::regular::EQUALIZER, "MASTERING EQ"),
                        (View::Topology, egui_phosphor::regular::SHARE_NETWORK, "TOPOLOGY"),
                        (View::Account, egui_phosphor::regular::USER, "ACCOUNT"),
                        (View::Settings, egui_phosphor::regular::GEAR, "SETTINGS"),
                    ];

                    let mut render_nav_btn = |ui: &mut egui::Ui, view: View, icon: &str, label: &str| {
                        let is_selected = *active_view == view;
                        let size = egui::vec2(50.0, 50.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        if response.clicked() {
                            *active_view = view;
                            ui.ctx().request_repaint();
                        }

                        if is_selected {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_md,
                                self.theme.accent.linear_multiply(0.12),
                            );
                            let accent_bar = egui::Rect::from_min_max(
                                rect.left_top() + egui::vec2(2.0, 8.0),
                                rect.left_bottom() + egui::vec2(5.0, -8.0),
                            );
                            ui.painter().rect_filled(accent_bar, 1.5, self.theme.accent);
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_md,
                                self.theme.bg_med.linear_multiply(0.4),
                            );
                        }

                        let icon_color = if is_selected {
                            self.theme.accent
                        } else if response.hovered() {
                            self.theme.text_primary
                        } else {
                            self.theme.text_secondary
                        };

                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icon,
                            egui::FontId::proportional(20.0),
                            icon_color,
                        );

                        response.on_hover_text(label);
                    };

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        for (view, icon, label) in bottom_nav.into_iter().rev() {
                            render_nav_btn(ui, view, icon, label);
                            ui.add_space(10.0);
                        }

                        ui.separator();

                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            egui::ScrollArea::vertical().id_source(format!("{}_nav_scroll", id_prefix)).show(ui, |ui| {
                                for (view, icon, label) in top_nav {
                                    render_nav_btn(ui, view, icon, label);
                                    ui.add_space(10.0);
                                }
                            });
                        });
                    });
                });
            });
    }

    fn render_right_sidebar(&mut self, ctx: &egui::Context, id_prefix: &str) {
        if let Some(tab) = self.active_right_tab {
            let right_panel_frame = egui::Frame::none()
                .fill(self.theme.bg_surface)
                .stroke(self.theme.border_stroke)
                .shadow(self.theme.shadow_md);

            egui::SidePanel::right(format!("{}_right_sidebar", id_prefix))
                .resizable(true)
                .min_width(280.0)
                .max_width(600.0)
                .default_width(450.0)
                .frame(right_panel_frame)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(self.theme.bg_surface)
                        .inner_margin(egui::Margin::symmetric(self.theme.space_md, self.theme.space_sm))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let tabs = [
                                    (RightTab::Library, egui_phosphor::regular::FOLDER_OPEN, "LIBRARY"),
                                    (RightTab::Store, egui_phosphor::regular::SHOPPING_BAG, "STORE"),
                                    (RightTab::GeneticCloud, egui_phosphor::regular::CLOUD, "CLOUD"),
                                    (RightTab::Notifications, egui_phosphor::regular::BRAIN, "AI"),
                                    (RightTab::Metrics, egui_phosphor::regular::CHART_BAR, "METRICS"),
                                ];

                                for (t_val, icon, label) in tabs {
                                    let is_sel = self.active_right_tab == Some(t_val);
                                    let text = egui::RichText::new(format!("{} {}", icon, label))
                                        .size(self.theme.type_caption)
                                        .strong();
                                    if ui.selectable_label(is_sel, text).clicked() {
                                        self.active_right_tab = Some(t_val);
                                    }
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(egui_phosphor::regular::X).clicked() {
                                        self.active_right_tab = None;
                                    }
                                });
                            });
                        });

                    ui.separator();
                    ui.add_space(self.theme.space_sm);

                    match tab {
                        RightTab::Library => views::library::render(self, ui),
                        RightTab::Store => views::store::render(self, ui),
                        RightTab::GeneticCloud => views::genetic_cloud::render(self, ui),
                        RightTab::Notifications => views::notifications::render(self, ui),
                        RightTab::Metrics => views::metrics::render(self, ui),
                    }
                });
        }
    }

    fn render_bottom_bar(&mut self, ctx: &egui::Context, telemetry: &Option<Telemetry>, id_prefix: &str) {
        egui::TopBottomPanel::bottom(format!("{}_bottom_bar", id_prefix)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("nullherz Alpha").size(10.0).color(self.theme.text_disabled));
                ui.separator();

                if let Some(t) = telemetry {
                    ui.label(format!("BPM: {:.1}", t.bpm));
                    ui.separator();
                    ui.label(format!("POS: {:.2}", t.beat_position));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tabs = [
                        (RightTab::Library, egui_phosphor::regular::FOLDER_OPEN, "LIBRARY"),
                        (RightTab::Store, egui_phosphor::regular::SHOPPING_BAG, "STORE"),
                        (RightTab::GeneticCloud, egui_phosphor::regular::CLOUD, "GENETIC CLOUD"),
                        (RightTab::Notifications, egui_phosphor::regular::BRAIN, "AI & INSIGHTS"),
                        (RightTab::Metrics, egui_phosphor::regular::CHART_BAR, "METRICS"),
                    ];

                    for (tab, icon, label) in tabs.into_iter().rev() {
                        let is_selected = self.active_right_tab == Some(tab);
                        let size = egui::vec2(36.0, 36.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        if response.clicked() {
                            if self.active_right_tab == Some(tab) {
                                self.active_right_tab = None;
                            } else {
                                self.active_right_tab = Some(tab);
                            }
                            ui.ctx().request_repaint();
                        }

                        if is_selected {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_sm,
                                self.theme.accent.linear_multiply(0.12),
                            );
                            let accent_bar = egui::Rect::from_min_max(
                                rect.left_bottom() + egui::vec2(6.0, -3.0),
                                rect.right_bottom() + egui::vec2(-6.0, -1.0),
                            );
                            ui.painter().rect_filled(accent_bar, 1.0, self.theme.accent);
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                rect.shrink(1.0),
                                self.theme.radius_sm,
                                self.theme.bg_med.linear_multiply(0.4),
                            );
                        }

                        let icon_color = if is_selected {
                            self.theme.accent
                        } else if response.hovered() {
                            self.theme.text_primary
                        } else {
                            self.theme.text_secondary
                        };

                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icon,
                            egui::FontId::proportional(16.0),
                            icon_color,
                        );

                        response.on_hover_text(label);
                    }

                    ui.separator();
                    ui.toggle_value(&mut self.broadcast.is_streaming, format!("{} BROADCAST", egui_phosphor::regular::BROADCAST));
                });
            });
        });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let current_time = ctx.input(|i| i.time);
        let wants_kb = ctx.wants_keyboard_input();

        if self.settings.shortcuts_enabled {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Space) && !wants_kb {
                    if self.decks.global_playing {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop));
                        self.decks.global_playing = false;
                    } else {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play));
                        self.decks.global_playing = true;
                    }
                }
                if i.key_pressed(egui::Key::Z) && i.modifiers.command {
                    if i.modifiers.shift {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Redo));
                    } else {
                        let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Undo));
                    }
                } else if i.key_pressed(egui::Key::Y) && i.modifiers.command {
                    let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Redo));
                }
                if i.key_pressed(egui::Key::S) && i.modifiers.command {
                    let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
                    let ports = "Pioneer DDJ-400,Generic MIDI Keyboard".to_string();
                    let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts({
                        let mut b = [0u8; 128];
                        let bytes = ports.as_bytes();
                        b[..bytes.len().min(128)].copy_from_slice(&bytes[..bytes.len().min(128)]);
                        b
                    })));
                    self.settings.config_saved_time = Some(current_time);
                    self.settings.autosave_triggered = None;
                    self.save_preferences();
                }
                if i.key_pressed(egui::Key::Num1) { self.active_view = View::Player; }
                if i.key_pressed(egui::Key::Num2) { self.active_view = View::Console; }
                if i.key_pressed(egui::Key::Num3) { self.active_view = View::Composer; }
                if i.key_pressed(egui::Key::Num4) { self.active_view = View::Editor; }
                if i.key_pressed(egui::Key::Num5) { self.active_view = View::Sampler; }
                if i.key_pressed(egui::Key::Num6) { self.active_view = View::Breeder; }
                if i.key_pressed(egui::Key::Num7) { self.active_view = View::Broadcast; }
                if i.key_pressed(egui::Key::Num8) { self.active_view = View::Topology; }
                if i.key_pressed(egui::Key::Num9) { self.active_view = View::Account; }
            });
        }

        // QWERTY Virtual MIDI Keyboard
        if self.settings.qwerty_midi_enabled && !wants_kb {
            let mut events_to_send = Vec::new();
            let base_note = (60i16 + (self.settings.qwerty_octave as i16) * 12).clamp(0, 127) as u8;

            ctx.input(|i| {
                // Octave adjustment
                if i.key_pressed(egui::Key::OpenBracket) || i.key_pressed(egui::Key::Minus) {
                    self.settings.qwerty_octave = (self.settings.qwerty_octave - 1).max(-2);
                }
                if i.key_pressed(egui::Key::CloseBracket) || i.key_pressed(egui::Key::Equals) {
                    self.settings.qwerty_octave = (self.settings.qwerty_octave + 1).min(2);
                }

                let piano_keys = [
                    egui::Key::Z, egui::Key::S, egui::Key::X, egui::Key::D, egui::Key::C, egui::Key::V, egui::Key::G,
                    egui::Key::B, egui::Key::H, egui::Key::N, egui::Key::J, egui::Key::M, egui::Key::Comma, egui::Key::L,
                    egui::Key::Period, egui::Key::Q, egui::Key::Num2, egui::Key::W, egui::Key::Num3, egui::Key::E,
                    egui::Key::R, egui::Key::Num5, egui::Key::T, egui::Key::Num6, egui::Key::Y, egui::Key::Num7,
                    egui::Key::U, egui::Key::I
                ];

                for key in piano_keys {
                    if i.key_pressed(key) && !i.modifiers.command && !i.modifiers.ctrl && !i.modifiers.alt {
                        if let Some(semi) = key_to_semitone(key) {
                            let note = (base_note + semi).min(127);
                            let event = nullherz_traits::MidiEvent {
                                timestamp_samples: 0,
                                status: 0x90, // Note On
                                data1: note,
                                data2: 100, // Velocity
                                _pad: 0,
                            };
                            events_to_send.push((key, event));
                        }
                    }
                }

                let held_keys_clone: Vec<egui::Key> = self.settings.qwerty_held_keys.iter().copied().collect();
                for key in held_keys_clone {
                    if i.key_released(key) || !i.key_down(key) {
                        if let Some(semi) = key_to_semitone(key) {
                            let note = (base_note + semi).min(127);
                            let event = nullherz_traits::MidiEvent {
                                timestamp_samples: 0,
                                status: 0x80, // Note Off
                                data1: note,
                                data2: 0,
                                _pad: 0,
                            };
                            events_to_send.push((key, event));
                        }
                    }
                }
            });

            for (key, event) in events_to_send {
                if event.status == 0x90 {
                    self.settings.qwerty_held_keys.insert(key);
                } else if event.status == 0x80 {
                    self.settings.qwerty_held_keys.remove(&key);
                }

                let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::InjectMidi(event)));

                self.settings.recent_midi_events.push_back(event);
                while self.settings.recent_midi_events.len() > 30 {
                    self.settings.recent_midi_events.pop_front();
                }
            }
        }
    }

    fn handle_autosave(&mut self, current_time: f64) {
        if self.settings.autosave_enabled {
            let interval_secs = (self.settings.autosave_interval_mins as f64) * 60.0;
            if current_time - self.settings.last_saved_time >= interval_secs {
                let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
                let ports = "Pioneer DDJ-400,Generic MIDI Keyboard".to_string();
                let _ = self.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMidiPorts({
                    let mut b = [0u8; 128];
                    let bytes = ports.as_bytes();
                    b[..bytes.len().min(128)].copy_from_slice(&bytes[..bytes.len().min(128)]);
                    b
                })));
                self.settings.last_saved_time = current_time;
                self.settings.config_saved_time = Some(current_time);
                self.settings.autosave_triggered = Some(current_time);
                self.save_preferences();
            }
        }
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let current_time = ctx.input(|i| i.time);

        // --- Background Library Loader Polling ---
        if let Some(ref rx) = self.library.bg_library_loader {
            if let Ok(payload) = rx.try_recv() {
                self.library.cached_library_raw = payload.all_tracks;
                self.library.cached_library = payload.crate_tracks;
                self.library.cached_crates = payload.crates;
                self.library.cached_smart_crates = payload.smart_crates;
                self.library.library_needs_refresh = false;
                self.library.bg_library_loader = None;
                self.library.last_refresh_time = current_time;
                self.decks.cached_tracks = std::array::from_fn(|_| None);
            }
        } else if self.library.library_needs_refresh {
            self.trigger_library_refresh();
        } else if current_time - self.library.last_refresh_time > 2.0 {
            // Periodic re-poll: the folder monitor analyzes tracks AFTER the
            // first refresh completed; a one-shot load left the panel
            // permanently empty on a fresh library ("can't load the track").
            self.trigger_library_refresh();
        }

        // Initialize last_saved_time on first loop run if it's 0.0
        if self.settings.last_saved_time == 0.0 {
            self.settings.last_saved_time = current_time;
        }

        // Refresh the per-deck track cache only when the loaded id changes;
        // views read the cache instead of hitting redb every frame.
        for i in 0..4 {
            let want = self.decks.now_playing[i];
            let have = self.decks.cached_tracks[i].as_ref().map(|t| t.id);
            if want != have {
                self.decks.cached_tracks[i] = want.and_then(|id| self.get_cached_track(id));
            }
        }

        // Sync cached inspected track when selected_library_track changes
        let selected_id = self.library.selected_library_track;
        let current_inspected_id = self.library.cached_inspected_track.as_ref().map(|t| t.id);
        if selected_id != current_inspected_id {
            self.library.cached_inspected_track = selected_id.and_then(|id| self.get_cached_track(id));
        }

        // --- Keyboard Shortcuts ---
        self.handle_shortcuts(ctx);

        // --- Autosave Background Job ---
        self.handle_autosave(current_time);

        let is_focused = ctx.input(|i| i.focused);
        let has_detached = !self.detached_views.is_empty() || self.viz.detached_channel.is_some();

        // Background Throttling: Skip telemetry processing if unfocused (and no detached windows open) and updated recently (<100ms)
        let should_process = is_focused || has_detached || (current_time - self.last_update_time) > 0.1;

        let telemetry = if should_process {
            self.last_update_time = current_time;
            *self.last_telemetry.lock()
        } else {
            None
        };

        // Update Damping (Liquid Asymmetrical Damping: Fast Attack, Slow Decay)
        if let Some(ref t) = telemetry {
            // Synchronize Master Deck from Telemetry
            self.decks.master_deck = Some((t.active_master_deck as u8 - b'A') as usize);

            let d = self.viz.visualizer_damping.clamp(0.01, 1.0);
            let decay = d * 0.5; // Slower decay for "liquid" feel

            // Optimized damping using Lerp formula: current + (target - current) * alpha
            for i in 0..128 {
                let target_spec = t.spectrum[i];
                let alpha = if target_spec > self.viz.damped_spectrum[i] { d } else { decay };
                self.viz.damped_spectrum[i] += (target_spec - self.viz.damped_spectrum[i]) * alpha;

                let target_gonio = t.goniometer_pts[i];
                let alpha_g = if target_gonio.abs() > self.viz.damped_goniometer[i].abs() { d } else { decay };
                self.viz.damped_goniometer[i] += (target_gonio - self.viz.damped_goniometer[i]) * alpha_g;
            }
            for i in 0..16 {
                let target_latent = t.dna_latent_space[i];
                let alpha_l = if target_latent.abs() > self.viz.damped_latent[i].abs() { d } else { decay };
                self.viz.damped_latent[i] += (target_latent - self.viz.damped_latent[i]) * alpha_l;
            }
            // Per-deck and master meters resolve their node indices from the
            // telemetry node map — NEVER from hardcoded indices. peak_levels
            // is indexed by GRAPH node id, and the bootstrap layout shifts
            // whenever a strip gains a stage; the old `peak_levels[0..4]`
            // read deck A's first four strip nodes, so every deck's meter
            // mirrored deck A.
            for (i, deck) in ['a', 'b', 'c', 'd'].iter().enumerate() {
                let node = self
                    .topo.node_map.get(&format!("deck_{}_isolator", deck))
                    .or_else(|| self.topo.node_map.get(&format!("deck_{}_sampler", deck)))
                    .copied();
                let target_peak = match node {
                    Some(n) if (n as usize) < t.peak_levels.len() => t.peak_levels[n as usize],
                    _ => 0.0,
                };
                let alpha_p = if target_peak > self.viz.damped_peaks[i] { 1.0 } else { decay * 0.5 };
                self.viz.damped_peaks[i] += (target_peak - self.viz.damped_peaks[i]) * alpha_p;
            }
            // Truthful play state: a deck is playing iff its playhead moved
            // ACROSS TELEMETRY SNAPSHOTS. Gated on sample_counter: the UI
            // repaints faster than telemetry refreshes, and re-deriving from
            // the SAME cached snapshot compared a position against itself —
            // deck_playing flapped false mid-playback and the player view's
            // play/stop toggle sent PlayDeck when the user meant StopDeck.
            if t.sample_counter != self.viz.last_playstate_counter {
                self.viz.last_playstate_counter = t.sample_counter;
                crate::state::update_deck_playing(
                    &t.deck_positions,
                    &mut self.viz.last_deck_positions,
                    &mut self.viz.deck_still_snapshots,
                    &mut self.decks.deck_playing,
                );
            }

            for (i, name) in ["master_sum_l", "master_sum_r"].iter().enumerate() {
                let target_m_peak = match self.topo.node_map.get(*name).copied() {
                    Some(n) if (n as usize) < t.peak_levels.len() => t.peak_levels[n as usize],
                    _ => 0.0,
                };
                let alpha_mp = if target_m_peak > self.viz.damped_master_peaks[i] { 1.0 } else { decay * 0.5 };
                self.viz.damped_master_peaks[i] += (target_m_peak - self.viz.damped_master_peaks[i]) * alpha_mp;
            }

            // Sync node map from telemetry.
            // Iterate the array's real length rather than a literal — a hardcoded
            // 32 here would have kept reading only half the slots after the map
            // was widened, which looks exactly like the overflow it fixed.
            for (i, key_bytes) in t.node_map_keys.iter().enumerate() {
                if key_bytes[0] != 0 {
                    let name = String::from_utf8_lossy(key_bytes).trim_matches(char::from(0)).to_string();
                    self.topo.node_map.insert(name, t.node_map_values[i]);
                }
            }

            // Sync audio devices from telemetry
            let mut devs = Vec::new();
            for i in 0..16 {
                let dev_bytes = t.audio_devices[i].name;
                if dev_bytes[0] != 0 {
                    devs.push(String::from_utf8_lossy(&dev_bytes).trim_matches(char::from(0)).to_string());
                }
            }
            if !devs.is_empty() {
                self.settings.audio_devices = devs;
            }
        }

        // 1. Left Sidebar (Navigation Plane)
        let mut main_view = self.active_view;
        self.render_left_sidebar(ctx, &mut main_view, "main");
        self.active_view = main_view;

        // 2. Right Sidebar (Intelligence Plane - Collapsible)
        self.render_right_sidebar(ctx, "main");

        // 3. Bottom Bar (Status & Global Controls)
        self.render_bottom_bar(ctx, &telemetry, "main");

        // --- Render Single Detached Visual Surface Window ---
        if let Some(c_idx) = self.viz.detached_channel {
            let channel_name = self.viz.channels.get(c_idx).map(|c| c.name.clone()).unwrap_or_else(|| "VISUAL SURFACE".to_string());
            let viewport_id = egui::ViewportId::from_hash_of(&format!("visual_surface_{}", c_idx));
            let viewport_builder = egui::ViewportBuilder::default()
                .with_title(format!("nullherz Visual Surface — {}", channel_name))
                .with_inner_size([1280.0, 800.0]);

            let mut close_visual_window = false;
            ctx.show_viewport_immediate(viewport_id, viewport_builder, |v_ctx, _class| {
                if v_ctx.input(|i| i.viewport().close_requested()) {
                    close_visual_window = true;
                }

                let v_focused = v_ctx.input(|i| i.focused);

                egui::CentralPanel::default().show(v_ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(egui::RichText::new(format!("FULLSCREEN SURFACE — {}", channel_name)).strong().color(self.theme.accent));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let is_fullscreen = v_ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                            let fs_icon = if is_fullscreen {
                                egui_phosphor::regular::ARROWS_IN_SIMPLE
                            } else {
                                egui_phosphor::regular::ARROWS_OUT_SIMPLE
                            };
                            let fs_tooltip = if is_fullscreen { "Exit Fullscreen" } else { "Toggle Fullscreen" };
                            if ui.button(fs_icon).on_hover_text(fs_tooltip).clicked() {
                                v_ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
                            }
                            if ui.button(format!("{} Close Window", egui_phosphor::regular::X)).clicked() {
                                close_visual_window = true;
                            }
                        });
                    });
                    ui.separator();
                    self.render_detached_interactive_surface(c_idx, v_ctx, ui, &telemetry);
                });

                let v_cadence = if v_focused || is_focused {
                    std::time::Duration::from_millis(33)
                } else {
                    std::time::Duration::from_millis(200)
                };
                v_ctx.request_repaint_after(v_cadence);
            });

            if close_visual_window {
                self.viz.detached_channel = None;
            }
        }

        // --- Render Detached Windows (Multi-Viewport System for Views) ---
        let detached_list: Vec<View> = self.detached_views.iter().copied().collect();
        for detached_view in detached_list {
            let view_name = view_to_string(detached_view);
            let viewport_id = egui::ViewportId::from_hash_of(&view_name);
            let viewport_builder = egui::ViewportBuilder::default()
                .with_title(format!("nullherz Studio — {}", view_name))
                .with_inner_size([1100.0, 750.0]);

            let mut close_detached = false;
            let mut current_detached_view = detached_view;
            ctx.show_viewport_immediate(viewport_id, viewport_builder, |v_ctx, _class| {
                if v_ctx.input(|i| i.viewport().close_requested()) {
                    close_detached = true;
                }

                let v_focused = v_ctx.input(|i| i.focused);

                // Render left & right sidebars and bottom bar for detached viewport
                self.render_left_sidebar(v_ctx, &mut current_detached_view, &view_name);
                self.render_right_sidebar(v_ctx, &view_name);
                self.render_bottom_bar(v_ctx, &telemetry, &view_name);

                egui::CentralPanel::default().show(v_ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(egui::RichText::new(view_to_string(current_detached_view)).strong().color(self.theme.accent));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let is_fullscreen = v_ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                            let fs_icon = if is_fullscreen {
                                egui_phosphor::regular::ARROWS_IN_SIMPLE
                            } else {
                                egui_phosphor::regular::ARROWS_OUT_SIMPLE
                            };
                            let fs_tooltip = if is_fullscreen { "Exit Fullscreen" } else { "Fullscreen" };
                            if ui.button(fs_icon).on_hover_text(fs_tooltip).clicked() {
                                v_ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
                            }
                            if ui.button(format!("{} Re-attach to Main", egui_phosphor::regular::ARROWS_IN)).clicked() {
                                close_detached = true;
                            }
                        });
                    });
                    ui.separator();
                    self.render_view_content(current_detached_view, ui, &telemetry);
                });

                // Continuous repaint for detached viewport at bounded cadence
                let v_cadence = if v_focused || is_focused {
                    std::time::Duration::from_millis(33)
                } else {
                    std::time::Duration::from_millis(200)
                };
                v_ctx.request_repaint_after(v_cadence);
            });

            if current_detached_view != detached_view {
                self.detached_views.remove(&detached_view);
                self.detached_views.insert(current_detached_view);
            }

            if close_detached {
                self.detached_views.remove(&detached_view);
            }
        }

        // 4. Central Panel (Execution Plane)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new(view_to_string(self.active_view)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                    let fs_icon = if is_fullscreen {
                        egui_phosphor::regular::ARROWS_IN_SIMPLE
                    } else {
                        egui_phosphor::regular::ARROWS_OUT_SIMPLE
                    };
                    let fs_tooltip = if is_fullscreen { "Exit Fullscreen" } else { "Fullscreen" };
                    if ui.button(fs_icon).on_hover_text(fs_tooltip).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
                    }

                    let is_detached = self.detached_views.contains(&self.active_view);
                    let btn_label = if is_detached {
                        format!("{} Re-attach Window", egui_phosphor::regular::ARROWS_IN)
                    } else {
                        format!("{} Detach Window", egui_phosphor::regular::ARROW_SQUARE_OUT)
                    };
                    if ui.button(btn_label).clicked() {
                        if is_detached {
                            self.detached_views.remove(&self.active_view);
                        } else {
                            self.detached_views.insert(self.active_view);
                        }
                    }
                });
            });
            ui.separator();

            if self.detached_views.contains(&self.active_view) {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.label(egui::RichText::new(format!("{} View Detached", egui_phosphor::regular::ARROW_SQUARE_OUT)).size(20.0).color(self.theme.text_secondary));
                    ui.label(format!("This view is open in an independent window (nullherz Studio — {}).", view_to_string(self.active_view)));
                    ui.add_space(20.0);
                    if ui.button("Re-attach to Main Window").clicked() {
                        self.detached_views.remove(&self.active_view);
                    }
                });
            } else {
                let view = self.active_view;
                self.render_view_content(view, ui, &telemetry);
            }
        });

        // Continuous repaint at a bounded cadence. egui only redraws on input
        // events by default, so without this the meters, waveforms and
        // transport FREEZE whenever the mouse is still — which reads as
        // "the UI lags". 30 Hz focused is smooth for meters without burning
        // a core; 5 Hz keeps background windows alive but cheap.
        let cadence = if is_focused {
            std::time::Duration::from_millis(33)
        } else {
            std::time::Duration::from_millis(200)
        };
        ctx.request_repaint_after(cadence);
    }
}

fn create_app(cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
    let graph = GraphJson { nodes: vec![], edges: vec![], node_assignments: nullherz_traits::NodeAssignmentArray::default() };
    let mut app = InspectorApp::new(graph, cc);

    if let Some(render_state) = &cc.wgpu_render_state {
        // eframe already manages WGPU.
        // We'll mark the renderer as active to enable the GPU-accelerated UI paths.
        app.wgpu_renderer = Some(Arc::new(Mutex::new(nullherz_ui_hal::render::wgpu_backend::WgpuRenderer {
            device: render_state.device.clone(),
            queue: render_state.queue.clone(),
            surface: None,
            config: None,
        })));

        let wf_renderer = nullherz_ui_hal::render::waveform_renderer::WaveformRenderer::new(
            &render_state.device,
            render_state.target_format,
            8192
        );
        app.waveform_renderer = Some(Arc::new(Mutex::new(wf_renderer)));

        let mut deck_wfs = [None, None, None, None];
        for wf_slot in &mut deck_wfs {
            let wf = nullherz_ui_hal::render::waveform_renderer::WaveformRenderer::new(
                &render_state.device,
                render_state.target_format,
                1024
            );
            *wf_slot = Some(Arc::new(Mutex::new(wf)));
        }
        app.deck_waveform_renderers = deck_wfs;
    }

    Box::new(app)
}

fn main() -> eframe::Result<()> {
    // 1. Detect if Wgpu is supported on this system before spawning anything.
    let has_wgpu = {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        pollster::block_on(async {
            instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: None,
            }).await.is_some()
        })
    };

    let renderer = if has_wgpu {
        println!("Wgpu adapter detected. Launching with Wgpu renderer...");
        eframe::Renderer::Wgpu
    } else {
        println!("No compatible Wgpu adapter found. Launching with Glow (OpenGL) renderer...");
        eframe::Renderer::Glow
    };

    let native_options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default().with_fullscreen(true),
        ..Default::default()
    };

    eframe::run_native(
        "nullherz Studio",
        native_options,
        Box::new(|cc| create_app(cc)),
    )
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct PersistedPreferences {
    pub restore_last_session: bool,
    pub default_view_on_launch: String,
    pub autosave_enabled: bool,
    pub autosave_interval_mins: u32,
    pub shortcuts_enabled: bool,
    pub theme_accent: Option<[u8; 4]>,
    pub theme_success: Option<[u8; 4]>,
    pub theme_danger: Option<[u8; 4]>,
}

fn view_to_string(view: View) -> String {
    match view {
        View::Player => "Player".to_string(),
        View::Console => "Console".to_string(),
        View::Mixer => "Mixer".to_string(),
        View::Composer => "Composer".to_string(),
        View::Library => "Library".to_string(),
        View::Editor => "Editor".to_string(),
        View::Sampler => "Sampler".to_string(),
        View::Breeder => "Breeder".to_string(),
        View::Broadcast => "Broadcast".to_string(),
        View::Topology => "Topology".to_string(),
        View::Account => "Account".to_string(),
        View::Settings => "Settings".to_string(),
        View::Store => "Store".to_string(),
        View::Mastering => "Mastering".to_string(),
        View::Modulation => "Modulation".to_string(),
        View::Visuals => "Visuals".to_string(),
        _ => "Console".to_string(),
    }
}

fn string_to_view(s: &str) -> View {
    match s {
        "Player" => View::Player,
        "Console" => View::Console,
        "Mixer" => View::Mixer,
        "Composer" => View::Composer,
        "Library" => View::Library,
        "Editor" => View::Editor,
        "Sampler" => View::Sampler,
        "Breeder" => View::Breeder,
        "Broadcast" => View::Broadcast,
        "Topology" => View::Topology,
        "Account" => View::Account,
        "Settings" => View::Settings,
        "Store" => View::Store,
        "Mastering" => View::Mastering,
        "Modulation" => View::Modulation,
        "Visuals" => View::Visuals,
        _ => View::Console,
    }
}

impl Drop for InspectorApp {
    fn drop(&mut self) {
        self.save_preferences();
    }
}

#[derive(Clone)]
pub struct SharedLibraryDb(pub Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>);

impl SharedLibraryDb {
    pub fn list_smart_crates(&self) -> Result<Vec<nullherz_dna::SmartCrateDefinition>, Box<dyn std::error::Error>> {
        self.0.lock().list_smart_crates()
    }
    pub fn save_smart_crate(&self, def: &nullherz_dna::SmartCrateDefinition) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().save_smart_crate(def)
    }
}

impl nullherz_dna::GeneticLibrary for SharedLibraryDb {
    fn get_track(&self, id: u64) -> Result<Option<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().get_track(id)
    }
    fn list_tracks(&self) -> Result<Vec<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().list_tracks()
    }
    fn save_track(&self, track: &nullherz_dna::LibraryTrack) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().save_track(track)
    }
    fn add_to_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().add_to_crate(crate_name, track_id)
    }
    fn remove_from_crate(&self, crate_name: &str, track_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().remove_from_crate(crate_name, track_id)
    }
    fn list_crates(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        self.0.lock().list_crates()
    }
    fn get_tracks_in_crate(&self, crate_name: &str) -> Result<Vec<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().get_tracks_in_crate(crate_name)
    }
    fn query_tracks(&self, genre: Option<&str>, min_bpm: Option<f32>, max_bpm: Option<f32>, root_key: Option<f32>) -> Result<Vec<nullherz_dna::LibraryTrack>, Box<dyn std::error::Error>> {
        self.0.lock().query_tracks(genre, min_bpm, max_bpm, root_key)
    }
    fn suggest_matches(&self, target_dna: &nullherz_traits::SoundDNA, limit: usize) -> Result<Vec<(u64, f32)>, Box<dyn std::error::Error>> {
        self.0.lock().suggest_matches(target_dna, limit)
    }
    fn remove_track(&self, id: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.0.lock().remove_track(id)
    }
}

pub fn start_in_process_conductor(
    cmd_rx: mpsc::Receiver<Command>,
    last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    db_arc: Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>,
    backend_override: Option<nullherz_backends::AudioBackendType>,
) -> (std::thread::JoinHandle<()>, Arc<Mutex<nullherz_conductor::Conductor>>) {
    let conductor = nullherz_conductor::Conductor::with_library(db_arc);
    let conductor_arc = Arc::new(Mutex::new(conductor));
    let conductor_clone = conductor_arc.clone();

    let join_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");
        let _guard = rt.enter();

        // Perform setup inside the Tokio context!
        let mut context = {
            let mut cond = conductor_clone.lock();
            let _ = cond.load_system_config();
            let context = cond.setup_engine();

            let mut session_restored = false;
            let restore_enabled = std::fs::read_to_string("preferences.json")
                .ok()
                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                .and_then(|prefs| prefs.get("restore_last_session").and_then(|v| v.as_bool()))
                == Some(true);

            if restore_enabled && std::path::Path::new("autosave.json").exists() && cond.load_project("autosave.json").is_ok() {
                session_restored = true;
                println!("Conductor: Last session restored successfully from autosave.json.");
            }

            if !session_restored {
                // Bootstrapping 4-Channel DJ Mixer (on the conductor's own
                // MixerManager so deck_mappings resolve at runtime)...
                cond.bootstrap_4channel_mixer();
            }

            if let Some(worker) = cond.analysis_worker.take() {
                worker.start();
            }

            // Library auto-discovery on startup is DISABLED: scanning the
            // tracks folder automatically decoded every file into the in-memory
            // registry at boot, which on a library of large files spiked memory
            // and froze the app soon after startup. The folder monitor is left
            // in place (NOT taken) so the Library view's "scan folder" button
            // (ResourceCommand::ScanFolder) can populate the library on demand —
            // and keeping it here is what makes that manual command work at all,
            // since `take()` used to move the monitor out of the conductor.

            cond.sidecar_discovery.start_watcher();

            // Start backend (override wins over system_config.json; tests use Mock)
            let mut backend_type = nullherz_backends::AudioBackendType::Alsa;
            if let Some(override_type) = backend_override {
                backend_type = override_type;
            } else {
                let config_path = "system_config.json";
                if std::path::Path::new(config_path).exists()
                    && let Ok(content) = std::fs::read_to_string(config_path)
                        && let Ok(config) = serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&content) {
                            backend_type = match config.audio_backend.to_lowercase().as_str() {
                                "alsa" => nullherz_backends::AudioBackendType::Alsa,
                                "pipewire" => nullherz_backends::AudioBackendType::Pipewire,
                                "jack" => nullherz_backends::AudioBackendType::Jack,
                                "threaded" => nullherz_backends::AudioBackendType::Threaded,
                                "mock" => nullherz_backends::AudioBackendType::Mock,
                                _ => nullherz_backends::AudioBackendType::Alsa,
                            };
                        }
            }

            // Try starting the preferred backend. If it fails, fallback to Threaded.
            if let Err(e) = cond.start_backend(backend_type) {
                eprintln!(
                    "Failed to start audio backend {:?}: {}. Attempting fallback to Threaded backend...",
                    backend_type, e
                );
                if let Err(fallback_err) = cond.start_backend(nullherz_backends::AudioBackendType::Threaded) {
                    eprintln!("CRITICAL: Failed to start fallback Threaded backend: {}", fallback_err);
                }
            }
            context
        };

        let mut ticker = std::time::Instant::now();
        loop {
            let mut disconnected = false;
            // Scope for locking conductor
            {
                let mut cond = conductor_clone.lock();

                // 1. Process any incoming commands
                loop {
                    match cmd_rx.try_recv() {
                        Ok(cmd) => {
                            cond.apply_mixer_commands(vec![cmd]);
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }

                if !disconnected {
                    // 2. Tick conductor
                    cond.tick();

                    // 3. Process telemetry
                    while let Some(mut tel) = context.telemetry_consumer.pop() {
                        cond.update_timeline(&mut tel);
                        for i in 0..4 {
                            let deck_char = (b'A' + i as u8) as char;
                            let sample_id = cond.mixer_manager.deck_samples.get(&deck_char).copied().unwrap_or(0);
                            if cond.hydration_pending.contains(&sample_id) {
                                tel.hydration_pending[i] = sample_id;
                                tel.hydration_progress[i] = cond.hydration_progress.lock().get(&sample_id).copied().unwrap_or(0.0);
                            } else {
                                tel.hydration_pending[i] = 0;
                                tel.hydration_progress[i] = 1.0;
                            }
                        }
                        *last_telemetry.lock() = Some(tel);
                    }
                }
            }

            if disconnected {
                break;
            }

            let elapsed = ticker.elapsed();
            if elapsed < std::time::Duration::from_millis(16) {
                std::thread::sleep(std::time::Duration::from_millis(16) - elapsed);
            }
            ticker = std::time::Instant::now();
        }
    });

    (join_handle, conductor_arc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Polls the conductor until `active_master_deck` matches `expected`, panicking after
    /// `timeout`. The conductor thread boots the engine before its command-drain loop starts,
    /// so a fixed sleep races setup; polling makes the test independent of boot time.
    fn wait_for_master_deck(
        conductor_arc: &Arc<Mutex<nullherz_conductor::Conductor>>,
        expected: char,
        timeout: std::time::Duration,
    ) {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            {
                let cond = conductor_arc.lock();
                if cond.mixer_manager.active_master_deck == expected {
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    panic!(
                        "Timed out waiting for active_master_deck == '{}' (still '{}')",
                        expected, cond.mixer_manager.active_master_deck
                    );
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn test_multi_window_detached_views() {
        let (cmd_tx, _cmd_rx) = mpsc::channel::<Command>();
        let raw_db = nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase");
        let db_arc = Arc::new(parking_lot::Mutex::new(raw_db));

        let mut app = InspectorApp {
            graph: GraphJson { nodes: vec![], edges: vec![], node_assignments: Default::default() },
            command_sender: cmd_tx,
            last_telemetry: Arc::new(Mutex::new(None)),
            active_view: View::Composer,
            detached_views: std::collections::HashSet::new(),
            mixer: Default::default(),
            decks: Default::default(),
            library: Default::default(),
            store: Default::default(),
            composer: Default::default(),
            sampler: Default::default(),
            editor: Default::default(),
            broadcast: Default::default(),
            settings: Default::default(),
            viz: Default::default(),
            topo: Default::default(),
            library_db: SharedLibraryDb(db_arc),
            active_right_tab: None,
            breeding_view: views::breeder::BreederView::new(),
            wgpu_renderer: None,
            waveform_renderer: None,
            deck_waveform_renderers: [None, None, None, None],
            discovered_sidecars: vec![],
            p2p_sync_success_toast: None,
            export_passport_success_toast: None,
            export_passport_error_toast: None,
            theme: nullherz_ui_hal::Theme::default(),
            last_update_time: 0.0,
            _conductor_thread: None,
        };

        assert!(app.detached_views.is_empty());

        // Detach Composer, Console and Mastering views
        app.detached_views.insert(View::Composer);
        app.detached_views.insert(View::Console);
        app.detached_views.insert(View::Mastering);

        assert_eq!(app.detached_views.len(), 3);
        assert!(app.detached_views.contains(&View::Composer));
        assert!(app.detached_views.contains(&View::Console));
        assert!(app.detached_views.contains(&View::Mastering));

        // Re-attach Composer view
        app.detached_views.remove(&View::Composer);
        assert_eq!(app.detached_views.len(), 2);
        assert!(!app.detached_views.contains(&View::Composer));
    }

    #[test]
    fn test_detached_window_sidebar_and_fullscreen_controls() {
        let (cmd_tx, _cmd_rx) = mpsc::channel::<Command>();
        let raw_db = nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase");
        let db_arc = Arc::new(parking_lot::Mutex::new(raw_db));

        let mut app = InspectorApp {
            graph: GraphJson { nodes: vec![], edges: vec![], node_assignments: Default::default() },
            command_sender: cmd_tx,
            last_telemetry: Arc::new(Mutex::new(None)),
            active_view: View::Console,
            detached_views: std::collections::HashSet::new(),
            mixer: Default::default(),
            decks: Default::default(),
            library: Default::default(),
            store: Default::default(),
            composer: Default::default(),
            sampler: Default::default(),
            editor: Default::default(),
            broadcast: Default::default(),
            settings: Default::default(),
            viz: Default::default(),
            topo: Default::default(),
            library_db: SharedLibraryDb(db_arc),
            active_right_tab: Some(RightTab::Library),
            breeding_view: views::breeder::BreederView::new(),
            wgpu_renderer: None,
            waveform_renderer: None,
            deck_waveform_renderers: [None, None, None, None],
            discovered_sidecars: vec![],
            p2p_sync_success_toast: None,
            export_passport_success_toast: None,
            export_passport_error_toast: None,
            theme: nullherz_ui_hal::Theme::default(),
            last_update_time: 0.0,
            _conductor_thread: None,
        };

        app.detached_views.insert(View::Visuals);
        assert!(app.detached_views.contains(&View::Visuals));

        // Test changing view inside detached window state
        let initial_detached_view = View::Visuals;
        let new_detached_view = View::Mixer;
        if new_detached_view != initial_detached_view {
            app.detached_views.remove(&initial_detached_view);
            app.detached_views.insert(new_detached_view);
        }

        assert!(!app.detached_views.contains(&View::Visuals));
        assert!(app.detached_views.contains(&View::Mixer));
    }

    #[test]
    fn test_qwerty_key_to_semitone() {
        assert_eq!(key_to_semitone(egui::Key::Z), Some(0)); // C
        assert_eq!(key_to_semitone(egui::Key::S), Some(1)); // C#
        assert_eq!(key_to_semitone(egui::Key::X), Some(2)); // D
        assert_eq!(key_to_semitone(egui::Key::C), Some(4)); // E
        assert_eq!(key_to_semitone(egui::Key::Q), Some(12)); // C +1
        assert_eq!(key_to_semitone(egui::Key::I), Some(24)); // C +2
        assert_eq!(key_to_semitone(egui::Key::Num0), None);
    }

    #[test]
    fn test_inspector_command_routing_to_conductor() {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let last_telemetry = Arc::new(Mutex::new(None));

        // Create an in-memory transient LibraryDatabase for testing to avoid lock files
        let raw_db = nullherz_dna::LibraryDatabase::load(":memory:").expect("Failed to initialize transient LibraryDatabase");
        let db_arc = Arc::new(parking_lot::Mutex::new(raw_db));

        // Start the in-process conductor thread on the Mock backend: no audio hardware
        // dependency, and CI runners have no sound card.
        let (_conductor_thread, conductor_arc) = start_in_process_conductor(
            cmd_rx,
            last_telemetry,
            db_arc,
            Some(nullherz_backends::AudioBackendType::Mock),
        );

        // Initial state check
        {
            let cond = conductor_arc.lock();
            assert_eq!(cond.mixer_manager.active_master_deck, 'A'); // Starts as 'A' by default
        }

        // Send a Command to mutate conductor's state and wait for the drain loop to apply it
        cmd_tx.send(Command::Core(nullherz_traits::CoreCommand::SetMasterDeck('C'))).unwrap();
        wait_for_master_deck(&conductor_arc, 'C', std::time::Duration::from_secs(10));

        // Send another Command
        cmd_tx.send(Command::Core(nullherz_traits::CoreCommand::SetMasterDeck('D'))).unwrap();
        wait_for_master_deck(&conductor_arc, 'D', std::time::Duration::from_secs(10));
    }
}
