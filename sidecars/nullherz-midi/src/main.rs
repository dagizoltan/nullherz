use nullherz_traits::MidiEvent;
use ipc_layer::{SharedMemory, ShmRingBuffer, ShmProducer};
#[cfg(feature = "midir-backend")]
use midir::{MidiInput, Ignore};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("nullherz-midi bridge starting...");

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: nullherz-midi --shm <name> [--port <name>]");
        return Ok(());
    }

    let mut shm_name = "";
    let mut _port_filter = "";
    let mut map_path = "";

    for i in 0..args.len() {
        match args[i].as_str() {
            "--shm" if i + 1 < args.len() => shm_name = &args[i+1],
            "--port" if i + 1 < args.len() => _port_filter = &args[i+1],
            "--map" if i + 1 < args.len() => map_path = &args[i+1],
            _ => {}
        }
    }

    if shm_name.is_empty() {
        eprintln!("Error: --shm is required.");
        return Ok(());
    }

    // 1. Setup Shared Memory for MIDI Events
    let shm = SharedMemory::open(shm_name, 65536)?;
    let rb = unsafe { &*(shm.ptr() as *const ShmRingBuffer<MidiEvent>) };
    let producer = ShmProducer::new(rb);

    #[cfg(feature = "midir-backend")]
    {
        // 2. Setup midir
        let mut midi_in = MidiInput::new("Nullherz MIDI Bridge")?;
        midi_in.ignore(Ignore::None);

        let ports = midi_in.ports();
        if ports.is_empty() {
            eprintln!("No MIDI input ports found.");
        } else {
            let port = if _port_filter.is_empty() {
                println!("No port filter specified. Using first available port: {}", midi_in.port_name(&ports[0])?);
                &ports[0]
            } else {
                ports.iter()
                    .find(|p| midi_in.port_name(p).unwrap_or_default().contains(_port_filter))
                    .ok_or_else(|| format!("MIDI port matching '{}' not found.", _port_filter))?
            };

            println!("Connecting to MIDI port: {}", midi_in.port_name(port)?);

            if !map_path.is_empty() {
                println!("Loading MIDI map from: {}", map_path);
            }

            // 3. Start MIDI Loop
            let _conn = midi_in.connect(port, "nullherz-midi-input", move |stamp, message, _| {
                if message.len() >= 3 {
                    let event = MidiEvent {
                        timestamp_samples: stamp,
                        status: message[0],
                        data1: message[1],
                        data2: message[2],
                        _pad: 0,
                    };
                    if producer.push(event).is_err() {
                        eprintln!("MIDI Bridge: Overflow! Dropping event.");
                    }
                }
            }, ())?;

            println!("MIDI Bridge active. Press Ctrl+C to exit.");
        }
    }

    #[cfg(not(feature = "midir-backend"))]
    {
        println!("MIDI Bridge compiled without midir-backend. Running in Mock mode.");
        let mut tick = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            // Simulate a MIDI CC event for testing
            let event = MidiEvent {
                timestamp_samples: tick,
                status: 0xB0,
                data1: 1,
                data2: (tick % 127) as u8,
                _pad: 0,
            };
            let _ = producer.push(event);
            tick += 1;
        }
    }

    #[cfg(feature = "midir-backend")]
    {
        // Keep main thread alive
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
