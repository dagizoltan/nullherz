pub mod alsa;
pub mod pipewire;
pub mod jack;
pub mod threaded;

pub use alsa::AlsaBackend;
pub use pipewire::PipewireBackend;
pub use jack::JackBackend;
pub use threaded::ThreadedBackend;

use audio_core::AudioEngine;

use std::sync::{Arc, Mutex};

pub trait AudioBackend: Send {
    fn start(&mut self, engine: Arc<Mutex<Option<audio_core::AudioEngine>>>) -> Result<(), String>;
    fn stop(&mut self);
}
