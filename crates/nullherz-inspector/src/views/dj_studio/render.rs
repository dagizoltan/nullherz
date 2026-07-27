use egui::{Ui, Color32, RichText, Frame, Margin, Rounding, Stroke, Vec2};
use crate::InspectorApp;
use nullherz_ui_hal::widgets;
use audio_core::Telemetry;

use super::{mixer, dna, transport, performance, waveform};

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    if app.decks.focused_deck >= 4 {
        app.decks.focused_deck = 0;
    }
    let theme = app.theme;

    // Compute total available height on the finite, constrained parent UI (outside of any ScrollArea)
    let total_h = ui.available_height().max(500.0);

    render_header(ui, telemetry, &theme);
    ui.add_space(theme.space_xs);

    // Waveform stack (top half of the window)
    // Consumes exactly 30% of the available central-panel height.
    // It sits OUTSIDE any ScrollArea so it remains persistent and visible at all times.
    let waveform_section_h = total_h * 0.30;
    let spacing_h = 2.0;
    let lane_h = (waveform_section_h - spacing_h * 3.0) / 4.0;

    ui.vertical(|ui| {
        for i in 0..4 {
            render_waveform_lane(app, ui, i, lane_h, telemetry);
            if i < 3 {
                ui.add_space(spacing_h);
            }
        }
    });

    ui.add_space(theme.space_xs);

    // Mixer section — sized to the space it has, NOT wrapped in a scroll area.
    //
    // It used to be `ScrollArea::both()` over five EQUAL columns laid out
    // A, B, MASTER, C, D. Two consequences, both bad on a control surface:
    // the strips did not fit, so the fader and transport ended up below a
    // scrollbar and unreachable mid-mix; and the master took a full deck's
    // width while pushing deck A and deck D about 1500 px apart.
    //
    // Now: four adjacent deck strips plus a narrow master rail, each given an
    // explicit width, and every strip laid out to the height available.
    let mixer_h = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space_xs;
        let gap = theme.space_xs;
        let master_w = 92.0;
        let deck_w = ((ui.available_width() - master_w - gap * 4.0) / 4.0).max(120.0);

        for i in 0..4 {
            ui.allocate_ui_with_layout(
                Vec2::new(deck_w, mixer_h),
                egui::Layout::top_down(egui::Align::Center),
                |ui| render_channel_strip(app, ui, i, telemetry, mixer_h),
            );
        }
        ui.allocate_ui_with_layout(
            Vec2::new(master_w, mixer_h),
            egui::Layout::top_down(egui::Align::Center),
            |ui| render_master_section(app, ui, telemetry),
        );
    });
}

/// Title, artist, live BPM and time, on the strip that controls them.
///
/// Track identity used to live only in the waveform lane at the top of the
/// window, ~700 px away from the controls acting on it, so a strip gave no
/// indication of what it was about to change.
fn render_strip_meta(
    app: &mut InspectorApp,
    ui: &mut Ui,
    i: usize,
    deck_color: Color32,
    is_focused: bool,
    telemetry: &Option<Telemetry>,
) {
    let theme = app.theme;
    let deck_id_label = (b'A' + i as u8) as char;
    let track = app.decks.cached_tracks[i].clone();

    ui.horizontal(|ui| {
        let label = RichText::new(format!("DECK {deck_id_label}"))
            .strong().size(theme.type_caption)
            .color(if is_focused { deck_color } else { theme.text_secondary });
        if ui.selectable_label(is_focused, label).clicked() {
            app.decks.focused_deck = i;
        }
        let is_master = app.decks.master_deck == Some(i);
        let m_color = if is_master { deck_color } else { theme.text_disabled };
        if ui.selectable_label(is_master, RichText::new("M").strong().size(theme.type_caption).color(m_color)).clicked() {
            app.decks.master_deck = Some(i);
            let _ = app.command_sender.send(nullherz_traits::Command::Core(
                nullherz_traits::CoreCommand::SetMasterDeck(deck_id_label)));
        }
    });

    // Char-safe truncation: byte slicing panics on a UTF-8 boundary.
    let truncate = |s: &str, max: usize| -> String {
        if s.chars().count() > max {
            format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
        } else {
            s.to_string()
        }
    };

    match track {
        Some(t) => {
            let loading = telemetry.as_ref().map(|tel| tel.hydration_pending[i] == t.id).unwrap_or(false);
            let title = truncate(&t.title, 22);
            ui.label(if loading {
                RichText::new(format!("⏳ {title}")).strong().size(theme.type_caption).color(theme.warning)
            } else {
                RichText::new(title).strong().size(theme.type_caption).color(theme.text_primary)
            });
            ui.label(RichText::new(truncate(&t.artist, 22)).size(theme.type_caption).color(theme.text_secondary));

            let rate = telemetry.as_ref().map(|tel| tel.deck_playback_rates[i]).unwrap_or(1.0);
            let sr = t.metadata.sample_rate.max(1) as f32;
            let pos = telemetry.as_ref().map(|tel| tel.deck_positions[i]).unwrap_or(0);
            let total = t.metadata.total_samples;
            let remaining = total.saturating_sub(pos);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:.1}", t.metadata.bpm * rate))
                    .monospace().strong().size(theme.type_caption).color(deck_color));
                ui.label(RichText::new(format_duration(pos, sr)).monospace().size(theme.type_caption).color(theme.text_secondary));
                ui.label(RichText::new(format!("-{}", format_duration(remaining, sr)))
                    .monospace().size(theme.type_caption).color(theme.text_disabled));
            });
        }
        None => {
            ui.label(RichText::new("no track").size(theme.type_caption).color(theme.text_disabled));
            ui.label(RichText::new(" ").size(theme.type_caption));
            ui.label(RichText::new("--.-  --:--").monospace().size(theme.type_caption).color(theme.text_disabled));
        }
    }
}

