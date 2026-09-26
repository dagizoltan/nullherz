use egui::Ui;
use audio_core::Telemetry;
use crate::state::{AudioNervousSystem, VisualGenome};

pub mod radial_mandala;
pub mod liquid_surface;
pub mod spectral_landscape;
pub mod hyper_attractor;
pub mod reaction_diffusion;
pub mod neural_raymarcher;
pub mod neural_nca_mesh;

pub trait NeuralVisualEngine {
    fn prepare_tensor_inputs(&mut self, nervous: &AudioNervousSystem, genome: &VisualGenome) -> Vec<f32>;

    fn render(
        &mut self,
        ui: &mut Ui,
        rect: egui::Rect,
        nervous: &AudioNervousSystem,
        genome: &VisualGenome,
        telemetry: &Option<Telemetry>,
        time: f32,
    );
}
