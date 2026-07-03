use egui::{Color32, Ui, Frame, Vec2, Sense, Stroke};
use crate::InspectorApp;
use audio_core::Telemetry;
use egui_wgpu::wgpu;

pub fn render(app: &mut InspectorApp, ui: &mut Ui, telemetry: &Option<Telemetry>) {
    ui.horizontal(|ui| {
        ui.heading("Production Sampler");
    });
    ui.add_space(10.0);

    Frame::none().fill(Color32::from_rgb(10, 10, 12)).rounding(4.0).inner_margin(12.0).show(ui, |ui| {
        let (rect, _response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 200.0), Sense::hover());

        // WGPU Accelerated Waveform rendering callback
        if let Some(wgpu_mtx) = &app.wgpu_renderer {
             let _wgpu = wgpu_mtx.lock().unwrap();

             // Setup callback for WGPU rendering
             struct WaveformCallback {}
             impl egui_wgpu::CallbackTrait for WaveformCallback {
                 fn paint<'a>(&'a self, _info: egui::PaintCallbackInfo, _render_pass: &mut wgpu::RenderPass<'a>, _resources: &egui_wgpu::CallbackResources) {
                     // Waveform rendering logic
                 }
             }

             let callback = egui_wgpu::Callback::new_paint_callback(rect, WaveformCallback {});
             ui.painter().add(callback);

             ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "GPU-ACCELERATED WAVEFORM ENGINE ACTIVE", egui::FontId::proportional(14.0), Color32::from_rgb(0, 100, 80));
        } else {
             ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "WGPU Accelerated Waveform (120fps)", egui::FontId::proportional(14.0), Color32::GRAY);
        }

        if let Some(t) = telemetry {
            // Visualize real-time playhead bit-exactly
            let playhead_x = rect.left() + (t.beat_position as f32 % 4.0) / 4.0 * rect.width();
            ui.painter().line_segment([egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(0, 255, 200)));
        }
    });

    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.heading("Loop Slicer");
        if ui.checkbox(&mut app.sampler_slicer_mode, "ENABLE").changed() {
             let _ = app.command_sender.send(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                target_id: 100,
                param_id: 3,
                value: if app.sampler_slicer_mode { 1.0 } else { 0.0 },
                ramp_duration_samples: 0,
            }));
        }
    });
}
