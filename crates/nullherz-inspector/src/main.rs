use serde::Deserialize;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use audio_core::Telemetry;
use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Deserialize, Debug, Clone)]
pub struct NodeJson {
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GraphJson {
    pub nodes: Vec<NodeJson>,
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: nullherz-inspector [--gui] <graph.json>");
        return Ok(());
    }

    let (gui_mode, path) = if args.len() > 1 && args[1] == "--gui" {
        if args.len() < 3 {
            eprintln!("Error: --gui requires a <graph.json> path.");
            eprintln!("Usage: nullherz-inspector [--gui] <graph.json>");
            return Ok(());
        }
        (true, &args[2])
    } else {
        (false, &args[1])
    };

    let content = fs::read_to_string(path).expect("Failed to read file");
    let graph: GraphJson = serde_json::from_str(&content).expect("Failed to parse JSON");

    if gui_mode {
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0])
                .with_title("nullherz Precision Studio"),
            ..Default::default()
        };
        eframe::run_native(
            "nullherz Studio",
            native_options,
            Box::new(|cc| {
                let app: Box<dyn eframe::App> = Box::new(InspectorApp::new(graph, cc));
                app
            }),
        )
    } else {
        println!("nullherz Topology Inspector");
        println!("===========================");
        render_ascii(&graph);
        Ok(())
    }
}

fn render_ascii(graph: &GraphJson) {
    for (i, node) in graph.nodes.iter().enumerate() {
        let ins = node.inputs.iter().map(|idx| format!("Buf{}", idx)).collect::<Vec<_>>().join(", ");
        let outs = node.outputs.iter().map(|idx| format!("Buf{}", idx)).collect::<Vec<_>>().join(", ");

        println!("  [{}]  --> ( Node {} ) --> [{}]", ins, i, outs);
        if i < graph.nodes.len() - 1 {
            println!("             |");
        }
    }
}

#[derive(PartialEq)]
enum View {
    Studio,
    Mixer,
    Sampler,
    Mastering,
    Broadcast,
    Topology,
    Arranger,
}

pub struct Track {
    pub title: String,
    pub artist: String,
    pub bpm: f32,
}

pub struct InspectorApp {
    graph: GraphJson,
    last_telemetry: Arc<Mutex<Option<Telemetry>>>,
    sample_registry: Arc<nullherz_dna::SampleRegistry>,
    command_sender: mpsc::Sender<nullherz_traits::Command>,
    active_view: View,

    // UI State — all controls bound to persistent state
    channel_faders: [f32; 4],
    channel_trims: [f32; 4],
    channel_eq_high: [f32; 4],
    channel_eq_mid: [f32; 4],
    channel_eq_low: [f32; 4],
    channel_fx_enabled: [bool; 4],
    channel_cue: [bool; 4],
    channel_sync: [bool; 4],
    quantize_enabled: bool,
    master_gain: f32,
    crossfader_pos: f32,
    library_db: nullherz_dna::LibraryDatabase,
    search_query: String,
    is_streaming: bool,
    library_visible: bool,
    selected_deck: usize,
    master_deck: Option<usize>,
    pitch_range: [f32; 4], // 0.08, 0.16, 1.0
    crossfader_curve: f32, // 0.0 = linear, 1.0 = power
    now_playing: [Option<String>; 4],
    global_bpm: f32,
    pitch_bend: [f32; 4],
    macros: [f32; 8],
    macro_names: [String; 8],

    // Peak hold
    channel_peak_hold: [f32; 4],
    master_peak_hold: f32,

    // Mastering chain state
    mastering_eq_enabled: bool,
    mastering_comp_enabled: bool,
    mastering_limiter_enabled: bool,
    mastering_limiter_gain: f32,
    mastering_comp_threshold: f32,
}

impl InspectorApp {
    fn deck_color(i: usize) -> egui::Color32 {
        match i {
            0 => egui::Color32::from_rgb(0, 255, 200),   // Neon Mint (A)
            1 => egui::Color32::from_rgb(0, 180, 255),   // Electric Blue (B)
            2 => egui::Color32::from_rgb(255, 150, 0),   // Vibrant Orange (C)
            3 => egui::Color32::from_rgb(255, 50, 150),  // Hot Pink (D)
            _ => egui::Color32::WHITE,
        }
    }

    fn render_knob(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, label: &str) -> egui::Response {
        let size = egui::vec2(32.0, 32.0);
        let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::drag());

