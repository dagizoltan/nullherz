// Non-RT plane (clock-sync beacon pacing): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use nullherz_traits::ClockProvider;

/// Wire protocol (UDP, little-endian):
///   SYNC       [0x01][t1: u64]              master -> broadcast, t1 = master send time
///   DELAY_REQ  [0x02][req_id: u64]          slave  -> master
///   DELAY_RESP [0x03][req_id: u64][t4: u64] master -> slave, t4 = master arrival time of the REQ
/// A bare 8-byte payload is accepted as a legacy SYNC (pre-typed protocol).
const MSG_SYNC: u8 = 0x01;
const MSG_DELAY_REQ: u8 = 0x02;
const MSG_DELAY_RESP: u8 = 0x03;

/// Fallback round-trip assumption used only until the first Delay_Req/Delay_Resp
/// exchange completes (and for legacy 8-byte SYNC peers).
const FALLBACK_RTT_NS: u64 = 1_000_000;

/// Reject implausible measurements: anything above this is treated as a
/// scheduling/queueing spike and skipped rather than fed to the servo.
const MAX_PLAUSIBLE_RTT_NS: i64 = 100_000_000; // 100 ms

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtpMessage {
    Sync { t1: u64 },
    DelayReq { req_id: u64 },
    DelayResp { req_id: u64, t4: u64 },
}

impl PtpMessage {
    pub fn parse(buf: &[u8]) -> Option<PtpMessage> {
        match buf {
            // Legacy: bare u64 master timestamp
            b if b.len() == 8 => Some(PtpMessage::Sync { t1: u64::from_le_bytes(b.try_into().ok()?) }),
            [MSG_SYNC, rest @ ..] if rest.len() == 8 => {
                Some(PtpMessage::Sync { t1: u64::from_le_bytes(rest.try_into().ok()?) })
            }
            [MSG_DELAY_REQ, rest @ ..] if rest.len() == 8 => {
                Some(PtpMessage::DelayReq { req_id: u64::from_le_bytes(rest.try_into().ok()?) })
            }
            [MSG_DELAY_RESP, rest @ ..] if rest.len() == 16 => Some(PtpMessage::DelayResp {
                req_id: u64::from_le_bytes(rest[..8].try_into().ok()?),
                t4: u64::from_le_bytes(rest[8..].try_into().ok()?),
            }),
            _ => None,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            PtpMessage::Sync { t1 } => {
                let mut v = vec![MSG_SYNC];
                v.extend_from_slice(&t1.to_le_bytes());
                v
            }
            PtpMessage::DelayReq { req_id } => {
                let mut v = vec![MSG_DELAY_REQ];
                v.extend_from_slice(&req_id.to_le_bytes());
                v
            }
            PtpMessage::DelayResp { req_id, t4 } => {
                let mut v = vec![MSG_DELAY_RESP];
                v.extend_from_slice(&req_id.to_le_bytes());
                v.extend_from_slice(&t4.to_le_bytes());
                v
            }
        }
    }
}

/// Slave-side sync state across the SYNC -> DELAY_REQ -> DELAY_RESP exchange.
#[derive(Default)]
struct SlaveState {
    /// (t1 master send, t2 slave arrival) of the most recent SYNC.
    last_sync: Option<(u64, u64)>,
    /// (req_id, t3 slave send) of the in-flight DELAY_REQ.
    pending_req: Option<(u64, u64)>,
    next_req_id: u64,
    /// EMA-filtered round-trip time; None until first successful exchange.
    filtered_rtt_ns: Option<u64>,
}

/// Offset-free round-trip from the four PTP timestamps:
/// rtt = (t2 - t1) + (t4 - t3). The unknown clock offset appears with opposite
/// signs in the two terms and cancels; only the true path delay remains.
/// Returns None for corrupt/implausible timestamp sets.
pub fn compute_rtt_ns(t1: u64, t2: u64, t3: u64, t4: u64) -> Option<u64> {
    let leg_a = (t2 as i64).wrapping_sub(t1 as i64);
    let leg_b = (t4 as i64).wrapping_sub(t3 as i64);
    let rtt = leg_a.checked_add(leg_b)?;
    if !(0..=MAX_PLAUSIBLE_RTT_NS).contains(&rtt) {
        return None;
    }
    Some(rtt as u64)
}

/// 1/8 exponential moving average, matching common PTP daemon practice:
/// smooth enough to reject scheduling jitter, fast enough to track route changes.
pub fn ema_rtt(prev: Option<u64>, sample: u64) -> u64 {
    match prev {
        None => sample,
        Some(p) => (p * 7 + sample) / 8,
    }
}

