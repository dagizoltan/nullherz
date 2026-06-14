pub const SIDECAR_PROTOCOL_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handshake {
    pub version: u64,
    pub num_channels: u32,
    pub magic: u32,
}

impl Handshake {
    pub fn new(num_channels: u32) -> Self {
        Self {
            version: SIDECAR_PROTOCOL_VERSION,
            num_channels,
            magic: 0xDEADBEEF,
        }
    }

    pub fn verify(&self, expected_channels: u32) -> Result<(), String> {
        if self.magic != 0xDEADBEEF {
            return Err("Invalid magic number".into());
        }
        if self.version != SIDECAR_PROTOCOL_VERSION {
            return Err(format!("Protocol version mismatch: engine={}, sidecar={}", SIDECAR_PROTOCOL_VERSION, self.version));
        }
        if self.num_channels != expected_channels {
            return Err(format!("Channel count mismatch: engine={}, sidecar={}", expected_channels, self.num_channels));
        }
        Ok(())
    }
}