fn render_header(ui: &mut Ui, telemetry: &Option<Telemetry>, theme: &nullherz_ui_hal::Theme) {
    Frame::none()
        .fill(theme.bg_surface)
        .stroke(theme.border_stroke)
        .rounding(theme.radius_md)
        .inner_margin(Margin::symmetric(theme.space_md, theme.space_sm))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("NULLHERZ DJ CONSOLE").strong().color(theme.text_primary).size(theme.type_heading).extra_letter_spacing(1.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(t) = telemetry {
                        ui.label(RichText::new("BPM").size(theme.type_caption).color(theme.text_secondary));
                        ui.label(RichText::new(format!("{:.1}", t.bpm)).monospace().strong().color(theme.accent).size(theme.type_heading));
                    }
                });
            });
        });
}

pub fn format_duration(samples: u64, sample_rate: f32) -> String {
    if sample_rate <= 0.0 {
        return "0:00".to_string();
    }
    let total_seconds = samples as f64 / sample_rate as f64;
    let minutes = (total_seconds / 60.0).floor() as u32;
    let seconds = (total_seconds % 60.0).floor() as u32;
    format!("{}:{:02}", minutes, seconds)
}

pub fn render_time_display(ui: &mut egui::Ui, elapsed: &str, remaining: &str, accent_color: Color32, theme: &nullherz_ui_hal::Theme) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(elapsed).monospace().size(13.0).color(theme.text_secondary));
        ui.add_space(theme.space_sm);
        ui.label(egui::RichText::new(format!("-{}", remaining)).monospace().size(13.0).color(accent_color));
    });
}

fn render_waveform_lane(app: &mut InspectorApp, ui: &mut Ui, i: usize, lane_h: f32, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, i);
    let is_focused = app.decks.focused_deck == i;

    let bg_color = if is_focused {
        theme.bg_surface
    } else {
        theme.bg_canvas
    };

    let stroke_color = if is_focused {
        deck_color
    } else {
        theme.border
    };

    let border_thickness = if is_focused { 1.5 } else { 1.0 };
    let header_h = 22.0;

    let response = Frame::none()
        .fill(bg_color)
        .stroke(Stroke::new(border_thickness, stroke_color))
        .rounding(Rounding::same(theme.radius_sm))
        .inner_margin(Margin::same(0.0))
        .show(ui, |ui| {
            ui.set_height(lane_h);
            ui.vertical(|ui| {
                // Condensed Header Strip
                render_condensed_deck_header(app, ui, i, deck_color, is_focused, telemetry);

                // Waveform Zone
                let remaining_wf_h = (lane_h - header_h - 2.0 * border_thickness).max(10.0);
                waveform::render_deck_waveform_zone(app, ui, i, telemetry, deck_color, remaining_wf_h);
            });
        });

    // Draw left vertical accent bar inside the lane bounds
    let rect = response.response.rect;
    let bar_width = if is_focused { 4.0 } else { 1.5 };
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, rect.top() + 1.0),
        egui::pos2(rect.left() + 1.0 + bar_width, rect.bottom() - 1.0)
    );
    let bar_color = if is_focused { deck_color } else { theme.border };
    ui.painter().rect_filled(bar_rect, Rounding::ZERO, bar_color);

    // Clicking the lane's HEADER focuses the deck.
    //
    // Deliberately not the whole lane. A lane-wide `interact` sits on top of the
    // waveform and wins every pointer event over it, which is what made
    // scroll-to-scrub unreachable; extending that to drag would do the same to
    // scratching. The waveform owns its own gestures and focuses the deck on
    // click, so the header strip is all this needs to cover.
    let header_rect = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.max.x, (rect.min.y + header_h).min(rect.max.y)),
    );
    if ui.interact(header_rect, ui.id().with(format!("lane_click_{i}")), egui::Sense::click()).clicked() {
        app.decks.focused_deck = i;
    }
}

