use audio_core::Telemetry;
use control_plane::TimestampedCommand;
use ipc_layer::{Consumer, Producer};
use std::net::TcpListener;
use std::thread;
use tungstenite::accept;
use serde_json;

pub struct Bridge {
    telemetry_consumer: Consumer<Telemetry>,
    command_producer: Producer<TimestampedCommand>,
}

impl Bridge {
    pub fn new(
        telemetry_consumer: Consumer<Telemetry>,
        command_producer: Producer<TimestampedCommand>,
    ) -> Self {
        Self {
            telemetry_consumer,
            command_producer,
        }
    }

    pub fn run(&mut self, addr: &str) -> Result<(), String> {
        let server = TcpListener::bind(addr).map_err(|e| e.to_string())?;
        println!("WebSocket Bridge listening on: {}", addr);

        for stream in server.incoming() {
            let mut telemetry_consumer = self.telemetry_consumer.clone();
            let mut command_producer = self.command_producer.clone();

            thread::spawn(move || {
                let mut websocket = accept(stream.unwrap()).unwrap();
                println!("New WebSocket connection");

                loop {
                    // 1. Broadcast Telemetry (limit per loop to avoid blocking other tasks)
                    let mut count = 0;
                    while let Some(tel) = telemetry_consumer.pop() {
                        let json = serde_json::to_string(&tel).unwrap();
                        if let Err(_) = websocket.send(tungstenite::Message::Text(json)) {
                            return;
                        }
                        count += 1;
                        if count > 10 { break; }
                    }

                    // 2. Forward Commands
                    if websocket.can_read() {
                        match websocket.read() {
                            Ok(msg) => {
                                if let tungstenite::Message::Text(text) = msg {
                                    if let Ok(cmd) = serde_json::from_str::<TimestampedCommand>(&text) {
                                        let _ = command_producer.push(cmd);
                                    }
                                }
                            }
                            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => return,
                        }
                    }

                    thread::sleep(std::time::Duration::from_millis(10));
                }
            });
        }
        Ok(())
    }
}
