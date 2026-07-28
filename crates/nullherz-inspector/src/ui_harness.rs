//! Drive the real UI headlessly, so "does this control actually work" is a test.
//!
//! Three of this project's defects were in the view layer and every one of them
//! was invisible to the rest of the suite, because the code was correct — it
//! just never ran, or reported a number it was not measuring:
//!
//!   * the deck lane's scroll-to-scrub handler was complete and unreachable.
//!     `render_waveform_lane` called `ui.interact()` over the whole lane AFTER
//!     the waveform, so the later interaction won every pointer event and
//!     `response.hovered()` inside the waveform was permanently false.
//!   * the channel meter drew the same peak twice and called it a stereo pair.
//!   * two telemetry fields rendered confident numbers from sources nothing
//!     ever wrote.
//!
//! No amount of unit testing finds those, because each part is individually
//! right. What catches them is running the view with synthetic input and
//! asserting on the commands that come out the other side.
//!
//! egui runs perfectly well without a window: `Context::run` takes a `RawInput`
//! and returns the frame's output. That is all this needs.

use crate::{state, GraphJson, InspectorApp, SharedLibraryDb, View};
use nullherz_traits::Command;
use std::sync::mpsc;
use std::sync::Arc;

/// Screen used for layout in tests — the reference machine, fullscreen.
pub const TEST_SCREEN: egui::Vec2 = egui::vec2(1920.0, 1080.0);

/// A headless InspectorApp plus the receiving end of its command channel.
pub struct Harness {
    pub app: InspectorApp,
    pub commands: mpsc::Receiver<Command>,
    ctx: egui::Context,
    time: f64,
}

impl Harness {
    /// Build an app with nothing loaded. Renderers are `None`; every view path
    /// guards on them because the GPU is optional at runtime too.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let db = nullherz_dna::LibraryDatabase::load(":memory:")
            .expect("in-memory library");
        let app = InspectorApp {
            graph: GraphJson { nodes: vec![], edges: vec![], node_assignments: Default::default() },
            command_sender: tx,
            last_telemetry: Arc::new(parking_lot::Mutex::new(None)),
            active_view: View::Console,
            mixer: state::MixerState::default(),
            decks: state::DeckState::default(),
            library: state::LibraryState::default(),
            composer: state::ComposerState::default(),
            sampler: state::SamplerState::default(),
            editor: state::EditorState::default(),
            broadcast: state::BroadcastState::default(),
            settings: state::SettingsState::default(),
            viz: state::VizState::default(),
            topo: state::TopologyViewState::default(),
            library_db: SharedLibraryDb(Arc::new(parking_lot::Mutex::new(db))),
            active_right_tab: None,
            breeding_view: crate::views::breeder::BreederView::new(),
            wgpu_renderer: None,
            waveform_renderer: None,
            deck_waveform_renderers: [None, None, None, None],
            discovered_sidecars: Vec::new(),
            p2p_sync_success_toast: None,
            export_passport_success_toast: None,
            export_passport_error_toast: None,
            theme: nullherz_ui_hal::Theme::default(),
            last_update_time: 0.0,
            _conductor_thread: None,
        };
        Self { app, commands: rx, ctx: egui::Context::default(), time: 0.0 }
    }

    /// Put a track on a deck, as a load would.
    pub fn load_deck(&mut self, deck: usize, id: u64, title: &str, total_frames: u64) {
        let mut meta = nullherz_traits::SampleMetadata::new_empty();
        meta.sample_rate = 48_000;
        meta.channels = 2;
        meta.total_samples = total_frames;
        meta.bpm = 0.0; // deliberately untempo'd: scrubbing must not need one
        let track = nullherz_dna::LibraryTrack {
            id,
            path: format!("/tmp/{title}.wav"),
            title: title.to_string(),
            artist: "test".into(),
            album: "test".into(),
            genre: "test".into(),
            energy_level: 0.5,
            metadata: Arc::new(meta),
        };
        self.app.decks.now_playing[deck] = Some(id);
        self.app.decks.cached_tracks[deck] = Some(track.clone());
        self.app.library.cached_library_raw.push(track);
        // The view resolves node ids through the telemetry-fed map.
        self.app.topo.node_map.insert(format!("deck_{}_sampler", (b'a' + deck as u8) as char), deck as u32);
    }

    /// Render one frame of the console with the given input events.
    pub fn frame(&mut self, events: Vec<egui::Event>, pointer: Option<egui::Pos2>) {
        self.time += 1.0 / 60.0;
        let mut ev = events;
        if let Some(p) = pointer {
            ev.insert(0, egui::Event::PointerMoved(p));
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, TEST_SCREEN)),
            time: Some(self.time),
            events: ev,
            ..Default::default()
        };
        let app = &mut self.app;
        let telemetry = None;
        // The frame's output (textures, shapes, platform commands) is not
        // needed here — the harness asserts on the COMMANDS the view emits.
        let _ = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                crate::views::dj_studio::render(app, ui, &telemetry);
            });
        });
    }

    /// Every command emitted so far.
    pub fn drain(&self) -> Vec<Command> {
        self.commands.try_iter().collect()
    }
}