pub struct PtpEngine {
    clock: Arc<dyn ClockProvider>,
    socket: UdpSocket,
    is_master: bool,
    /// Where the master broadcasts SYNC (tests point this at loopback).
    broadcast_addr: String,
    state: SlaveState,
}

impl PtpEngine {
    pub fn new(clock: Arc<dyn ClockProvider>, port: u16, is_master: bool) -> std::io::Result<Self> {
        // SO_REUSEADDR/SO_REUSEPORT: PtpClockProvider may already hold a socket
        // bound to this port for SO_TIMESTAMPING; without reuse the engine's own
        // bind fails with EADDRINUSE and the sync loop silently never runs.
        use socket2::{Domain, Protocol, Socket, Type};
        let raw = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        raw.set_reuse_address(true)?;
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        raw.set_reuse_port(true)?;
        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().expect("static addr is valid");
        raw.bind(&addr.into())?;
        let socket: UdpSocket = raw.into();
        socket.set_broadcast(true)?;
        socket.set_read_timeout(Some(std::time::Duration::from_millis(200)))?;
        Ok(Self {
            clock,
            socket,
            is_master,
            broadcast_addr: format!("255.255.255.255:{}", port),
            state: SlaveState::default(),
        })
    }

    /// Point SYNC broadcasts somewhere specific (loopback in tests).
    pub fn with_broadcast_addr(mut self, addr: &str) -> Self {
        self.broadcast_addr = addr.to_string();
        self
    }

    /// The measured, EMA-filtered round-trip time, if at least one
    /// Delay_Req/Delay_Resp exchange has completed.
    pub fn measured_rtt_ns(&self) -> Option<u64> {
        self.state.filtered_rtt_ns
    }

    pub fn run_loop(mut self) {
        let mut buf = [0u8; 64];
        let mut last_beacon = std::time::Instant::now() - std::time::Duration::from_secs(1);
        loop {
            if self.is_master {
                if last_beacon.elapsed() >= std::time::Duration::from_secs(1) {
                    self.send_sync();
                    last_beacon = std::time::Instant::now();
                }
                // Between beacons, answer DELAY_REQs (read timeout paces the loop).
                if let Ok((len, src)) = self.socket.recv_from(&mut buf) {
                    self.handle_packet_as_master(&buf[..len], src);
                }
            } else if let Ok((len, src)) = self.socket.recv_from(&mut buf) {
                self.handle_packet_as_slave(&buf[..len], src);
            }
        }
    }

    fn send_sync(&self) {
        let t1 = self.clock.get_device_time_ns();
        let _ = self.socket.send_to(&PtpMessage::Sync { t1 }.encode(), &self.broadcast_addr);
    }

    fn handle_packet_as_master(&self, buf: &[u8], src: SocketAddr) {
        if let Some(PtpMessage::DelayReq { req_id }) = PtpMessage::parse(buf) {
            // t4 is stamped as close to arrival as this software path allows.
            let t4 = self.clock.get_device_time_ns();
            let _ = self.socket.send_to(&PtpMessage::DelayResp { req_id, t4 }.encode(), src);
        }
    }

