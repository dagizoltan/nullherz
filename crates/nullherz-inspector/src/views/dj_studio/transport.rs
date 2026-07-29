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
        // Transport gets a bit over a third; CUE, SYNC, KEY and LOCK split the rest.
        let play_w = (avail * 0.34).max(52.0);
        let side_w = ((avail - play_w - theme.space_xs * 4.0) / 4.0).max(26.0);

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

        // --- SYNC / KEY -------------------------------------------------------
        // Both are LATCHES, and both are off by default: the console plays a
        // track the way it was recorded until the operator says otherwise.
        //
        // SYNC used to send `SyncDecks`, which `MixerOrchestrator` matches and
        // then does nothing with ("Future: implementation for BPM/Phase sync
        // logic") — a lit, enabled button wired to a no-op. Meanwhile the tempo
        // and pitch changes it appeared to offer were being applied to every deck
        // automatically, with no control at all. This inverts that: the automatic
        // path is gone and these two buttons are how you ask for it.
        //
        // Lit = engaged, so a glance at the deck answers "is this track being
        // altered?" — which under the old behaviour was unanswerable.
        let sync_on = app.decks.deck_sync[i];
        let sync = ui.add_sized(
            [side_w, h],
            egui::Button::new(
                RichText::new("SYNC")
                    .size(theme.type_caption)
                    .strong()
                    .color(if sync_on { theme.bg_dark } else { theme.text_secondary }),
            )
            .fill(if sync_on { theme.accent } else { theme.bg_surface }),
        );
        if sync.clicked() {
            app.decks.deck_sync[i] = !sync_on;
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                nullherz_traits::PerformanceCommand::SetDeckSync { deck_id, enabled: !sync_on },
            ));
        }
        sync.on_hover_text(if sync_on {
            "SYNC on: deck follows the master tempo. Click to return to the track's own tempo."
        } else {
            "RAW: playing at the track's own tempo. Click to tempo-match to the transport."
        });

        // KEY matches this deck to the MASTER DECK's key, so it is meaningless on
        // the master itself — dimmed rather than lit-and-inert, which is the
        // state the old SYNC button was in.
        let is_master = app.decks.master_deck == Some(i);
        let key_on = app.decks.deck_key_sync[i];
        let key = ui
            .add_enabled_ui(!is_master, |ui| {
                ui.add_sized(
                    [side_w, h],
                    egui::Button::new(
                        RichText::new("KEY")
                            .size(theme.type_caption)
                            .strong()
                            .color(if key_on { theme.bg_dark } else { theme.text_secondary }),
                    )
                    .fill(if key_on { theme.accent } else { theme.bg_surface }),
                )
            })
            .inner;
        if key.clicked() {
            app.decks.deck_key_sync[i] = !key_on;
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                nullherz_traits::PerformanceCommand::SetDeckKeySync { deck_id, enabled: !key_on },
            ));
        }
        key.on_hover_text(if is_master {
            "This deck IS the master — it defines the key others match to."
        } else if key_on {
            "KEY on: pitch-shifted to match the master deck's key. Click to return to the track's own pitch."
        } else {
            "RAW: playing at the track's own pitch. Click to match the master deck's key."
        });

        // --- LOCK (master tempo) -----------------------------------------------
        // Distinct from KEY: KEY matches ANOTHER deck's key, LOCK stops THIS
        // deck's own tempo changes from moving its pitch. Both install the same
        // vocoder in the pitch slot and their corrections add, so either one lit
        // means this deck is paying the 21.3 ms window.
        let lock_on = app.decks.deck_key_lock[i];
        let lock = ui.add_sized(
            [side_w, h],
            egui::Button::new(
                RichText::new("LOCK")
                    .size(theme.type_caption)
                    .strong()
                    .color(if lock_on { theme.bg_dark } else { theme.text_secondary }),
            )
            .fill(if lock_on { theme.accent } else { theme.bg_surface }),
        );
        if lock.clicked() {
            app.decks.deck_key_lock[i] = !lock_on;
            let _ = app.command_sender.send(nullherz_traits::Command::Performance(
                nullherz_traits::PerformanceCommand::SetDeckKeyLock { deck_id, enabled: !lock_on },
            ));
        }
        lock.on_hover_text(if lock_on {
            "KEY LOCK on: tempo changes keep the original pitch (master tempo)."
        } else if sync_on {
            "RAW: SYNC changes pitch along with tempo, like a turntable. Click to hold the pitch."
        } else {
            "Master tempo — holds pitch when SYNC changes the tempo. No effect while this deck runs at its own tempo."
        });
    });
}