fn render_condensed_deck_header(app: &mut InspectorApp, ui: &mut Ui, i: usize, deck_color: Color32, is_focused: bool, telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    let deck_id_label = (b'A' + i as u8) as char;
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        // Left padding for the left accent bar
        ui.add_space(theme.space_sm);

        // Deck label
        let label_text = RichText::new(format!("DECK {}", deck_id_label)).strong().size(theme.type_caption).color(if is_focused { deck_color } else { theme.text_secondary });
        if ui.selectable_label(is_focused, label_text).clicked() {
            app.decks.focused_deck = i;
        }

        ui.add_space(theme.space_xs);

        // Master Deck Toggle ("M")
        let is_master = app.decks.master_deck == Some(i);
        let m_color = if is_master { deck_color } else { theme.text_disabled };
        if ui.selectable_label(is_master, RichText::new("M").strong().size(theme.type_caption).color(m_color)).clicked() {
             app.decks.master_deck = Some(i);
             let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetMasterDeck(deck_id_label)));
        }

        ui.add_space(theme.space_xs);

        // Sync toggle
        let is_sync = app.mixer.channel_sync[i];
        let sync_color = if is_sync { theme.accent } else { theme.text_disabled };
        if ui.selectable_label(is_sync, RichText::new("S").strong().size(theme.type_caption).color(sync_color)).clicked() {
            app.mixer.channel_sync[i] = !is_sync;
            // Wire it: S = the deck sampler's quantize/BPM-lock (param 2).
            // The toggle used to be a local bool that did nothing.
            if let Some(node) = app.get_node_id(&format!("deck_{}_sampler", (b'a' + i as u8) as char)) {
                let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: node as u64,
                    param_id: 2,
                    value: if app.mixer.channel_sync[i] { 1.0 } else { 0.0 },
                    ramp_duration_samples: 0,
                }));
            }
        }

        ui.add_space(theme.space_sm);

        // Track metadata block
        let track = app.decks.cached_tracks[i].clone();

        if let Some(ref t) = track {
            let is_loading = telemetry.as_ref().map(|tel| tel.hydration_pending[i] == t.id).unwrap_or(false);
            let progress = if is_loading {
                telemetry.as_ref().map(|tel| tel.hydration_progress[i]).unwrap_or(0.0)
            } else {
                1.0
            };

            // Track Title & Artist — char-safe truncation: byte-slicing
            // (&s[..18]) PANICS on a UTF-8 boundary, so any track title with
            // an accent or emoji at the wrong offset crashed the console.
            let truncate = |s: &str, max: usize| -> String {
                if s.chars().count() > max {
                    format!("{}...", s.chars().take(max.saturating_sub(2)).collect::<String>())
                } else {
                    s.to_string()
                }
            };
            let title_text = truncate(&t.title, 20);
            if is_loading {
                ui.label(RichText::new(format!("⏳ {}", title_text)).strong().size(theme.type_caption).color(theme.warning));
            } else {
                ui.label(RichText::new(title_text).strong().size(theme.type_caption).color(theme.text_primary));
            }

            let artist_text = if t.artist.chars().count() > 15 {
                format!("by {}...", t.artist.chars().take(13).collect::<String>())
            } else {
                format!("by {}", t.artist)
            };
            ui.label(RichText::new(artist_text).size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_sm);

            // Live BPM
            let playback_rate = telemetry.as_ref().map(|t| t.deck_playback_rates[i]).unwrap_or(1.0);
            let live_bpm = t.metadata.bpm * playback_rate;
            ui.label(RichText::new(format!("{:.1}", live_bpm)).monospace().strong().size(theme.type_caption).color(deck_color));
            ui.label(RichText::new("BPM").size(theme.type_caption).color(theme.text_secondary));

            ui.add_space(theme.space_sm);

            // Native track key & genre
            let mut meta_text = String::new();
            if let Some(key) = t.metadata.root_key {
                meta_text.push_str(&format!("K:{:.0} ", key));
            }
            if !t.genre.is_empty() {
                meta_text.push_str(&format!("G:{}", t.genre));
            }
            ui.label(RichText::new(meta_text).size(theme.type_caption).color(theme.text_secondary));

            // Time Display on the far right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(theme.space_xs); // right padding
                if is_loading {
                    ui.add(egui::ProgressBar::new(progress).desired_width(120.0).text(format!("LOADING {:.0}%", progress * 100.0)).fill(theme.accent));
                } else {
                    // The TRACK's rate, not the device's: `deck_positions` and
                    // `total_samples` both count source frames, so the device
                    // rate would misreport the time on any material whose rate
                    // differs from the output's.
                    let sample_rate = t.metadata.sample_rate.max(1) as f32;
                    let elapsed_samples = telemetry.as_ref().map(|t| t.deck_positions[i]).unwrap_or(0);
                    let total_samples = t.metadata.total_samples;

                    let elapsed_str = format_duration(elapsed_samples, sample_rate);
                    let remaining_samples = total_samples.saturating_sub(elapsed_samples);
                    let remaining_str = format_duration(remaining_samples, sample_rate);

                    render_time_display(ui, &elapsed_str, &remaining_str, deck_color, &theme);
                }
            });
        } else {
            ui.label(RichText::new("NO TRACK LOADED").monospace().color(theme.text_disabled).size(theme.type_caption));
        }
    });
}

