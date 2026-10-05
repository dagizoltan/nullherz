use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardGridStyle {
    PianoKeys,
    PadMatrix,
}

impl KeyboardGridStyle {
    pub fn name(&self) -> &'static str {
        match self {
            Self::PianoKeys => "Piano Keys",
            Self::PadMatrix => "Pad Matrix (4x4)",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::PianoKeys, Self::PadMatrix]
    }
}

/// Midi event trigger produced by the keyboard grid widget
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardNoteTrigger {
    pub note: u8,
    pub velocity: u8,
    pub is_note_on: bool,
}

/// Render interactive Keyboard Grid widget (Piano Keyboard or Pad Matrix)
pub fn render_keyboard_grid(
    ui: &mut Ui,
    octave: &mut i8,
    style: &mut KeyboardGridStyle,
    active_notes: &[u8],
    color: Color32,
) -> Vec<KeyboardNoteTrigger> {
    let mut triggers = Vec::new();

    ui.vertical(|ui| {
        // Top Toolbar: Style selector + Octave Controls
        ui.horizontal(|ui| {
            ui.label(RichText::new("KEYBOARD GRID").strong().size(10.0).color(Color32::from_rgb(180, 190, 210)));

            ui.add_space(8.0);

            // Style selector
            egui::ComboBox::from_id_source("kbd_grid_style_combo")
                .selected_text(RichText::new(style.name()).size(10.0).strong())
                .show_ui(ui, |ui| {
                    for s in KeyboardGridStyle::all() {
                        ui.selectable_value(style, *s, s.name());
                    }
                });

            ui.add_space(12.0);

            // Octave controls
            ui.label(RichText::new("OCTAVE:").size(10.0).color(Color32::from_rgb(140, 150, 170)));
            if ui.add_enabled(*octave > -2, egui::Button::new(RichText::new("◀").size(10.0)).min_size(Vec2::new(18.0, 18.0))).clicked() {
                *octave = (*octave - 1).max(-2);
            }
            let oct_label = if *octave >= 0 { format!("+{}", octave) } else { format!("{}", octave) };
            ui.label(RichText::new(oct_label).monospace().size(10.0).strong().color(color));
            if ui.add_enabled(*octave < 2, egui::Button::new(RichText::new("▶").size(10.0)).min_size(Vec2::new(18.0, 18.0))).clicked() {
                *octave = (*octave + 1).min(2);
            }
        });

        ui.add_space(4.0);

        let base_note = (60i16 + (*octave as i16) * 12).clamp(0, 127) as u8;

        match style {
            KeyboardGridStyle::PianoKeys => {
                let piano_triggers = render_piano_keyboard(ui, base_note, active_notes, color);
                triggers.extend(piano_triggers);
            }
            KeyboardGridStyle::PadMatrix => {
                let pad_triggers = render_pad_matrix(ui, base_note, active_notes, color);
                triggers.extend(pad_triggers);
            }
        }
    });

    triggers
}

