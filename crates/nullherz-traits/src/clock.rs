
pub trait ClockProvider: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    /// Returns the current system monotonic time in nanoseconds.
    fn get_system_time_ns(&self) -> u64;
    /// Returns the synchronized hardware clock time in nanoseconds.
    fn get_device_time_ns(&self) -> u64;
    /// Returns the current estimated clock jitter in nanoseconds.
    fn get_estimated_jitter_ns(&self) -> u64;
    /// Calibrates the local clock against a remote master.
    fn synchronize_with_master(&self, master_time_ns: u64, round_trip_delay_ns: u64);
}

/// A standard implementation of ClockProvider using std::time::Instant.
/// Note: For Production Beta, this should be extended with so_timestamping
/// on Linux to support true PTP/IEEE 1588 hardware clock discipline.
pub struct SystemClockProvider {
    start_time: std::time::Instant,
}

impl SystemClockProvider {
    pub fn new() -> Self {
        Self { start_time: std::time::Instant::now() }
    }
}

impl Default for SystemClockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockProvider for SystemClockProvider {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn get_system_time_ns(&self) -> u64 {
        self.start_time.elapsed().as_nanos() as u64
    }

    fn get_device_time_ns(&self) -> u64 {
        // Fallback to system time until so_timestamping is integrated
        self.get_system_time_ns()
    }

    fn get_estimated_jitter_ns(&self) -> u64 {
        0 // Baseline jitter
    }

    fn synchronize_with_master(&self, _master_time_ns: u64, _round_trip_delay_ns: u64) {
        // Placeholder for PTP sync logic
    }
}

/// A high-precision ClockProvider using Linux SO_TIMESTAMPING.
pub struct PtpClockProvider {
    _socket_fd: std::os::unix::io::RawFd,
    offset_ns: std::sync::atomic::AtomicI64,
    servo: ClockServo,
}

impl PtpClockProvider {
    pub fn new(_interface: &str) -> std::io::Result<Self> {
        use nix::sys::socket::*;
        use std::os::unix::io::AsRawFd;

        let fd = socket(AddressFamily::Inet, SockType::Datagram, SockFlag::empty(), None)
            .map_err(std::io::Error::other)?;

        // Enable hardware and software timestamping
        let flags = TimestampingFlag::SOF_TIMESTAMPING_TX_HARDWARE
            | TimestampingFlag::SOF_TIMESTAMPING_TX_SOFTWARE
            | TimestampingFlag::SOF_TIMESTAMPING_RX_HARDWARE
            | TimestampingFlag::SOF_TIMESTAMPING_RX_SOFTWARE
            | TimestampingFlag::SOF_TIMESTAMPING_RAW_HARDWARE;

        setsockopt(&fd, sockopt::Timestamping, &flags)
            .map_err(std::io::Error::other)?;

        // Bind to interface (simplified for PTP example)
        let addr = std::net::SocketAddrV4::new(std::net::Ipv4Addr::new(0,0,0,0), 319);
        bind(fd.as_raw_fd(), &nix::sys::socket::SockaddrIn::from(addr)).map_err(std::io::Error::other)?;

        Ok(Self {
            _socket_fd: fd.as_raw_fd(),
            offset_ns: std::sync::atomic::AtomicI64::new(0),
            servo: ClockServo::default(),
        })
    }

    /// High-precision packet receive with SO_TIMESTAMPING extraction.
    pub fn recv_with_timestamp(&self, buf: &mut [u8]) -> std::io::Result<(usize, u64)> {
        #[cfg(target_os = "linux")]
        {
            let mut iov = libc::iovec {
                iov_base: buf.as_mut_ptr() as *mut libc::c_void,
                iov_len: buf.len(),
            };

            let mut control = [0u8; 512];
            let mut msg = libc::msghdr {
                msg_name: std::ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: &mut iov,
                msg_iovlen: 1,
                msg_control: control.as_mut_ptr() as *mut libc::c_void,
                msg_controllen: control.len() as _,
                msg_flags: 0,
            };

            let n = loop {
                let n = unsafe { libc::recvmsg(self._socket_fd, &mut msg, 0) };
                if n < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::Interrupted { continue; }
                    if err.kind() == std::io::ErrorKind::WouldBlock { continue; }
                    return Err(err);
                }
                break n;
            };

            let mut timestamp_ns = self.get_system_time_ns();

            unsafe {
                let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
                while !cmsg.is_null() {
                    if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_TIMESTAMPING {
                        let ts_ptr = libc::CMSG_DATA(cmsg) as *const libc::timespec;
                        // SCM_TIMESTAMPING returns 3 timespecs: [software, hw_transformed, hw_raw]
                        let ts_hw_raw = *ts_ptr.add(2);
                        let ts_sw = *ts_ptr.add(0);

                        if ts_hw_raw.tv_sec != 0 || ts_hw_raw.tv_nsec != 0 {
                            timestamp_ns = (ts_hw_raw.tv_sec as u64 * 1_000_000_000) + ts_hw_raw.tv_nsec as u64;
                        } else if ts_sw.tv_sec != 0 || ts_sw.tv_nsec != 0 {
                            timestamp_ns = (ts_sw.tv_sec as u64 * 1_000_000_000) + ts_sw.tv_nsec as u64;
                        }
                    }
                    cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
                }
            }
            Ok((n as usize, timestamp_ns))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let now = self.get_system_time_ns();
            let n = loop {
                let n = unsafe { libc::recv(self._socket_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };
                if n < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::Interrupted { continue; }
                    if err.kind() == std::io::ErrorKind::WouldBlock { continue; }
                    return Err(err);
                }
                break n;
            };
            Ok((n as usize, now))
        }
    }
}