impl Default for Harness {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::PerformanceCommand;

    /// Vertical band the four waveform lanes occupy: the view gives them 30% of
    /// the panel below the header. Sampled at several heights rather than one
    /// exact pixel so the test survives spacing tweaks.
    fn lane_probe_points() -> Vec<egui::Pos2> {
        (1..=8).map(|n| egui::pos2(900.0, 40.0 + n as f32 * 30.0)).collect()
    }

    /// egui 0.27 accumulates `raw_scroll_delta` from `Event::Scroll` only —
    /// `Event::MouseWheel` falls through its match arm untouched (see
    /// egui-0.27.2/src/input_state.rs). Emitting the wrong one produces a frame
    /// with zero scroll and a test that fails for a reason that has nothing to
    /// do with the code under test.
    fn wheel(delta: f32) -> egui::Event {
        egui::Event::Scroll(egui::vec2(0.0, delta))
    }

    #[test]
    fn test_scrolling_a_deck_lane_emits_a_position_command() {
        // THE regression test for the unreachable handler. If a later
        // `ui.interact()` ever covers the waveform again, this goes silent —
        // which is exactly how the bug presented.
        let mut h = Harness::new();
        h.load_deck(0, 11, "probe", 48_000 * 120);

        // One frame to lay out, then scroll over each candidate point.
        h.frame(vec![], None);
        let _ = h.drain();

        let mut nudges = 0;
        for p in lane_probe_points() {
            h.frame(vec![wheel(50.0)], Some(p));
            nudges += h.drain().iter().filter(|c| matches!(
                c, Command::Performance(PerformanceCommand::NudgePosition { .. }))).count();
        }

        assert!(
            nudges > 0,
            "scrolling over the deck lanes produced no NudgePosition command. The handler \
             is unreachable — most likely something is interacting over the waveform and \
             taking its pointer events."
        );
    }

    #[test]
    fn test_scrolling_an_empty_lane_emits_nothing() {
        // No track, no seeking. Guards against a nudge addressed at a deck with
        // nothing on it, which would move another deck's sampler id.
        let mut h = Harness::new();
        h.frame(vec![], None);
        let _ = h.drain();

        for p in lane_probe_points() {
            h.frame(vec![wheel(50.0)], Some(p));
        }
        let nudges = h.drain().iter().filter(|c| matches!(
            c, Command::Performance(PerformanceCommand::NudgePosition { .. }))).count();
        assert_eq!(nudges, 0, "an empty deck lane emitted {nudges} position command(s)");
    }

    #[test]
    fn test_scroll_direction_is_signed() {
        // Wheel up must move forward and wheel down backward. A sign error here
        // is not a crash, just a control that fights the hand.
        let mut h = Harness::new();
        h.load_deck(0, 11, "probe", 48_000 * 120);
        h.frame(vec![], None);
        let _ = h.drain();

        let collect = |h: &mut Harness, delta: f32| -> Vec<i64> {
            let mut out = Vec::new();
            for p in lane_probe_points() {
                h.frame(vec![wheel(delta)], Some(p));
                for c in h.drain() {
                    if let Command::Performance(PerformanceCommand::NudgePosition { frames, .. }) = c {
                        out.push(frames);
                    }
                }
            }
            out
        };

        let up = collect(&mut h, 50.0);
        let down = collect(&mut h, -50.0);
        assert!(!up.is_empty() && !down.is_empty(), "no commands to compare directions with");
        assert!(up.iter().all(|f| *f > 0), "wheel up produced non-forward moves: {up:?}");
        assert!(down.iter().all(|f| *f < 0), "wheel down produced non-backward moves: {down:?}");
    }

    #[test]
    fn test_the_console_renders_without_a_gpu() {
        // Every renderer is None here, as it is on a machine without working
        // wgpu. The view must degrade rather than panic.
        let mut h = Harness::new();
        h.load_deck(0, 1, "a", 48_000 * 60);
        h.load_deck(1, 2, "b", 48_000 * 60);
        for _ in 0..3 {
            h.frame(vec![], Some(egui::pos2(900.0, 500.0)));
        }
    }
}
