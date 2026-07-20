use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};
use std::sync::Arc;
use parking_lot::Mutex;
use nullherz_traits::{TimestampedCommand, SoundDNA};
use ipc_layer::{Consumer, RingBuffer};
use nullherz_traits::telemetry::Telemetry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum GatewayRequest {
    Matchmaking {
        target_dna: SoundDNA,
        limit: usize,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GatewayResponse {
    MatchmakingResult {
        matches: Vec<(u64, f32)>,
    },
}

pub fn connect_to_engine() -> Result<(ipc_layer::NonRtProducer<TimestampedCommand>, Consumer<Telemetry>, Arc<ipc_layer::MpscRingBuffer<TimestampedCommand>>, ipc_layer::Producer<Telemetry>), Box<dyn std::error::Error>> {
    // In a real nullherz deployment, the Conductor spawns the Gateway
    // and passes these buffers via handle or SHM.
    let cmd_buffer = Arc::new(ipc_layer::MpscRingBuffer::new(1024));
    let cmd_prod = ipc_layer::NonRtProducer::from_boxed(Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())));

    let (tel_prod, tel_cons) = RingBuffer::<Telemetry>::new(1024).split();

    Ok((cmd_prod, tel_cons, cmd_buffer, tel_prod))
}

pub trait TelemetryProvider: Send + Sync {
    fn get_telemetry(&self) -> Option<Telemetry>;
}

pub struct TelemetryBroadcaster {
    pub current: Mutex<Option<Telemetry>>,
}

impl TelemetryProvider for TelemetryBroadcaster {
    fn get_telemetry(&self) -> Option<Telemetry> {
        *self.current.lock()
    }
}

#[cfg(test)]
mod tests;

pub async fn run_gateway(
    addr: &str,
    cmd_prod: ipc_layer::NonRtProducer<TimestampedCommand>,
    mut tel_cons: Consumer<Telemetry>,
    library_db: Option<Arc<Mutex<nullherz_dna::LibraryDatabase>>>
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    println!("nullherz-gateway listening on: {}", addr);

    let broadcaster = Arc::new(TelemetryBroadcaster { current: Mutex::new(None) });
    let broadcaster_clone = broadcaster.clone();

    // Spawn a task to update the broadcaster from the consumer
    tokio::spawn(async move {
        loop {
            while let Some(tel) = tel_cons.pop() {
                *broadcaster_clone.current.lock() = Some(tel);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    });

    while let Ok((stream, _)) = listener.accept().await {
        let cmd_prod_clone = cmd_prod.clone();
        let tel_provider = Arc::clone(&broadcaster) as Arc<dyn TelemetryProvider>;
        let lib_clone = library_db.clone();
        tokio::spawn(handle_connection(stream, cmd_prod_clone, tel_provider, lib_clone));
    }

    Ok(())
}


async fn handle_connection(
    stream: TcpStream,
    cmd_prod: ipc_layer::NonRtProducer<TimestampedCommand>,
    tel_provider: Arc<dyn TelemetryProvider>,
    library_db: Option<Arc<Mutex<nullherz_dna::LibraryDatabase>>>
) {
    // Panic-freedom: any TCP client that fails the WebSocket handshake (a
    // port scanner, a health check, a curl) used to PANIC this connection
    // task with a full backtrace. Drop the connection quietly instead.
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("Gateway: rejected non-WebSocket connection: {}", e);
            return;
        }
    };
    println!("New WebSocket connection");

    let (mut write, mut read) = ws_stream.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Message>(32);

    // Task 1: Receiver - Forward messages from tx channel to WebSocket write half
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(msg).await.is_err() { break; }
        }
    });

    // Task 2: Telemetry Broadcaster - Push telemetry to tx channel
    let tx_telemetry = tx.clone();
    let tel_task = tokio::spawn(async move {
        let mut last_sample_counter = 0;
        loop {
            let tel = tel_provider.get_telemetry();
            if let Some(t) = tel
                && t.sample_counter != last_sample_counter {
                    if let Ok(json) = serde_json::to_string(&t)
                        && tx_telemetry.send(Message::Text(json.into())).await.is_err() { break; }
                    last_sample_counter = t.sample_counter;
                }
            tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
        }
    });

    // Task 3: Command & Request Handler
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // 1. Try TimestampedCommand
                if let Ok(cmd) = serde_json::from_str::<TimestampedCommand>(&text) {
                    let _ = cmd_prod.push_command(cmd);
                    continue;
                }

                // 2. Try GatewayRequest (Matchmaking)
                if let Ok(req) = serde_json::from_str::<GatewayRequest>(&text) {
                    match req {
                        GatewayRequest::Matchmaking { target_dna, limit } => {
                            if let Some(ref lib_mutex) = library_db {
                                let mut matches: Vec<(u64, f32)> = Vec::new();
                                let mut success = false;
                                {
                                    let lib = lib_mutex.lock();
                                    if let Ok(m) = nullherz_dna::Matchmaker::find_best_matches(&lib, &target_dna, limit) {
                                        matches = m;
                                        success = true;
                                    }
                                }

                                if success {
                                    let resp = GatewayResponse::MatchmakingResult { matches };
                                    if let Ok(json) = serde_json::to_string(&resp) {
                                        let _ = tx.send(Message::Text(json.into())).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    tel_task.abort();
    write_task.abort();
}
