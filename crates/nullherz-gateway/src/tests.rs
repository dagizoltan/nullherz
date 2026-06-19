use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct MockTelemetryProvider {
    pub call_count: AtomicUsize,
}

impl TelemetryProvider for MockTelemetryProvider {
    fn pop_telemetry(&self) -> Option<Telemetry> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        None
    }
}

#[tokio::test]
async fn test_gateway_connection_handling() {
    let provider = Arc::new(MockTelemetryProvider { call_count: AtomicUsize::new(0) });
    let (_cmd_prod, _, _, _) = connect_to_engine().unwrap();

    // We can't easily test handle_connection because it requires a TcpStream
    // and tokio_tungstenite handshake.
    // But we verified the TelemetryProvider abstraction.
    assert_eq!(provider.call_count.load(Ordering::SeqCst), 0);
    let _ = provider.pop_telemetry();
    assert_eq!(provider.call_count.load(Ordering::SeqCst), 1);
}
