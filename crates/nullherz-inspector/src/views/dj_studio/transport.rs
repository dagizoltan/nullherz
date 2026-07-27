use egui::{Ui, RichText};
use crate::InspectorApp;

/// Deck transport: one play/stop toggle, plus CUE and SYNC.
///
/// The toggle replaces a stacked pair of buttons — a 36x24 play above a 36x16
/// stop. Play and stop are one mutually exclusive state, so showing two targets
/// asks the operator to work out which one applies before pressing it, and the
/// button for the more urgent action was the smaller of the two. One control
/// that displays the current state and switches it is both a bigger target and
/// a shorter decision.
pub fn render_deck_transport(app: &mut InspectorApp, ui: &mut Ui, i: usize) {
    let deck_id = (b'A' + i as u8) as char;
    let theme = app.theme;
    let deck_color = crate::InspectorApp::deck_color(&theme, i);
    let is_playing = app.decks.deck_playing[i];
    let has_track = app.decks.now_playing[i].is_some();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space_xs;
        let h = 28.0;
        let avail = ui.available_width();
        // Transport gets half the row; CUE and SYNC split the rest.
        let play_w = (avail * 0.44).max(56.0);
        let side_w = ((avail - play_w - theme.space_xs * 2.0) / 2.0).max(34.0);

        // --- PLAY / STOP -----------------------------------------------------
        // Filled while playing so deck state reads from across the room; the
        // glyph shows what the control WILL DO, the word shows what it IS.
        let (glyph, word, fill, fg) = if !has_track {
            ("▶", "—", theme.bg_surface, theme.text_disabled)
        } else if is_playing {
            ("■", "PLAYING", deck_color, theme.bg_dark)
        } else {
            ("▶", "STOPPED", theme.bg_surface, theme.text_secondary)
        };

        let btn = egui::Button::new(
            RichText::new(format!("{glyph}  {word}")).size(theme.type_caption).strong().color(fg),
        )
        .fill(fill);

        let resp = ui.add_enabled_ui(has_track, |ui| ui.add_sized([play_w, h], btn)).inner;
        if resp.clicked() {
            let cmd = if is_playing {
                nullherz_traits::PerformanceCommand::StopDeck { deck_id }
            } else {
                nullherz_traits::PerformanceCommand::PlayDeck { deck_id }
            };
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(cmd));
            // Reflect immediately. Telemetry confirms within a frame or two, but
            // a toggle that does not move the instant it is pressed reads as a
            // missed click and invites a second press — which would undo it.
            app.decks.deck_playing[i] = !is_playing;
        }
        if has_track {
            resp.on_hover_text(if is_playing { "Stop this deck" } else { "Play this deck" });
        }

        // --- CUE -------------------------------------------------------------
        let cue = ui.add_sized(
            [side_w, h],
            egui::Button::new(RichText::new("CUE").size(theme.type_caption).strong()).fill(theme.bg_surface),
        );
        if cue.clicked() {
            let node_name = match i {
                0 => "deck_a_sampler",
                1 => "deck_b_sampler",
                2 => "deck_c_sampler",
                3 => "deck_d_sampler",
                _ => "",
            };
            if let Some(node_idx) = app.get_node_id(node_name) {
                let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                    nullherz_traits::PerformanceCommand::JumpToHotCue { node_idx, cue_idx: 0 },
                ));
            }
        }
        cue.on_hover_text("Jump to hot cue 1");

        // --- SYNC ------------------------------------------------------------
        // Dimmed on the master deck: syncing a deck to itself is a no-op, and a
        // live-looking button that does nothing is worse than a dim one.
        let is_master = app.decks.master_deck == Some(i);
        let sync = ui.add_enabled_ui(!is_master, |ui| {
            ui.add_sized(
                [side_w, h],
                egui::Button::new(RichText::new("SYNC").size(theme.type_caption).strong())
                    .fill(if is_master { theme.bg_surface } else { theme.accent_muted }),
            )
        })
        .inner;
        if sync.clicked() {
            let master_deck_id = (b'A' + app.decks.master_deck.unwrap_or(0) as u8) as char;
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                nullherz_traits::PerformanceCommand::SyncDecks {
                    source_deck: master_deck_id,
                    target_deck: deck_id,
                },
            ));
        }
        if !is_master {
            sync.on_hover_text("Match tempo and phase to the master deck");
        }
    });
}
