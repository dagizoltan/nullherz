use serde::{Serialize, Deserialize};
use std::fs;
use midir::MidiInput;
use cpal::traits::HostTrait;

#[derive(Serialize, Deserialize, Debug)]
pub struct SystemConfig {
    pub audio_backend: String,
    pub midi_ports: Vec<String>,
    pub sample_rate: u32,
    pub block_size: u32,
}

fn main() {
    println!("--- nullherz Alpha Setup Wizard ---");

    let mut config = SystemConfig {
        audio_backend: "ALSA".to_string(),
        midi_ports: Vec::new(),
        sample_rate: 44100,
        block_size: 256,
    };

    // 1. Detect Audio Backends
    println!("\n[1/3] Detecting Audio Backends...");
    let hosts = cpal::available_hosts();
    println!("Available audio hosts: {:?}", hosts);
    if let Some(host) = hosts.first() {
        config.audio_backend = format!("{:?}", host);
        println!("Selected default backend: {}", config.audio_backend);
    }

    // 2. Scan for MIDI Hardware
    println!("\n[2/3] Scanning for MIDI Hardware...");
    if let Ok(midi_in) = MidiInput::new("nullherz-setup") {
        let ports = midi_in.ports();
        for port in ports {
            if let Ok(name) = midi_in.port_name(&port) {
                println!("Found MIDI Input: {}", name);
                config.midi_ports.push(name);
            }
        }
    }
    if config.midi_ports.is_empty() {
        println!("No MIDI hardware detected. MIDI bridge will run in Mock mode.");
    }

    // 3. Save Configuration
    println!("\n[3/3] Saving Configuration...");
    let json = serde_json::to_string_pretty(&config).unwrap();
    if let Err(e) = fs::write("system_config.json", json) {
        eprintln!("Failed to save system_config.json: {}", e);
    } else {
        println!("system_config.json generated successfully.");
    }

    println!("\nSetup Complete. You can now start nullherz-conductor.");
}