    fn handle_packet_as_slave(&mut self, buf: &[u8], src: SocketAddr) {
        match PtpMessage::parse(buf) {
            Some(PtpMessage::Sync { t1 }) => {
                let t2 = self.clock.get_system_time_ns();
                self.state.last_sync = Some((t1, t2));

                // Kick off the path-delay measurement for this sync cycle.
                let req_id = self.state.next_req_id;
                self.state.next_req_id = self.state.next_req_id.wrapping_add(1);
                let t3 = self.clock.get_system_time_ns();
                if self.socket.send_to(&PtpMessage::DelayReq { req_id }.encode(), src).is_ok() {
                    self.state.pending_req = Some((req_id, t3));
                }

                // Discipline immediately with the best delay estimate we have;
                // the DELAY_RESP for this cycle refines the next one.
                let rtt = self.state.filtered_rtt_ns.unwrap_or(FALLBACK_RTT_NS);
                self.clock.synchronize_with_master(t1, rtt);

                let current = self.clock.get_device_time_ns();
                println!(
                    "PTP: Sync received. Master: {}. Local: {}. Offset: {} ns. RTT: {} ns ({})",
                    t1,
                    current,
                    (current as i64 - t1 as i64),
                    rtt,
                    if self.state.filtered_rtt_ns.is_some() { "measured" } else { "assumed" },
                );
            }
            Some(PtpMessage::DelayResp { req_id, t4 }) => {
                let (Some((t1, t2)), Some((pending_id, t3))) = (self.state.last_sync, self.state.pending_req) else {
                    return;
                };
                if pending_id != req_id {
                    return; // stale response from an earlier cycle
                }
                self.state.pending_req = None;
                if let Some(rtt) = compute_rtt_ns(t1, t2, t3, t4) {
                    let filtered = ema_rtt(self.state.filtered_rtt_ns, rtt);
                    self.state.filtered_rtt_ns = Some(filtered);
                    // Re-discipline with the fresh measurement.
                    self.clock.synchronize_with_master(t1, filtered);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn test_message_roundtrip_and_legacy() {
        for msg in [
            PtpMessage::Sync { t1: 42 },
            PtpMessage::DelayReq { req_id: 7 },
            PtpMessage::DelayResp { req_id: 7, t4: u64::MAX },
        ] {
            assert_eq!(PtpMessage::parse(&msg.encode()), Some(msg));
        }
        // Legacy bare-u64 SYNC still parses
        assert_eq!(PtpMessage::parse(&99u64.to_le_bytes()), Some(PtpMessage::Sync { t1: 99 }));
        assert_eq!(PtpMessage::parse(&[0xFF, 1, 2]), None);
        assert_eq!(PtpMessage::parse(&[]), None);
    }

    #[test]
    fn test_rtt_is_offset_free() {
        // True path delay 300ns each way; slave clock is 1_000_000ns AHEAD of master.
        let offset: i64 = 1_000_000;
        let t1: u64 = 5_000_000; // master sends SYNC
        let t2 = (t1 as i64 + 300 + offset) as u64; // slave receives
        let t3 = t2 + 50; // slave sends DELAY_REQ
        let t4 = (t3 as i64 + 300 - offset) as u64; // master receives
        assert_eq!(compute_rtt_ns(t1, t2, t3, t4), Some(600));

        // Same delays with the slave BEHIND the master must give the same answer.
        let offset: i64 = -1_000_000;
        let t2 = (t1 as i64 + 300 + offset) as u64;
        let t3 = t2 + 50;
        let t4 = (t3 as i64 + 300 - offset) as u64;
        assert_eq!(compute_rtt_ns(t1, t2, t3, t4), Some(600));
    }

    #[test]
    fn test_rtt_rejects_implausible() {
        // Negative (corrupt timestamps)
        assert_eq!(compute_rtt_ns(1000, 500, 600, 100), None);
        // Above the plausibility ceiling
        assert_eq!(compute_rtt_ns(0, 200_000_000, 0, 1), None);
    }

    #[test]
    fn test_ema_filter() {
        assert_eq!(ema_rtt(None, 800), 800);
        assert_eq!(ema_rtt(Some(800), 800), 800);
        // Converges toward a persistent change
        let mut v = 800;
        for _ in 0..40 {
            v = ema_rtt(Some(v), 1600);
        }
        assert!(v > 1500, "EMA should converge toward the new level, got {}", v);
        // A single spike moves the estimate by only 1/8
        assert_eq!(ema_rtt(Some(800), 8800), 1800);
    }

    /// Deterministic scripted clock for exchange tests.
    struct ScriptClock {
        now: AtomicU64,
        last_sync: parking_lot::Mutex<Option<(u64, u64)>>,
    }
    impl ScriptClock {
        fn new(start: u64) -> Self {
            Self { now: AtomicU64::new(start), last_sync: parking_lot::Mutex::new(None) }
        }
        fn advance(&self, ns: u64) {
            self.now.fetch_add(ns, Ordering::SeqCst);
        }
    }
    impl ClockProvider for ScriptClock {
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn get_system_time_ns(&self) -> u64 { self.now.load(Ordering::SeqCst) }
        fn get_device_time_ns(&self) -> u64 { self.now.load(Ordering::SeqCst) }
        fn get_estimated_jitter_ns(&self) -> u64 { 0 }
        fn synchronize_with_master(&self, master_time_ns: u64, round_trip_delay_ns: u64) {
            *self.last_sync.lock() = Some((master_time_ns, round_trip_delay_ns));
        }
    }

    #[test]
    fn test_slave_measures_delay_over_loopback() {
        // Slave on an ephemeral port; a scripted "master" socket talks to it directly.
        let master_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let slave_clock = Arc::new(ScriptClock::new(10_000_000));

        let slave_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let slave_addr = slave_sock.local_addr().unwrap();
        slave_sock.set_read_timeout(Some(std::time::Duration::from_millis(500))).unwrap();
        let mut slave = PtpEngine {
            clock: slave_clock.clone() as Arc<dyn ClockProvider>,
            socket: slave_sock,
            is_master: false,
            broadcast_addr: String::new(),
            state: SlaveState::default(),
        };

        // Master sends SYNC with t1
        let t1 = 20_000_000u64;
        master_sock.send_to(&PtpMessage::Sync { t1 }.encode(), slave_addr).unwrap();

        // Slave processes the SYNC (records t2, emits DELAY_REQ, disciplines with fallback)
        let mut buf = [0u8; 64];
        let (len, src) = slave.socket.recv_from(&mut buf).unwrap();
        slave.handle_packet_as_slave(&buf[..len], src);
        assert!(slave.measured_rtt_ns().is_none());
        assert_eq!(*slave_clock.last_sync.lock(), Some((t1, FALLBACK_RTT_NS)));

        // Master receives the DELAY_REQ and answers with t4 chosen so that the
        // scripted timestamps encode a 600ns round trip:
        // t2 = t3 = 10_000_000 (script clock did not advance between them), so
        // rtt = (t2 - t1) + (t4 - t3) = 600 requires t4 = 600 + t1.
        let (len, _) = master_sock.recv_from(&mut buf).unwrap();
        let Some(PtpMessage::DelayReq { req_id }) = PtpMessage::parse(&buf[..len]) else {
            panic!("slave should have sent a DELAY_REQ");
        };
        let t4 = t1 + 600;
        master_sock.send_to(&PtpMessage::DelayResp { req_id, t4 }.encode(), slave_addr).unwrap();

        let (len, src) = slave.socket.recv_from(&mut buf).unwrap();
        slave.handle_packet_as_slave(&buf[..len], src);

        assert_eq!(slave.measured_rtt_ns(), Some(600), "rtt must come from the exchange, not the 1ms assumption");
        assert_eq!(*slave_clock.last_sync.lock(), Some((t1, 600)), "servo must be re-disciplined with the measured rtt");

        // A stale DELAY_RESP (wrong req_id) must be ignored.
        slave_clock.advance(1);
        master_sock.send_to(&PtpMessage::DelayResp { req_id: req_id.wrapping_add(99), t4: t4 + 5_000 }.encode(), slave_addr).unwrap();
        let (len, src) = slave.socket.recv_from(&mut buf).unwrap();
        slave.handle_packet_as_slave(&buf[..len], src);
        assert_eq!(slave.measured_rtt_ns(), Some(600));
    }
}

#[cfg(test)]
mod rtt_properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// For ANY true path delays and ANY clock offset (slave ahead or
        /// behind, up to ±10s), the four-timestamp computation recovers
        /// exactly the sum of the two legs — the offset must cancel.
        #[test]
        fn rtt_recovers_delay_regardless_of_offset(
            t1 in 1_000_000_000u64..2_000_000_000,
            delay_ab in 0i64..50_000_000,
            delay_ba in 0i64..50_000_000,
            offset in -10_000_000_000i64..10_000_000_000,
            think_time in 0u64..1_000_000,
        ) {
            let t2 = (t1 as i64 + delay_ab + offset) as u64;
            let t3 = t2 + think_time;
            let t4 = (t3 as i64 + delay_ba - offset) as u64;

            let expected = delay_ab + delay_ba;
            if expected <= MAX_PLAUSIBLE_RTT_NS {
                prop_assert_eq!(compute_rtt_ns(t1, t2, t3, t4), Some(expected as u64));
            } else {
                prop_assert_eq!(compute_rtt_ns(t1, t2, t3, t4), None);
            }
        }

        /// The EMA never leaves the closed interval spanned by its inputs and
        /// is monotone in the sample — no overshoot, no sign surprises.
        #[test]
        fn ema_stays_within_input_bounds(
            prev in 0u64..100_000_000,
            sample in 0u64..100_000_000,
        ) {
            let out = ema_rtt(Some(prev), sample);
            let lo = prev.min(sample);
            let hi = prev.max(sample);
            prop_assert!(out >= lo && out <= hi, "EMA must interpolate, got {} outside [{}, {}]", out, lo, hi);
        }
    }
}
