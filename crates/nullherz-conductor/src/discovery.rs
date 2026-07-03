use std::net::UdpSocket;
use std::time::Duration;

pub struct DiscoveryBeacon {
    pub port: u16,
    pub service_name: String,
}

impl DiscoveryBeacon {
    pub fn new(port: u16, name: &str) -> Self {
        Self { port, service_name: name.to_string() }
    }

    pub fn start_broadcast(self) {
        tokio::spawn(async move {
            let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP socket");
            socket.set_broadcast(true).expect("Failed to set UDP broadcast");

            let msg = format!("nullherz_conductor:{}", self.port);
            let addr = "255.255.255.255:9001";

            loop {
                let _ = socket.send_to(msg.as_bytes(), addr);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }
}