fn render_piano_keyboard(
    ui: &mut Ui,
    base_note: u8,
    active_notes: &[u8],
    color: Color32,
) -> Vec<KeyboardNoteTrigger> {
    let mut triggers = Vec::new();

    // 2 octaves = 14 white keys
    const WHITE_KEY_COUNT: usize = 14;
    let available_w = ui.available_width().max(280.0);
    let key_w = (available_w / WHITE_KEY_COUNT as f32).max(18.0);
    let key_h = 75.0f32;

    let (rect, _response) = ui.allocate_exact_size(Vec2::new(key_w * WHITE_KEY_COUNT as f32, key_h), Sense::hover());
    let painter = ui.painter();

    // White key semitone offsets relative to base_note
    let white_offsets = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23];
    let white_labels = ["C", "D", "E", "F", "G", "A", "B", "C", "D", "E", "F", "G", "A", "B"];

    // First pass: Render White Keys
    for i in 0..WHITE_KEY_COUNT {
        let note = base_note.saturating_add(white_offsets[i]);
        let key_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + i as f32 * key_w, rect.min.y),
            Vec2::new(key_w - 1.0, key_h),
        );

        let is_active = active_notes.contains(&note);
        let key_response = ui.interact(key_rect, ui.make_persistent_id(format!("white_key_{}", note)), Sense::click_and_drag());

        let fill_color = if is_active || key_response.is_pointer_button_down_on() {
            color
        } else if key_response.hovered() {
            Color32::from_rgb(230, 235, 245)
        } else {
            Color32::from_rgb(245, 248, 255)
        };

        painter.rect_filled(key_rect, 2.0, fill_color);
        painter.rect_stroke(key_rect, 2.0, Stroke::new(1.0_f32, Color32::from_rgb(180, 190, 200)));

        // Label on white key
        let text_color = if is_active || key_response.is_pointer_button_down_on() {
            Color32::BLACK
        } else {
            Color32::from_rgb(100, 110, 130)
        };
        painter.text(
            Pos2::new(key_rect.center().x, key_rect.bottom() - 10.0),
            egui::Align2::CENTER_CENTER,
            white_labels[i],
            egui::FontId::proportional(9.0),
            text_color,
        );

        if key_response.clicked() || key_response.drag_started() {
            // Velocity computed by vertical position on key (higher = louder)
            let click_y = key_response.interact_pointer_pos().map(|p| p.y - key_rect.top()).unwrap_or(key_h * 0.5);
            let vel = ((1.0 - (click_y / key_h).clamp(0.0, 1.0)) * 100.0 + 27.0) as u8;
            triggers.push(KeyboardNoteTrigger { note, velocity: vel, is_note_on: true });
        }
    }

    // Second pass: Render Black Keys on top
    // Black key indices relative to white keys (0=C#, 1=D#, 3=F#, 4=G#, 5=A#)
    let black_key_pattern = [
        (0, 1),  // C#
        (1, 3),  // D#
        (3, 6),  // F#
        (4, 8),  // G#
        (5, 10), // A#
        (7, 13), // C# +1
        (8, 15), // D# +1
        (10, 18),// F# +1
        (11, 20),// G# +1
        (12, 22),// A# +1
    ];

    let black_w = key_w * 0.6;
    let black_h = key_h * 0.6;

    for (white_idx, semitone_offset) in black_key_pattern {
        let note = base_note.saturating_add(semitone_offset);
        let black_x = rect.min.x + (white_idx as f32 + 1.0) * key_w - black_w * 0.5;
        let black_rect = Rect::from_min_size(Pos2::new(black_x, rect.min.y), Vec2::new(black_w, black_h));

        let is_active = active_notes.contains(&note);
        let key_response = ui.interact(black_rect, ui.make_persistent_id(format!("black_key_{}", note)), Sense::click_and_drag());

        let fill_color = if is_active || key_response.is_pointer_button_down_on() {
            color
        } else if key_response.hovered() {
            Color32::from_rgb(60, 65, 80)
        } else {
            Color32::from_rgb(20, 22, 30)
        };

        painter.rect_filled(black_rect, 1.0, fill_color);
        painter.rect_stroke(black_rect, 1.0, Stroke::new(1.0_f32, Color32::from_rgb(40, 45, 60)));

        if key_response.clicked() || key_response.drag_started() {
            let click_y = key_response.interact_pointer_pos().map(|p| p.y - black_rect.top()).unwrap_or(black_h * 0.5);
            let vel = ((1.0 - (click_y / black_h).clamp(0.0, 1.0)) * 100.0 + 27.0) as u8;
            triggers.push(KeyboardNoteTrigger { note, velocity: vel, is_note_on: true });
        }
    }

    triggers
}

fn render_pad_matrix(
    ui: &mut Ui,
    base_note: u8,
    active_notes: &[u8],
    color: Color32,
) -> Vec<KeyboardNoteTrigger> {
    let mut triggers = Vec::new();

    let grid_size = 4;
    let available_w = ui.available_width().max(280.0);
    let pad_w = (available_w / grid_size as f32) - 4.0;
    let pad_h = 36.0f32;

    let pad_names = [
        "PAD 1 (C)", "PAD 2 (C#)", "PAD 3 (D)", "PAD 4 (D#)",
        "PAD 5 (E)", "PAD 6 (F)", "PAD 7 (F#)", "PAD 8 (G)",
        "PAD 9 (G#)", "PAD 10 (A)", "PAD 11 (A#)", "PAD 12 (B)",
        "PAD 13 (+C)", "PAD 14 (+C#)", "PAD 15 (+D)", "PAD 16 (+D#)",
    ];

    ui.vertical(|ui| {
        for row in 0..grid_size {
            ui.horizontal(|ui| {
                for col in 0..grid_size {
                    let pad_idx = (grid_size - 1 - row) * grid_size + col;
                    let note = base_note.saturating_add(pad_idx as u8);
                    let label = pad_names.get(pad_idx).copied().unwrap_or("PAD");

                    let is_active = active_notes.contains(&note);

                    let (rect, response) = ui.allocate_exact_size(Vec2::new(pad_w, pad_h), Sense::click());

                    let fill_color = if is_active || response.is_pointer_button_down_on() {
                        color
                    } else if response.hovered() {
                        color.linear_multiply(0.3)
                    } else {
                        Color32::from_rgb(25, 30, 42)
                    };

                    ui.painter().rect_filled(rect, 4.0, fill_color);
                    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0_f32, color.linear_multiply(0.4)));

                    let text_color = if is_active || response.is_pointer_button_down_on() {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(180, 190, 210)
                    };

                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(10.0),
                        text_color,
                    );

                    if response.clicked() {
                        triggers.push(KeyboardNoteTrigger { note, velocity: 100, is_note_on: true });
                    }
                }
            });
            ui.add_space(2.0);
        }
    });

    triggers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_grid_style_names() {
        assert_eq!(KeyboardGridStyle::PianoKeys.name(), "Piano Keys");
        assert_eq!(KeyboardGridStyle::PadMatrix.name(), "Pad Matrix (4x4)");
        assert_eq!(KeyboardGridStyle::all().len(), 2);
    }
}
