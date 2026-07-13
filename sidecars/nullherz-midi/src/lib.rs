use nullherz_traits::MidiEvent;
use ipc_layer::Producer;

pub trait MidiSource: Send {
    fn read_event(&mut self) -> Option<MidiEvent>;
}

pub struct MidiBridge {
    pub producer: Option<Producer<MidiEvent>>,
}

impl Default for MidiBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiBridge {
    pub fn new() -> Self {
        Self { producer: None }
    }

    pub fn handle_event(&mut self, event: MidiEvent) {
        if let Some(ref mut prod) = self.producer {
            let _ = prod.push(event);
        }
    }
}

#[cfg(feature = "midir-backend")]
pub mod midir_backend {
    use super::*;
    use midir::{MidiInput, MidiInputConnection};
    use std::error::Error;

    pub struct MidirSource {
        _connection: MidiInputConnection<()>,
    }

    impl MidirSource {
        pub fn connect(port_name: &str, mut bridge: MidiBridge) -> Result<Self, Box<dyn Error>> {
            let midi_in = MidiInput::new("Nullherz MIDI Bridge")?;
            let ports = midi_in.ports();
            let port = ports.iter()
                .find(|p| midi_in.port_name(p).unwrap_or_default().contains(port_name))
                .ok_or_else(|| format!("MIDI port not found: {}", port_name))?;

            let conn = midi_in.connect(port, "nullherz-input", move |stamp, message, _| {
                if message.len() >= 3 {
                    bridge.handle_event(MidiEvent {
                        timestamp_samples: stamp,
                        status: message[0],
                        data1: message[1],
                        data2: message[2],
                        _pad: 0,
                    });
                }
            }, ())?;

            Ok(Self { _connection: conn })
        }
    }
}
