use std::net::UdpSocket;
use std::sync::Arc;
use nullherz_traits::ClockProvider;

pub struct PtpEngine {
    clock: Arc<dyn ClockProvider>,
    socket: UdpSocket,
    is_master: bool,
}

impl PtpEngine {
    pub fn new(clock: Arc<dyn ClockProvider>, port: u16, is_master: bool) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
        socket.set_broadcast(true)?;
        Ok(Self { clock, socket, is_master })
    }

    pub fn run_loop(&self) {
        let mut buf = [0u8; 128];
        loop {
            if self.is_master {
                // Send Sync message
                let now = self.clock.get_device_time_ns();
                let msg = now.to_le_bytes();
                let _ = self.socket.send_to(&msg, "255.255.255.255:319");
                std::thread::sleep(std::time::Duration::from_millis(1000));
            } else {
                // Receive Sync message
                // If it's a PtpClockProvider, use high-precision receipt
                let recv_res = if let Some(ptp) = self.clock.as_any().downcast_ref::<nullherz_traits::PtpClockProvider>() {
                    ptp.recv_with_timestamp(&mut buf).map(|(len, ts)| (len, ts))
                } else {
                    self.socket.recv_from(&mut buf).map(|(len, _)| (len, self.clock.get_system_time_ns()))
                };

                if let Ok((len, arrival_ts)) = recv_res {
                    if len == 8 {
                        let master_time = u64::from_le_bytes(buf[..8].try_into().unwrap());
                        // Basic PTP synchronization
                        self.clock.synchronize_with_master(master_time, 1_000_000); // 1ms wire delay assumption

                        let current = self.clock.get_device_time_ns();
                        if arrival_ts > 0 {
                             println!("PTP: Sync received at {} ns. Master: {}. Local: {}. Offset: {} ns",
                                arrival_ts, master_time, current, (current as i64 - master_time as i64));
                        }
                    }
                }
            }
        }
    }
}