        if response.dragged() {
            let old_value = *value;
            let delta = response.drag_delta().y * -0.01;
            *value = (*value + delta).clamp(*range.start(), *range.end());
            if *value != old_value {
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact(&response);
            let center = rect.center();
            let radius = rect.width() / 2.0;

            // Outer Circle (Tactile Aluminum)
            ui.painter().circle_filled(center, radius, egui::Color32::from_gray(30));
            ui.painter().circle_stroke(center, radius, egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

            // Subtle shadow
            ui.painter().circle_stroke(center, radius + 1.0, egui::Stroke::new(1.0, egui::Color32::from_black_alpha(50)));

            // Pointer line
            let normalized = (*value - *range.start()) / (*range.end() - *range.start());
            let angle = egui::lerp(
                (-135.0f32).to_radians()..=(135.0f32).to_radians(),
                normalized,
            );
            let (sin, cos) = angle.sin_cos();
            let pointer_start = center + egui::vec2(sin, -cos) * (radius * 0.2);
            let pointer_end = center + egui::vec2(sin, -cos) * (radius * 0.95);

            let color = if (normalized - 0.5).abs() < 0.01 { egui::Color32::from_rgb(0, 255, 200) } else { visuals.fg_stroke.color };
            ui.painter().line_segment([pointer_start, pointer_end], egui::Stroke::new(4.0, egui::Color32::BLACK));
            ui.painter().line_segment([pointer_start, pointer_end], egui::Stroke::new(2.0, color));

            // Small indicator dot for center
            ui.painter().circle_filled(center, 2.5, egui::Color32::from_gray(80));

            if !label.is_empty() {
                ui.painter().text(rect.center_bottom() + egui::vec2(0.0, 5.0), egui::Align2::CENTER_TOP, label, egui::FontId::proportional(8.0), egui::Color32::from_gray(120));
            }
        }

        response
    }

    pub fn new(graph: GraphJson, cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = 0.0.into();

        let bg_deep = egui::Color32::from_rgb(8, 8, 10);
        let accent_primary = egui::Color32::from_rgb(0, 255, 200); // Neon Mint
        let stroke_dim = egui::Color32::from_gray(20);

        visuals.widgets.noninteractive.bg_fill = bg_deep;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, stroke_dim);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(15, 15, 18);
        visuals.widgets.inactive.rounding = 2.0.into();
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(25, 25, 30);
        visuals.widgets.active.bg_fill = accent_primary;

        visuals.selection.bg_fill = accent_primary.linear_multiply(0.2);
        cc.egui_ctx.set_visuals(visuals);

        // Adjust spacing for high-density "Pro" layout
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.visuals.window_rounding = 2.0.into();
        cc.egui_ctx.set_style(style);

        let last_telemetry = Arc::new(Mutex::new(None));
        let tel_clone = last_telemetry.clone();
        let sample_registry = Arc::new(nullherz_dna::SampleRegistry::new());
        let (cmd_tx, cmd_rx) = mpsc::channel::<nullherz_traits::Command>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let url = "ws://127.0.0.1:9001";
                if let Ok((ws_stream, _)) = connect_async(url).await {
                    let (mut write, mut read) = ws_stream.split();

                    let sender_task = tokio::spawn(async move {
                        while let Ok(cmd) = cmd_rx.recv() {
                            let ts_cmd = nullherz_traits::TimestampedCommand {
                                timestamp_samples: 0,
                                command: cmd,
                            };
                            if let Ok(json) = serde_json::to_string(&ts_cmd) {
                                let _ = write.send(Message::Text(json.into())).await;
                            }
                        }
                    });

                    while let Some(Ok(msg)) = read.next().await {
                        if let Ok(text) = msg.to_text() {
                            let tel_res = serde_json::from_str::<Telemetry>(text);
                            if let Ok(tel) = tel_res {
                                let mut lock = tel_clone.lock().unwrap();
                                *lock = Some(tel);
                            }
                        }
                    }
                    sender_task.abort();
                }
            });
        });

        Self {
            graph,
            last_telemetry,
            sample_registry,
            command_sender: cmd_tx,
            active_view: View::Studio,
            channel_faders: [0.8; 4],
            channel_trims: [1.0; 4],
            channel_eq_high: [1.0; 4],
            channel_eq_mid: [1.0; 4],
            channel_eq_low: [1.0; 4],
            channel_fx_enabled: [false; 4],
            channel_cue: [false; 4],
            channel_sync: [false; 4],
            quantize_enabled: true,
            master_gain: 1.0,
            crossfader_pos: 0.5,
            library_db: nullherz_dna::LibraryDatabase::load("library.redb").expect("Failed to load library"),
            search_query: String::new(),
            is_streaming: false,
            library_visible: true,
            selected_deck: 0,
            master_deck: Some(0),
            pitch_range: [0.08; 4],
            crossfader_curve: 0.5,
            now_playing: [None, None, None, None],
            global_bpm: 128.0,
            pitch_bend: [1.0; 4],
            macros: [0.0; 8],
            macro_names: [
                "MACRO 1".to_string(), "MACRO 2".to_string(), "MACRO 3".to_string(), "MACRO 4".to_string(),
                "MACRO 5".to_string(), "MACRO 6".to_string(), "MACRO 7".to_string(), "MACRO 8".to_string(),
            ],
            channel_peak_hold: [0.0; 4],
            master_peak_hold: 0.0,
            mastering_eq_enabled: true,
            mastering_comp_enabled: true,
            mastering_limiter_enabled: false,
            mastering_limiter_gain: 1.0,
            mastering_comp_threshold: 0.5,
        }
    }

    fn render_oscillator_monitor(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 160.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(5, 5, 6));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(20)));

        let time = ui.input(|i| i.time);
        let w = rect.width();
        let h = rect.height();

        // High-density background grid
        for i in 0..16 {
            let x = rect.min.x + (i as f32 * (w / 16.0));
            ui.painter().vline(x, rect.y_range(), egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 5)));
        }

        // Aggregate All-Deck Visualization
        for i in 0..4 {
            let color = Self::deck_color(i).linear_multiply(0.4);
            let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1]);
            let speed = 4.0 + (i as f32 * 2.0);
            let offset = i as f32 * 0.5;

            let points: Vec<egui::Pos2> = (0..w as i32).step_by(3).map(|x| {
                let px = x as f32 / w;
                let wave = (px * 20.0 + time as f32 * speed + offset).sin() * (px * 10.0 + time as f32).cos();
                let amp = (h * 0.3) * wave * peak.min(1.0);
                egui::pos2(rect.min.x + x as f32, rect.center().y + amp)
            }).collect();

            ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
        }

        ui.painter().text(rect.min + egui::vec2(10.0, 10.0), egui::Align2::LEFT_TOP, "WIDESCREEN OSCILLATOR MONITOR", egui::FontId::proportional(10.0), egui::Color32::from_gray(80));
    }

    fn render_library(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none().fill(egui::Color32::from_rgb(12, 12, 14)).inner_margin(12.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("LIBRARY").color(egui::Color32::from_gray(150)).small().strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("REFRESH").clicked() {
                        if let Ok(db) = nullherz_dna::LibraryDatabase::load("library.redb") {
                            self.library_db = db;
                        }
                    }
                });
            });
            ui.add_space(10.0);
            ui.text_edit_singleline(&mut self.search_query);
            ui.add_space(15.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut tracks = self.library_db.list_tracks().unwrap_or_default();
                tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

                for track in &tracks {
                    let title = &track.title;
                    let artist = &track.artist;
                    let bpm = track.metadata.bpm;

                    if !self.search_query.is_empty() {
                        let q = self.search_query.to_lowercase();
                        if !title.to_lowercase().contains(&q) && !artist.to_lowercase().contains(&q) {
                            continue;
                        }
                    }

                    let (rect, res) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
                    let how_h = ui.ctx().animate_bool(res.id, res.hovered());
                    if how_h > 0.0 { ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray((how_h * 20.0) as u8)); }

                    res.context_menu(|ui| {
                        for deck_idx in 0..4 {
                            if ui.button(format!("Load to Deck {}", (b'A' + deck_idx as u8) as char)).clicked() {
                                let _ = self.command_sender.send(nullherz_traits::Command::AddSourceFromRegistry {
                                    granular_node_idx: (deck_idx as u32 * 4),
                                    sample_id: track.id,
                                });
                                        self.now_playing[deck_idx] = Some(title.to_string());
                                ui.close_menu();
                            }
                        }
                    });

                    if res.clicked() {
                        let _ = self.command_sender.send(nullherz_traits::Command::AddSourceFromRegistry {
                            granular_node_idx: (self.selected_deck as u32 * 4),
                            sample_id: track.id,
                        });
                                self.now_playing[self.selected_deck] = Some(title.to_string());
                    }

                    ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
                        ui.add_space(5.0);
                        let is_loaded = self.now_playing.iter().any(|np| np.as_deref() == Some(title));
                        let t_color = if is_loaded { egui::Color32::from_rgb(0, 255, 150) } else { egui::Color32::WHITE };

                        ui.label(egui::RichText::new(format!("{} - {}", title, artist)).size(11.0).color(t_color));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(5.0);
                            ui.label(egui::RichText::new(format!("{:.0}", bpm)).color(egui::Color32::from_gray(80)).size(10.0));
                        });
                    });
                    ui.painter().hline(rect.x_range(), rect.max.y, egui::Stroke::new(1.0, egui::Color32::from_gray(20)));
                }
            });
        });
    }

    fn render_dj_studio(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        let total_w = ui.available_width();
        let mixer_w = 320.0;
        let deck_w = (total_w - mixer_w - 48.0) / 4.0;

        ui.vertical(|ui| {
            ui.set_width(total_w);
            ui.add_space(5.0);

            // TOP: FULLSCREEN WIDE OSCILLATOR MONITOR
            self.render_oscillator_monitor(ui, telemetry);
            ui.add_space(10.0);

            // MACRO ROW
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(egui::RichText::new("MACROS").small().strong().color(egui::Color32::from_gray(120)));
                ui.add_space(20.0);
                for i in 0..8 {
                    ui.vertical(|ui| {
                        ui.set_width(60.0);
                        if Self::render_knob(ui, &mut self.macros[i], 0.0..=1.0, "").changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetMacro {
                                macro_id: i as u32,
                                value: self.macros[i],
                            });
                        }
                        ui.label(egui::RichText::new(&self.macro_names[i]).size(7.0).color(egui::Color32::from_gray(100)));
                    });
                    ui.add_space(10.0);
                }
            });
            ui.add_space(10.0);

            // PERFORMANCE ROW: A, B, MIXER, C, D
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);

                // DECK A
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 0, telemetry, 420.0); });
                // DECK B
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 1, telemetry, 420.0); });

                // ULTRA-COMPACT CENTRAL MIXER
                ui.vertical(|ui| {
                    ui.set_width(mixer_w);
                    self.render_central_mixer(ui, telemetry, mixer_w);
                });

                // DECK C
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 2, telemetry, 420.0); });
                // DECK D
                ui.vertical(|ui| { ui.set_width(deck_w); self.render_deck(ui, 3, telemetry, 420.0); });
            });

            ui.add_space(20.0);

            // INTEGRATED MASTER & STATUS DASHBOARD
            ui.horizontal(|ui| {
                let dash_w = total_w * 0.4;
                let (d_rect, _) = ui.allocate_exact_size(egui::vec2(dash_w, 60.0), egui::Sense::hover());
                ui.painter().rect_filled(d_rect, 2.0, egui::Color32::from_rgb(8, 8, 10));

                ui.child_ui(d_rect, egui::Layout::left_to_right(egui::Align::Center)).horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        ui.add_space(10.0);
                        let cpu_pct = telemetry.as_ref().map_or(0.0, |t| {
                            let budget_ns = (128.0 / 44100.0) * 1e9;
                            (t.process_time_ns as f64 / budget_ns * 100.0).min(100.0)
                        });
                        ui.label(egui::RichText::new(format!("CPU {:.0}%", cpu_pct)).small().color(egui::Color32::from_gray(80)));
                        ui.label(egui::RichText::new("SYSTEM STABLE").color(egui::Color32::from_rgb(0, 255, 150)).size(8.0));
                    });

                    ui.add_space(30.0);
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("-").clicked() { self.global_bpm -= 1.0; }
                            ui.add(egui::DragValue::new(&mut self.global_bpm).speed(0.1).suffix(" BPM"));
                            if ui.button("+").clicked() { self.global_bpm += 1.0; }
                        });
                        let q_color = if self.quantize_enabled { egui::Color32::from_rgb(255, 50, 50) } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("QUANTIZE").small().strong().color(q_color)).frame(false)).clicked() {
                            self.quantize_enabled = !self.quantize_enabled;
                        }
                    });
                });

                ui.add_space(ui.available_width() - 320.0);
                self.render_master_panel(ui, telemetry);
            });
        });
    }

    fn render_topology(&self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Engine Topology");
        ui.add_space(10.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
        let painter = ui.painter();
        let cell_w = rect.width() / 64.0;
        for i in 0..64 {
            let load = telemetry.as_ref().map_or(0.0, |t| (t.node_times_ns[i] as f32 / 500000.0).min(1.0));
            painter.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * cell_w, 0.0), egui::vec2(cell_w, 24.0)), 0.0, egui::Color32::from_rgb((load * 255.0) as u8, (255.0 * (1.0 - load)) as u8, 100));
        }
        ui.add_space(20.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, node) in self.graph.nodes.iter().enumerate() {
                ui.label(format!("Node {:02}: In {:?} Out {:?}", i, node.inputs, node.outputs));
            }
        });
    }

    fn render_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Studio Console");
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            for i in 0..4 {
                ui.vertical_centered(|ui| {
                    ui.strong(format!("CH {}", i + 1));
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                    if ui.add(egui::Slider::new(&mut self.channel_faders[i], 0.0..=1.2).vertical().show_value(false)).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4 + 1),
                            param_id: 0,
                            value: self.channel_faders[i] * self.channel_trims[i],
                            ramp_duration_samples: 128,
                        });
                    }
                    ui.label(format!("{:.0}%", peak * 100.0));
                });
                ui.add_space(50.0);
            }
        });
    }

    fn render_deck(&mut self, ui: &mut egui::Ui, i: usize, telemetry: &Option<Telemetry>, height: f32) {
        let deck_name = format!("DECK {}", (b'A' + i as u8) as char);
        let color = Self::deck_color(i);

        let is_selected = self.selected_deck == i;
        let stroke_color = if is_selected { color } else { egui::Color32::from_gray(30) };

        let rect = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click()).0;
        if ui.rect_contains_pointer(rect) && ui.input(|i| i.pointer.any_click()) {
            self.selected_deck = i;
        }

        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, stroke_color));
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 10, 12));

        ui.child_ui(rect, egui::Layout::top_down(egui::Align::Center)).vertical(|ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(deck_name).color(if is_selected { color } else { egui::Color32::from_gray(180) }).strong());

                let is_master = self.master_deck == Some(i);
                let master_color = if is_master { color } else { egui::Color32::from_gray(40) };
                if ui.add(egui::Button::new(egui::RichText::new("MASTER").small().strong().color(master_color)).frame(true)).clicked() {
                    if is_master { self.master_deck = None; } else { self.master_deck = Some(i); }
                }

                if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                    let key_text = if let Some(key) = sample.metadata.root_key {
                        let notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                        format!("Key: {}", notes[(key.round() as usize) % 12])
                    } else {
                        "Key: -".to_string()
                    };
                    ui.label(egui::RichText::new(format!("{} | {:.1} BPM", key_text, sample.metadata.bpm)).small().color(egui::Color32::from_gray(100)));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1]);
                    let db = 20.0 * peak.log10().max(-60.0);
                    ui.label(egui::RichText::new(format!("{:.1} dB", db)).small().color(if peak > 1.0 { egui::Color32::RED } else { egui::Color32::from_gray(100) }));
                });
            });

            ui.horizontal(|ui| {
                ui.add_space(10.0);
                egui::Frame::none().fill(egui::Color32::from_rgb(5, 5, 6)).inner_margin(4.0).rounding(2.0).show(ui, |ui| {
                    ui.set_width(ui.available_width() - 80.0);
                    if let Some(ref title) = self.now_playing[i] {
                        ui.label(egui::RichText::new(title).color(color).size(11.0).strong());
                    } else {
                        ui.label(egui::RichText::new("EMPTY").color(egui::Color32::from_gray(40)).size(11.0));
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    // REMAINING TIME SIMULATION
                    let total_sec = 324.0;
                    let elapsed = (ui.input(|i| i.time) % total_sec as f64) as f32;
                    let remaining = total_sec - elapsed;
                    let mins = (remaining / 60.0) as i32;
                    let secs = (remaining % 60.0) as i32;
                    ui.label(egui::RichText::new(format!("-{:02}:{:02}", mins, secs)).color(color).monospace().size(12.0));
                });
            });

            // WAVEFORM
            ui.add_space(10.0);
            let (w_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 20.0, 120.0), egui::Sense::hover());
            ui.painter().rect_filled(w_rect, 2.0, egui::Color32::from_rgb(5, 5, 6));

            let w_width = w_rect.width();
            let w_height = w_rect.height();

            let time = ui.input(|i| i.time);
            if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                let peaks = &sample.metadata.peaks;
                if !peaks.is_empty() {
                    let mut points = Vec::new();
                    for x in 0..w_width as usize {
                        let p_idx = (x * peaks.len()) / w_width as usize;
                        let val = peaks[p_idx.min(peaks.len() - 1)];
                        let y_off = val * (w_height / 2.0);
                        points.push(egui::pos2(w_rect.min.x + x as f32, w_rect.center().y - y_off));
                        points.push(egui::pos2(w_rect.min.x + x as f32, w_rect.center().y + y_off));
                    }
                    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.0, color)));

                    // DETAIL SCROLLING WAVEFORM
                    ui.add_space(5.0);
                    let (d_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 20.0, 40.0), egui::Sense::hover());
                    ui.painter().rect_filled(d_rect, 2.0, egui::Color32::from_rgb(5, 5, 6));

                    let dw = d_rect.width();
                    let dh = d_rect.height();
                    let play_head_norm = (time as f32 % 30.0) / 30.0; // Simulated playhead
                    let view_range = 0.05; // 5% of track

                    let mut d_points = Vec::new();
                    for x in 0..dw as usize {
                        let rel_x = x as f32 / dw;
                        let track_norm = (play_head_norm - view_range/2.0 + rel_x * view_range).clamp(0.0, 1.0);
                        let p_idx = (track_norm * peaks.len() as f32) as usize;
                        let val = peaks[p_idx.min(peaks.len() - 1)];
                        let y_off = val * (dh / 2.0);
                        d_points.push(egui::pos2(d_rect.min.x + x as f32, d_rect.center().y - y_off));
                        d_points.push(egui::pos2(d_rect.min.x + x as f32, d_rect.center().y + y_off));
                    }
                    ui.painter().add(egui::Shape::line(d_points, egui::Stroke::new(1.5, color.additive())));
                    ui.painter().vline(d_rect.center().x, d_rect.y_range(), egui::Stroke::new(2.0, egui::Color32::WHITE));
                }
            } else {
                // FALLBACK Simulation if no real data
                let time = ui.input(|i| i.time);
                for (band, speed, scale) in [ (0, 8.0, 15.0), (1, 12.0, 10.0), (2, 20.0, 5.0) ] {
                    let points: Vec<egui::Pos2> = (0..w_width as i32).step_by(2).map(|x| {
                        let phase = x as f32 * 0.05 * (band as f32 + 1.0) + (time * speed) as f32;
                        let amp = scale * ((phase * 0.01).cos().abs() + 0.2);
                        egui::pos2(w_rect.min.x + x as f32, w_rect.center().y + (phase.sin() * amp))
                    }).collect();
                    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.2, color.linear_multiply(0.8 - band as f32 * 0.2))));
                }
            }

            // High-Visibility Beat Markers
            for b in 0..8 {
                let bx = w_rect.min.x + (w_width * (b as f32 / 8.0));
                ui.painter().vline(bx, w_rect.y_range(), egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30)));
            }

            // Transient Metadata Overlay
            if let Some(sample) = self.sample_registry.get(i as u64 * 4) {
                let total_samples = sample.buffer.len() as f32;
                if total_samples > 0.0 {
                    for &pos in sample.metadata.transients.iter() {
                        let x_off = (pos as f32 / total_samples).fract(); // Simple wrap for simulation
                        let tx = w_rect.min.x + (x_off * w_width);
                        ui.painter().vline(tx, w_rect.y_range(), egui::Stroke::new(1.5, color.linear_multiply(0.5)));
                    }
                }
            }

            // Central Playhead
            ui.painter().vline(w_rect.center().x, w_rect.y_range(), egui::Stroke::new(2.5, egui::Color32::from_rgb(255, 255, 255)));

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.add_space(15.0);
                // JOG SIMULATION (Simplified)
                let (jog_rect, _) = ui.allocate_exact_size(egui::vec2(100.0, 100.0), egui::Sense::hover());
                ui.painter().circle_stroke(jog_rect.center(), 50.0, egui::Stroke::new(2.0, egui::Color32::from_gray(40)));
                let angle = (time * 2.0) as f32;
                let needle = jog_rect.center() + egui::vec2(angle.cos() * 45.0, angle.sin() * 45.0);
                ui.painter().line_segment([jog_rect.center(), needle], egui::Stroke::new(2.0, color));

                ui.add_space(15.0);

                ui.vertical(|ui| {
                    let cue_color = if self.channel_cue[i] { egui::Color32::from_rgb(255, 150, 0) } else { egui::Color32::from_gray(40) };
                    if ui.add(egui::Button::new(egui::RichText::new("CUE").color(cue_color).strong()).min_size(egui::vec2(60.0, 45.0))).clicked() {
                        self.channel_cue[i] = !self.channel_cue[i];
                    }
                    ui.add_space(8.0);
                    let play_color = egui::Color32::from_rgb(0, 255, 100);
                    if ui.add(egui::Button::new(egui::RichText::new("PLAY").color(play_color).strong()).min_size(egui::vec2(60.0, 45.0))).clicked() {
                        // Play logic
                    }
                });

                ui.add_space(15.0);

                ui.vertical(|ui| {
                    let s_color = if self.channel_sync[i] { color } else { egui::Color32::from_gray(40) };
                    if ui.add(egui::Button::new(egui::RichText::new("SYNC").color(s_color).strong()).min_size(egui::vec2(60.0, 45.0))).clicked() {
                        self.channel_sync[i] = !self.channel_sync[i];
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4),
                            param_id: 2, // Sync/Quantize toggle
                            value: if self.channel_sync[i] { 1.0 } else { 0.0 },
                            ramp_duration_samples: 0,
                        });
                    }
                });

                ui.add_space(15.0);

                // PITCH FADER
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("PITCH").size(9.0).strong().color(egui::Color32::from_gray(100)));

                    egui::ComboBox::from_id_source(format!("pitch_range_{}", i))
                        .selected_text(format!("{:.0}%", self.pitch_range[i] * 100.0))
                        .width(40.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.pitch_range[i], 0.08, "8%");
                            ui.selectable_value(&mut self.pitch_range[i], 0.16, "16%");
                            ui.selectable_value(&mut self.pitch_range[i], 1.0, "WIDE");
                        });

                    let range = (1.0 - self.pitch_range[i])..=(1.0 + self.pitch_range[i]);
                    let p_res = ui.add(egui::Slider::new(&mut self.pitch_bend[i], range).vertical().show_value(false).handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 4.0 }));
                    if p_res.changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: (i as u64 * 4), // Targeting Sampler node
                            param_id: 1, // Assume 1 is playback rate
                            value: self.pitch_bend[i],
                            ramp_duration_samples: 128,
                        });
                    }
                    let pct = (self.pitch_bend[i] - 1.0) * 100.0;
                    ui.label(egui::RichText::new(format!("{:+.1}%", pct)).size(10.0).monospace().color(color));
                });
            });

            // PHASE METER
            ui.add_space(5.0);
            let (p_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 30.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(p_rect, 1.0, egui::Color32::from_rgb(5, 5, 6));
            ui.painter().vline(p_rect.center().x, p_rect.y_range(), egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

            if self.channel_sync[i] {
                let phase_diff = (time.sin() * 0.5) as f32; // Simulated phase diff
                let px = p_rect.center().x + (phase_diff * p_rect.width() * 0.4);
                ui.painter().circle_filled(egui::pos2(px, p_rect.center().y), 4.0, color);
            }
        });
    }

    fn render_master_panel(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        egui::Frame::none().fill(egui::Color32::from_rgb(25, 25, 30)).rounding(4.0).stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(40))).inner_margin(12.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(70.0);
                    if Self::render_knob(ui, &mut self.mastering_limiter_gain, 0.0..=2.0, "CEIL").changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: self.mastering_limiter_gain, ramp_duration_samples: 128 });
                    }
                    ui.add_space(10.0);
                    if Self::render_knob(ui, &mut self.mastering_comp_threshold, 0.0..=1.0, "THR").changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 20, param_id: 0, value: self.mastering_comp_threshold, ramp_duration_samples: 128 });
                    }
                });

                ui.add_space(15.0);

                let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[21].min(1.2));
                if peak > self.master_peak_hold { self.master_peak_hold = peak; }
                else { self.master_peak_hold *= 0.98; }

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                    for _ in 0..2 {
                        let (m_rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 100.0), egui::Sense::hover());
                        ui.painter().rect_filled(m_rect, 1.0, egui::Color32::from_rgb(10, 10, 12));

                            // Tick marks
                            for db in [-48, -24, -12, -6, 0] {
                                let val = 10.0f32.powf(db as f32 / 20.0);
                                let ty = m_rect.max.y - (val * 100.0).min(100.0);
                                ui.painter().hline(m_rect.x_range(), ty, egui::Stroke::new(0.5, egui::Color32::from_gray(60)));
                            }

                        let m_h = (peak * 100.0).min(100.0);
                            let m_p_rect = egui::Rect::from_min_size(m_rect.max - egui::vec2(10.0, m_h), egui::vec2(10.0, m_h));
                            let color = if peak > 1.0 { egui::Color32::from_rgb(255, 50, 50) } else { egui::Color32::from_rgb(0, 255, 180) };
                        ui.painter().rect_filled(m_p_rect, 0.0, color);
                    }
                });

                ui.add_space(10.0);

                let m_fader = ui.add(egui::Slider::new(&mut self.master_gain, 0.0..=1.5).vertical().show_value(false).handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 4.0 }));
                if m_fader.changed() {
                    let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id: 21, param_id: 0, value: self.master_gain, ramp_duration_samples: 128 });
                }
            });
        });
    }

    fn render_central_mixer(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>, main_w: f32) {
        let rect = ui.allocate_exact_size(egui::vec2(main_w, 420.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(15, 15, 18));
        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(30)));

        ui.child_ui(rect, egui::Layout::top_down(egui::Align::Center)).vertical_centered(|ui| {
            ui.add_space(10.0);

            // CHANNEL STRIPS
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
                for i in 0..4 {
                    ui.vertical(|ui| {
                        ui.set_width(60.0);

                        // GAIN / TRIM
                        if Self::render_knob(ui, &mut self.channel_trims[i], 0.0..=2.0, "TRIM").changed() {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4 + 1),
                                param_id: 0,
                                value: self.channel_trims[i] * self.channel_faders[i],
                                ramp_duration_samples: 128,
                            });
                        }

                        ui.add_space(10.0);

                        // HI / MID / LOW
                        for (label, param_idx, state_val) in [("HI", 2, &mut self.channel_eq_high[i]), ("MID", 1, &mut self.channel_eq_mid[i]), ("LOW", 0, &mut self.channel_eq_low[i])] {
                            if Self::render_knob(ui, state_val, 0.0..=2.0, label).changed() {
                                let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                    target_id: (i as u64 * 4 + 3),
                                    param_id: param_idx,
                                    value: *state_val,
                                    ramp_duration_samples: 0,
                                });
                            }
                            ui.add_space(6.0);
                        }

                        ui.add_space(8.0);

                        // FX BUTTON
                        let fx_color = if self.channel_fx_enabled[i] { egui::Color32::from_rgb(0, 255, 200) } else { egui::Color32::from_gray(40) };
                        if ui.add(egui::Button::new(egui::RichText::new("FX").color(fx_color).small().strong()).min_size(egui::vec2(40.0, 24.0))).clicked() {
                            self.channel_fx_enabled[i] = !self.channel_fx_enabled[i];
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                target_id: (i as u64 * 4 + 2),
                                param_id: 999,
                                value: if self.channel_fx_enabled[i] { 1.0 } else { 0.0 },
                                ramp_duration_samples: 0,
                            });
                        }

                        ui.add_space(12.0);

                        // FADER & VU
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                            let fader_res = ui.add(egui::Slider::new(&mut self.channel_faders[i], 0.0..=1.0).vertical().show_value(false).handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 5.0 }));
                            if fader_res.changed() {
                                let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                                    target_id: (i as u64 * 4 + 1),
                                    param_id: 0,
                                    value: self.channel_trims[i] * self.channel_faders[i],
                                    ramp_duration_samples: 128,
                                });
                            }

                            // VU METER
                            let peak = telemetry.as_ref().map_or(0.0, |t| t.peak_levels[i*4 + 1].min(1.2));
                            if peak > self.channel_peak_hold[i] { self.channel_peak_hold[i] = peak; }
                            else { self.channel_peak_hold[i] *= 0.98; }

                            let (m_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 100.0), egui::Sense::hover());
                            ui.painter().rect_filled(m_rect, 1.0, egui::Color32::from_rgb(10, 10, 12));

                            // Tick marks
                            for db in [-24, -12, -6, 0] {
                                let val = 10.0f32.powf(db as f32 / 20.0);
                                let ty = m_rect.max.y - (val * 120.0).min(120.0);
                                ui.painter().hline(m_rect.x_range(), ty, egui::Stroke::new(0.5, egui::Color32::from_gray(60)));
                            }

                            let m_h = (peak * 100.0).min(100.0);
                            let m_p_rect = egui::Rect::from_min_size(m_rect.max - egui::vec2(8.0, m_h), egui::vec2(8.0, m_h));
                            let meter_color = if peak > 1.0 { egui::Color32::from_rgb(255, 50, 50) } else { egui::Color32::from_rgb(0, 255, 180) };
                            ui.painter().rect_filled(m_p_rect, 0.0, meter_color);

                            // Peak hold line
                            let ph_h = (self.channel_peak_hold[i] * 100.0).min(100.0);
                            let ph_y = m_rect.max.y - ph_h;
                            ui.painter().hline(m_rect.x_range(), ph_y, egui::Stroke::new(1.0, Self::deck_color(i)));
                        });
                    });
                }
            });

            ui.add_space(20.0);

            ui.add_space(15.0);

            // GLOBAL CROSSFADER (Centered below faders)
            ui.vertical_centered(|ui| {
                ui.set_width(main_w);
                ui.horizontal(|ui| {
                    ui.add_space(main_w / 2.0 - 40.0);
                    ui.label(egui::RichText::new("X-FADE").small().strong().color(egui::Color32::from_gray(100)));
                    if ui.add(egui::Button::new(if self.crossfader_curve > 0.5 { "POWER" } else { "LIN" }).small()).clicked() {
                        self.crossfader_curve = if self.crossfader_curve > 0.5 { 0.0 } else { 1.0 };
                        for target_id in [16, 17] {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 1, value: self.crossfader_curve, ramp_duration_samples: 0 });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("A").color(egui::Color32::from_rgb(0, 200, 255)));
                    ui.spacing_mut().slider_width = main_w - 60.0;
                    let x_slider = ui.add(egui::Slider::new(&mut self.crossfader_pos, 0.0..=1.0).show_value(false).handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.5 }));
                    if x_slider.changed() {
                        for target_id in [16, 17] {
                            let _ = self.command_sender.send(nullherz_traits::Command::SetParam { target_id, param_id: 0, value: self.crossfader_pos, ramp_duration_samples: 0 });
                        }
                    }
                    ui.label(egui::RichText::new("B").color(egui::Color32::from_rgb(0, 255, 150)));
                });
            });
        });
    }

    fn render_sampler(&mut self, ui: &mut egui::Ui) {
        ui.heading("Production Sampler");
        ui.add_space(10.0);
        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.strong("SAMPLE BANK");
                if let Ok(tracks) = self.library_db.list_tracks() {
                    for track in tracks { ui.label(&track.title); }
                }
            });
            cols[1].group(|ui| {
                ui.strong("GRID SEQUENCER");
                ui.label("Beat Step Active");
            });
        });
    }

    fn render_mastering(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mastering Chain");
        ui.add_space(20.0);
        ui.vertical(|ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.mastering_eq_enabled, "MASTER EQ").changed() {
                        // Bypass logic: 1.0 = active, 0.0 = bypass
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: 19, param_id: 999, value: if self.mastering_eq_enabled { 1.0 } else { 0.0 }, ramp_duration_samples: 0
                        });
                    }
                });
                ui.add_space(5.0);
                ui.label("Simulated 3-band response");
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.mastering_comp_enabled, "DYNAMIC COMP").changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: 20, param_id: 999, value: if self.mastering_comp_enabled { 1.0 } else { 0.0 }, ramp_duration_samples: 0
                        });
                    }
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label("Threshold");
                    if ui.add(egui::Slider::new(&mut self.mastering_comp_threshold, 0.0..=1.0)).changed() {
                         let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: 20, param_id: 0, value: self.mastering_comp_threshold, ramp_duration_samples: 128
                        });
                    }
                });
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.mastering_limiter_enabled, "BRICKWALL LIMITER").changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: 21, param_id: 999, value: if self.mastering_limiter_enabled { 1.0 } else { 0.0 }, ramp_duration_samples: 0
                        });
                    }
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label("Ceiling (dB)");
                    if ui.add(egui::Slider::new(&mut self.mastering_limiter_gain, 0.0..=1.5)).changed() {
                        let _ = self.command_sender.send(nullherz_traits::Command::SetParam {
                            target_id: 21, param_id: 0, value: self.mastering_limiter_gain, ramp_duration_samples: 128
                        });
                    }
                });
            });
        });
    }

    fn render_broadcast(&mut self, ui: &mut egui::Ui) {
        ui.heading("Live Broadcast Hub");
        ui.add_space(10.0);
        if ui.button(if self.is_streaming { "🛑 STOP STREAM" } else { "🚀 GO LIVE" }).clicked() { self.is_streaming = !self.is_streaming; }
        ui.label(format!("Status: {}", if self.is_streaming { "ONLINE" } else { "OFFLINE" }));
    }

    fn render_arranger(&mut self, ui: &mut egui::Ui, telemetry: &Option<Telemetry>) {
        ui.heading("Song Arranger");
        ui.add_space(10.0);

        let current_beat = telemetry.as_ref().map_or(0.0, |t| t.sample_counter as f64 / (44100.0 * 60.0 / self.global_bpm as f64));
        ui.label(format!("Current Beat: {:.2}", current_beat));

        ui.add_space(20.0);
        ui.label("Timeline (Mockup)");
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 200.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 18));

        // Draw grid
        for i in 0..32 {
            let x = rect.min.x + (i as f32 * (rect.width() / 32.0));
            let color = if i % 4 == 0 { egui::Color32::from_gray(60) } else { egui::Color32::from_gray(30) };
            ui.painter().vline(x, rect.y_range(), egui::Stroke::new(1.0, color));
        }

        // Playhead
        let ph_x = rect.min.x + (current_beat.fract() as f32 * rect.width());
        ui.painter().vline(ph_x, rect.y_range(), egui::Stroke::new(2.0, egui::Color32::RED));

        ui.add_space(20.0);
        ui.group(|ui| {
            ui.label("Add Arrangement Event");
            ui.horizontal(|ui| {
                ui.button("Trigger Pattern A on Deck 1 at Beat 16");
                ui.button("Set Macro 1 to 0.8 at Beat 32");
            });
        });
    }
}