/// Fixed vertical costs in a channel strip, in logical pixels.
///
/// Named so the fader budget is arithmetic rather than a guess. The old layout
/// had no budget at all — sections were stacked and the overflow pushed into a
/// scroll area, which is how the fader and transport ended up unreachable.
pub(crate) const STRIP_META_H: f32 = 74.0;
pub(crate) const STRIP_KNOBS_H: f32 = 88.0;
pub(crate) const STRIP_TRANSPORT_H: f32 = 34.0;
pub(crate) const STRIP_DRAWER_H: f32 = 26.0;
/// Separators and inter-section spacing.
pub(crate) const STRIP_CHROME_H: f32 = 28.0;
/// Below this the fader is too short to aim at, so the strip is allowed to be
/// taller than its column rather than shipping an unusable control.
pub(crate) const STRIP_FADER_MIN: f32 = 56.0;
pub(crate) const STRIP_FADER_MAX: f32 = 200.0;

/// Height the volume fader gets for a strip of `strip_h`.
///
/// Everything the strip draws must fit inside the column it was allocated, so
/// the fader takes what is left after the fixed sections rather than a constant.
pub(crate) fn strip_fader_height(strip_h: f32) -> f32 {
    let fixed = STRIP_META_H + STRIP_KNOBS_H + STRIP_TRANSPORT_H + STRIP_DRAWER_H + STRIP_CHROME_H;
    (strip_h - fixed).clamp(STRIP_FADER_MIN, STRIP_FADER_MAX)
}

/// Total height the strip will occupy at this fader size.
///
/// Only the layout tests call this — it exists so "does the strip fit its
/// column" is an assertion rather than something you find out by looking.
#[cfg(test)]
pub(crate) fn strip_total_height(strip_h: f32) -> f32 {
    STRIP_META_H + STRIP_KNOBS_H + STRIP_TRANSPORT_H + STRIP_DRAWER_H + STRIP_CHROME_H
        + strip_fader_height(strip_h)
}