impl ClockProvider for PtpClockProvider {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn get_system_time_ns(&self) -> u64 {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts); }
        (ts.tv_sec as u64 * 1_000_000_000) + ts.tv_nsec as u64
    }

    fn get_device_time_ns(&self) -> u64 {
        let sys = self.get_system_time_ns();
        let offset = self.offset_ns.load(std::sync::atomic::Ordering::Relaxed);
        (sys as i64 + offset) as u64
    }

    fn get_estimated_jitter_ns(&self) -> u64 {
        // In a real PTP stack, this would be calculated from the variance of offsets
        500
    }

    fn synchronize_with_master(&self, master_time_ns: u64, round_trip_delay_ns: u64) {
        let local_time = self.get_system_time_ns();
        // Basic PTP offset calculation: master_time + delay - local_arrival
        let raw_offset = (master_time_ns as i64 + (round_trip_delay_ns / 2) as i64) - local_time as i64;

        // Pass through servo for smoothing
        let disciplined_offset = self.servo.sample(raw_offset) as i64;
        self.offset_ns.store(disciplined_offset, std::sync::atomic::Ordering::Relaxed);
    }
}

/// A Proportional-Integral (PI) Clock Servo for smooth clock discipline.
/// Used to eliminate phase and frequency drift in distributed PTP systems.
pub struct ClockServo {
    ki: f64,
    kp: f64,
    integral: std::sync::atomic::AtomicU64, // bits representation of f64
    last_offset: std::sync::atomic::AtomicI64,
}

impl ClockServo {
    pub fn new(kp: f64, ki: f64) -> Self {
        Self {
            kp,
            ki,
            integral: std::sync::atomic::AtomicU64::new(0.0f64.to_bits()),
            last_offset: std::sync::atomic::AtomicI64::new(0),
        }
    }

    pub fn sample(&self, offset_ns: i64) -> f64 {
        let mut integral = f64::from_bits(self.integral.load(std::sync::atomic::Ordering::Relaxed));

        // Stage 2 PI Controller:
        // Disciplines the system clock frequency by integrating the phase error.
        integral += offset_ns as f64 * self.ki;

        // Anti-windup clamping (1ms max integral correction)
        integral = integral.clamp(-1_000_000.0, 1_000_000.0);

        self.integral.store(integral.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.last_offset.store(offset_ns, std::sync::atomic::Ordering::Relaxed);

        // Proportional + Integral output
        (offset_ns as f64 * self.kp) + integral
    }

    pub fn reset(&self) {
        self.integral.store(0.0f64.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.last_offset.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for ClockServo {
    fn default() -> Self {
        Self::new(0.1, 0.01)
    }
}

#[cfg(all(feature = "kani-verify", kani))]
mod clock_verification {
    use super::*;

    #[kani::proof]
    pub fn prove_clock_servo_integral_clamping() {
        let servo = ClockServo::new(0.1, 0.01);

        // Push a very large offset repeatedly
        for _ in 0..10 {
            let offset: i64 = kani::any();
            // We only care about large values for overflow testing
            kani::assume(offset > 1_000_000_000);
            servo.sample(offset);
        }

        let integral = f64::from_bits(servo.integral.load(std::sync::atomic::Ordering::Relaxed));
        kani::assert(integral <= 1_000_000.0, "Integral must be clamped to prevent windup");
        kani::assert(integral >= -1_000_000.0, "Integral must be clamped to prevent windup");
    }
}
