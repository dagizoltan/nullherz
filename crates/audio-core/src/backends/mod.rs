pub mod alsa;
pub mod pipewire;
pub mod jack;
pub mod threaded;

pub use alsa::AlsaBackend;
pub use pipewire::PipewireBackend;
pub use jack::JackBackend;
pub use threaded::ThreadedBackend;

use crate::engine::AudioEngine;

pub trait AudioBackend: Send {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String>;
    fn stop(&mut self) -> Option<AudioEngine>;
}