/// One deck: identity, controls, transport, and a drawer for the rest.
///
/// The always-visible part is what you touch while mixing. Hot cues and the DNA
/// panel move into a collapsed drawer — previously they were stacked inline,
/// which is what pushed the fader and transport past the bottom of the window.
fn render_channel_strip(
    app: &mut InspectorApp,
    ui: &mut Ui,
    i: usize,
    telemetry: &Option<Telemetry>,
    strip_h: f32,
) {
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, i);
    let is_focused = app.decks.focused_deck == i;

    Frame::none()
        .fill(if is_focused { theme.bg_surface } else { theme.bg_canvas })
        .stroke(Stroke::new(if is_focused { 1.5 } else { 1.0 },
                            if is_focused { deck_color } else { theme.border }))
        .rounding(theme.radius_md)
        .inner_margin(Margin::symmetric(theme.space_xs, theme.space_sm))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                render_strip_meta(app, ui, i, deck_color, is_focused, telemetry);
                ui.add_space(theme.space_xs);
                ui.separator();
                ui.add_space(theme.space_xs);

                let fader_h = strip_fader_height(strip_h);

                mixer::render_deck_mixer(app, ui, i, deck_color, fader_h);

                ui.add_space(theme.space_xs);
                transport::render_deck_transport(app, ui, i);
                ui.add_space(theme.space_xs);

                egui::CollapsingHeader::new(
                    RichText::new("PADS · DNA").size(theme.type_caption).color(theme.text_secondary))
                    .id_source(format!("deck_drawer_{i}"))
                    .default_open(false)
                    .show(ui, |ui| {
                        performance::render_deck_performance(app, ui, i, telemetry);
                        ui.add_space(theme.space_xs);
                        dna::render_deck_dna_panel(app, ui, i);
                    });
            });
        });
}

fn render_stereo_vu_meter(ui: &mut Ui, peak_l: f32, peak_r: f32, peak_hold: f32, accent_color: Color32, height: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        widgets::render_vu_meter(ui, peak_l, peak_hold, accent_color, height);
        widgets::render_vu_meter(ui, peak_r, peak_hold, accent_color, height);
    });
}

