use egui::{Ui, Frame, RichText};
use crate::{InspectorApp, View};

pub fn render_preferences(app: &mut InspectorApp, ui: &mut Ui) {
    let theme = app.theme;
    ui.strong("App Preferences");
    ui.add_space(theme.space_xs);

    // Startup Behavior Group
    ui.label(RichText::new("STARTUP BEHAVIOR").small().strong().color(theme.text_secondary));
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            // Restore session (disabled/mocked)
            ui.add_enabled_ui(false, |ui| {
                ui.checkbox(&mut app.settings.restore_last_session, "Restore last session on launch")
                    .on_hover_text("Planned / Not yet functional: actual session persistence is under development");
            });
            ui.add_space(theme.space_sm);

            // Default view on launch
            ui.horizontal(|ui| {
                ui.label("Default view on launch:");
                egui::ComboBox::from_id_source("default_view_select")
                    .selected_text(view_label(app.settings.default_view_on_launch))
                    .show_ui(ui, |ui| {
                        let views = [
                            View::Console,
                            View::Player,
                            View::Composer,
                            View::Editor,
                            View::Sampler,
                            View::Breeder,
                            View::Broadcast,
                            View::Topology,
                            View::Account,
                            View::Settings,
                        ];
                        for v in views {
                            ui.selectable_value(&mut app.settings.default_view_on_launch, v, view_label(v));
                        }
                    });
            });
        });

    ui.add_space(theme.space_md);

    // Autosave Settings Group
    ui.label(RichText::new("AUTOSAVE SETTINGS").small().strong().color(theme.text_secondary));
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.checkbox(&mut app.settings.autosave_enabled, "Enable background autosave");
            ui.add_space(theme.space_sm);
            ui.horizontal(|ui| {
                ui.label("Save Interval (minutes):");
                ui.add(egui::Slider::new(&mut app.settings.autosave_interval_mins, 1..=30).show_value(true));
            });
        });

    ui.add_space(theme.space_md);

    // Keyboard Shortcuts Group
    ui.label(RichText::new("KEYBOARD SHORTCUTS").small().strong().color(theme.text_secondary));
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.checkbox(&mut app.settings.shortcuts_enabled, "Enable keyboard shortcuts");
            ui.add_space(theme.space_sm);

            ui.label(RichText::new("Reference List").strong().color(theme.text_secondary));
            ui.add_space(theme.space_xs);

            egui::Grid::new("shortcuts_reference_grid")
                .num_columns(2)
                .spacing([30.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Shortcut").strong());
                    ui.label(RichText::new("Action").strong());
                    ui.end_row();

                    ui.label(RichText::new("Space").code());
                    ui.label("Toggle play/stop transport");
                    ui.end_row();

                    ui.label(RichText::new("Ctrl / Cmd + S").code());
                    ui.label("Save system configuration");
                    ui.end_row();

                    ui.label(RichText::new("1").code());
                    ui.label("Switch to Media Player");
                    ui.end_row();

                    ui.label(RichText::new("2").code());
                    ui.label("Switch to DJ Console");
                    ui.end_row();

                    ui.label(RichText::new("3").code());
                    ui.label("Switch to Composer");
                    ui.end_row();

                    ui.label(RichText::new("4").code());
                    ui.label("Switch to Editor");
                    ui.end_row();

                    ui.label(RichText::new("5").code());
                    ui.label("Switch to Sampler");
                    ui.end_row();

                    ui.label(RichText::new("6").code());
                    ui.label("Switch to DNA Breeder");
                    ui.end_row();

                    ui.label(RichText::new("7").code());
                    ui.label("Switch to Broadcast");
                    ui.end_row();

                    ui.label(RichText::new("8").code());
                    ui.label("Switch to Topology");
                    ui.end_row();

                    ui.label(RichText::new("9").code());
                    ui.label("Switch to Account");
                    ui.end_row();
                });
        });

    ui.add_space(theme.space_md);

    // Theme Customizer Group
    ui.label(RichText::new("THEME CUSTOMIZER").small().strong().color(theme.text_secondary));
    ui.add_space(theme.space_xs);
    Frame::none()
        .fill(theme.bg_surface)
        .rounding(theme.radius_md)
        .stroke(theme.border_stroke)
        .inner_margin(theme.space_md)
        .show(ui, |ui| {
            ui.label("Shift Accent, Success, and Danger colors, adjust rounding scales, or toggle shadow weights dynamically in real-time:");
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                ui.label("Accent Color:");
                ui.color_edit_button_srgba(&mut app.theme.accent);

                ui.add_space(theme.space_md);
                ui.label("Success Color:");
                ui.color_edit_button_srgba(&mut app.theme.success);

                ui.add_space(theme.space_md);
                ui.label("Danger Color:");
                ui.color_edit_button_srgba(&mut app.theme.danger);
            });
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                ui.label("Border Color:");
                if ui.color_edit_button_srgba(&mut app.theme.border).changed() {
                    app.theme.border_stroke.color = app.theme.border;
                }
            });
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                ui.label("Small Rounding:");
                ui.add(egui::Slider::new(&mut app.theme.radius_sm, 0.0..=12.0).show_value(true));
            });
            ui.horizontal(|ui| {
                ui.label("Card Rounding:");
                ui.add(egui::Slider::new(&mut app.theme.radius_md, 0.0..=24.0).show_value(true));
            });
            ui.horizontal(|ui| {
                ui.label("Panel Rounding:");
                ui.add(egui::Slider::new(&mut app.theme.radius_lg, 0.0..=36.0).show_value(true));
            });
            ui.add_space(theme.space_sm);

            ui.horizontal(|ui| {
                ui.label("Shadow Blur (Medium):");
                ui.add(egui::Slider::new(&mut app.theme.shadow_md.blur, 0.0..=30.0).show_value(true));
            });
            ui.horizontal(|ui| {
                ui.label("Shadow Y Offset:");
                ui.add(egui::Slider::new(&mut app.theme.shadow_md.offset.y, 0.0..=10.0).show_value(true));
            });
        });
}

pub fn view_label(view: View) -> &'static str {
    match view {
        View::Console => "DJ Console",
        View::Player => "Media Player",
        View::Composer => "Composer",
        View::Editor => "Editor",
        View::Sampler => "Sampler",
        View::Breeder => "DNA Breeder",
        View::Broadcast => "Broadcast",
        View::Topology => "Topology",
        View::Account => "Account",
        View::Settings => "Settings",
        _ => "Other",
    }
}
