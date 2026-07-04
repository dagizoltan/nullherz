use egui::{Ui, Color32, RichText};
use crate::InspectorApp;

pub fn render(app: &mut InspectorApp, ui: &mut Ui) {
    ui.heading("Alpha Setup Wizard");
    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(RichText::new("Welcome to Nullherz Alpha. Please configure your hardware environment below.").color(Color32::from_gray(180)));
        ui.add_space(20.0);

        ui.strong("1. Audio Backend");
        ui.horizontal(|ui| {
            ui.label("Current Selection:");
            ui.label(RichText::new("ALSA (Optimized)").color(Color32::from_rgb(0, 255, 200)));
        });
        if ui.button("Scan for Backends").clicked() {
             // Bridge to nullherz-setup logic (mocked for UI)
             println!("Scanning for audio backends...");
        }
        ui.add_space(10.0);

        ui.strong("2. MIDI Hardware");
        ui.label("Detecting controllers...");
        ui.group(|ui| {
            ui.label("• Pioneer DDJ-400 (Attached)");
            ui.label("• Generic MIDI Keyboard (Attached)");
        });
        if ui.button("Refresh MIDI List").clicked() {
             println!("Refreshing MIDI device list...");
        }
        ui.add_space(10.0);

        ui.strong("3. Distributed DSP Discovery");
        ui.label("Searching for remote sidecar nodes on local network...");
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("• 192.168.1.45 (Studio-PC-2)");
                if ui.button("ATTACH").clicked() { println!("Attaching to remote node..."); }
            });
            ui.horizontal(|ui| {
                ui.label("• 192.168.1.12 (MacBook-Pro-DSP)");
                if ui.button("ATTACH").clicked() { println!("Attaching to remote node..."); }
            });
        });

        ui.add_space(30.0);
        if ui.button(RichText::new("FINALIZE CONFIGURATION").strong().size(18.0)).clicked() {
            println!("Configuration saved to system_config.json");
            app.active_view = crate::View::Console;
        }
    });
}