fn render_master_section(app: &mut InspectorApp, ui: &mut Ui, _telemetry: &Option<Telemetry>) {
    let theme = app.theme;
    // Width bounded INSIDE the frame: vertical_centered expands to the full
    // remaining width otherwise (the ballooning master card), and a
    // full-height container would stretch every wrapped row to panel height.
    Frame::none()
        .fill(theme.bg_surface)
        .stroke(theme.border_stroke)
        .rounding(theme.radius_md)
        .inner_margin(Margin::symmetric(theme.space_sm, theme.space_md))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {

                // Crossfader — drives BOTH sides of the stereo crossfader
                // pair, resolved by name. (The old "master_crossfader" name
                // is only registered by the standalone builder; the lookup
                // fell back to node 0 = deck A's sampler.)
                ui.label(RichText::new("CROSSFADER").size(theme.type_caption).color(theme.text_secondary));
                ui.add_space(theme.space_xs);
                let xf_width = (ui.available_width() - 16.0).clamp(80.0, 160.0);
                let r_cross = widgets::render_horizontal_fader(ui, &mut app.mixer.crossfader_pos, 0.0..=1.0, theme.text_primary, xf_width, 32.0);
                if r_cross.changed() {
                    for name in ["master_xf_l", "master_xf_r"] {
                        if let Some(&node) = app.topo.node_map.get(name) {
                            let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: node as u64,
                                param_id: 0,
                                value: app.mixer.crossfader_pos,
                                ramp_duration_samples: 0,
                            }));
                        }
                    }
                }
                if r_cross.drag_stopped() || r_cross.lost_focus() {
                    let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                }
                ui.add_space(theme.space_xs);
                ui.horizontal(|ui| {
                    ui.add_space(theme.space_sm);
                    ui.label(RichText::new("A").size(theme.type_caption).color(theme.text_secondary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(theme.space_sm);
                        ui.label(RichText::new("B").size(theme.type_caption).color(theme.text_secondary));
                    });
                });

                ui.add_space(theme.space_sm);
                // Divider
                ui.separator();
                ui.add_space(theme.space_sm);

                // Master, Booth, and Rec Out with Stereo VU Meters
                ui.label(RichText::new("OUTPUTS").strong().size(theme.type_body).color(theme.text_secondary));
                ui.add_space(theme.space_xs);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;

                    // Master VU — the only real output pair in the graph.
                    // (The old BTH/REC meters displayed the master scaled by
                    // 0.8: there are no booth or record sends in the
                    // topology, and a meter that means nothing is worse than
                    // no meter. Build the sends first, then meter them.)
                    ui.vertical(|ui| {
                        ui.label(RichText::new("MST").size(theme.type_caption).color(theme.text_secondary));
                        render_stereo_vu_meter(ui, app.viz.damped_master_peaks[0], app.viz.damped_master_peaks[1], app.mixer.master_peak_hold, theme.text_primary, 100.0);
                    });

                    // Master Gain Fader — per-side summing gain, resolved by
                    // name. ("master_sum" never existed: get_node_id fell
                    // back to 0 and this fader set a deck A sampler param.)
                    ui.vertical(|ui| {
                        ui.label(RichText::new("GAIN").size(theme.type_caption).color(theme.text_secondary));
                        let r_master = widgets::render_fader(ui, &mut app.mixer.master_gain, 0.0..=1.5, theme.text_primary, 100.0, 18.0);
                        if r_master.changed() {
                            for name in ["master_sum_l", "master_sum_r"] {
                                if let Some(&node) = app.topo.node_map.get(name) {
                                    let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                        target_id: node as u64,
                                        param_id: 0,
                                        value: app.mixer.master_gain,
                                        ramp_duration_samples: 0,
                                    }));
                                }
                            }
                        }
                        if r_master.drag_stopped() || r_master.lost_focus() {
                            let _ = app.command_sender.send(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CheckpointParameterEdit));
                        }
                    });
                });
            });
        });
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    /// Mixer height available on the reference machine: 1920x1080 fullscreen,
    /// minus the app header and the waveform stack (30% of the panel).
    fn reference_mixer_height() -> f32 {
        let panel_h = 1080.0 - 60.0;      // window minus chrome/top bar
        let waveforms = panel_h * 0.30;
        panel_h - 40.0 - waveforms - 12.0 // header + spacing
    }

    #[test]
    fn test_strip_fits_its_column_at_the_reference_resolution() {
        // The bug this replaces: sections were stacked with no budget, the sum
        // exceeded the column, and a ScrollArea hid the difference — putting the
        // fader and transport below the fold on a live control surface.
        let h = reference_mixer_height();
        let used = strip_total_height(h);
        assert!(
            used <= h,
            "channel strip needs {used:.0} px but its column is {h:.0} px — the fader and \
             transport would be pushed out of view"
        );
    }

    #[test]
    fn test_strip_fits_across_plausible_window_heights() {
        // Anything from a small window up to a tall display. Below the point
        // where the fader hits its minimum the strip is allowed to exceed the
        // column — a 20 px fader is not a control — but that must be a
        // deliberate floor, not silent overflow, so assert where it starts.
        let fixed = STRIP_META_H + STRIP_KNOBS_H + STRIP_TRANSPORT_H + STRIP_DRAWER_H + STRIP_CHROME_H;
        let floor = fixed + STRIP_FADER_MIN;
        for h in [300.0f32, 400.0, 500.0, 650.0, 800.0, 1000.0, 1400.0] {
            let used = strip_total_height(h);
            if h >= floor {
                assert!(used <= h, "strip needs {used:.0} px in a {h:.0} px column");
            } else {
                assert_eq!(
                    strip_fader_height(h), STRIP_FADER_MIN,
                    "below {floor:.0} px the fader must sit at its minimum, not shrink further"
                );
            }
        }
    }

    #[test]
    fn test_fader_grows_with_the_window_up_to_its_cap() {
        // A taller window should give a longer fader, not more empty space.
        assert!(
            strip_fader_height(700.0) > strip_fader_height(400.0),
            "fader does not use extra vertical space"
        );
        assert_eq!(
            strip_fader_height(4000.0), STRIP_FADER_MAX,
            "fader must stop growing so an ultrawide display does not produce a 3 m slider"
        );
    }

    #[test]
    fn test_four_decks_and_master_fit_the_reference_width() {
        // Deck columns are computed as (avail - master - gaps) / 4 with a 120 px
        // floor. At 1920 they must comfortably clear that floor, or the strips
        // would start overlapping the master rail.
        let avail = 1920.0f32 - 24.0; // panel margins
        let master_w = 92.0;
        let gap = 4.0;
        let deck_w = (avail - master_w - gap * 4.0) / 4.0;
        assert!(
            deck_w >= 120.0,
            "deck column is {deck_w:.0} px, under the 120 px floor — strips would overflow"
        );
        // Six 30 px dials in rows of three, plus spacing, must fit the column.
        let knob_row = 30.0 * 3.0 + gap * 2.0;
        assert!(
            knob_row < deck_w,
            "a row of three dials is {knob_row:.0} px in a {deck_w:.0} px column"
        );
    }
}