impl eframe::App for InspectorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let telemetry = *self.last_telemetry.lock().unwrap();

        // LEFT-ALIGNED VERTICAL NAVIGATION
        egui::SidePanel::left("nav_panel")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(8, 8, 10)).inner_margin(12.0))
            .width_range(60.0..=60.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("NH").color(egui::Color32::from_rgb(0, 255, 200)).strong().size(20.0));
                    ui.add_space(30.0);

                    for (view, label, icon) in [
                        (View::Studio, "STUDIO", "🎧"),
                        (View::Mixer, "MIXER", "🎚"),
                        (View::Sampler, "SAMPLER", "🎹"),
                        (View::Mastering, "MASTERING", "💎"),
                        (View::Broadcast, "BROADCAST", "📡"),
                        (View::Arranger, "ARRANGER", "📊"),
                        (View::Topology, "TOPOLOGY", "🕸"),
                    ] {
                        let is_active = self.active_view == view;
                        let bg_color = if is_active { egui::Color32::from_rgb(0, 255, 200).linear_multiply(0.1) } else { egui::Color32::TRANSPARENT };

                        egui::Frame::none().fill(bg_color).rounding(4.0).show(ui, |ui| {
                            if ui.add(egui::Button::new(egui::RichText::new(icon).size(20.0)).frame(false)).on_hover_text(label).clicked() {
                                self.active_view = view;
                            }
                        });
                        ui.add_space(20.0);
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        if ui.add(egui::Button::new(egui::RichText::new(if self.library_visible { "📁" } else { "📂" }).size(20.0)).frame(false)).clicked() {
                            self.library_visible = !self.library_visible;
                        }
                    });
                });
            });

        // COLLAPSIBLE LIBRARY SIDEBAR
        if self.library_visible {
            egui::SidePanel::right("library_panel")
                .frame(egui::Frame::none().fill(egui::Color32::from_rgb(12, 12, 14)))
                .width_range(250.0..=350.0)
                .show(ctx, |ui| {
                    self.render_library(ui);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_view {
                View::Studio => self.render_dj_studio(ui, &telemetry),
                View::Mixer => self.render_mixer(ui, &telemetry),
                View::Sampler => self.render_sampler(ui),
                View::Topology => self.render_topology(ui, &telemetry),
                View::Mastering => self.render_mastering(ui),
                View::Broadcast => self.render_broadcast(ui),
                View::Arranger => self.render_arranger(ui, &telemetry),
            }
        });

        ctx.request_repaint();
    }
}
