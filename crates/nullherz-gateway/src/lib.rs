use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};
use std::sync::{Arc, Mutex};
use nullherz_traits::{TimestampedCommand};
use ipc_layer::{Consumer, RingBuffer};
use nullherz_traits::telemetry::Telemetry;

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
        *self.current.lock().unwrap()
    }
}

#[cfg(test)]
mod tests;

pub async fn run_gateway(
    addr: &str,
    cmd_prod: ipc_layer::NonRtProducer<TimestampedCommand>,
    mut tel_cons: Consumer<Telemetry>
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    println!("nullherz-gateway listening on: {}", addr);

    let broadcaster = Arc::new(TelemetryBroadcaster { current: Mutex::new(None) });
    let broadcaster_clone = broadcaster.clone();

    // Spawn a task to update the broadcaster from the consumer
    tokio::spawn(async move {
        loop {
            while let Some(tel) = tel_cons.pop() {
                *broadcaster_clone.current.lock().unwrap() = Some(tel);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    });

    while let Ok((stream, _)) = listener.accept().await {
        let cmd_prod_clone = cmd_prod.clone();
        let tel_provider = Arc::clone(&broadcaster) as Arc<dyn TelemetryProvider>;
        tokio::spawn(handle_connection(stream, cmd_prod_clone, tel_provider));
    }

    Ok(())
}


async fn handle_connection(
    stream: TcpStream,
    cmd_prod: ipc_layer::NonRtProducer<TimestampedCommand>,
    tel_provider: Arc<dyn TelemetryProvider>
) {
    let ws_stream = accept_async(stream).await.expect("Error during the websocket handshake occurred");
    println!("New WebSocket connection");

    let (mut write, mut read) = ws_stream.split();

    // Spawn a task to broadcast telemetry
    let tel_task = tokio::spawn(async move {
        let mut last_sample_counter = 0;
        loop {
            let tel = tel_provider.get_telemetry();

            if let Some(t) = tel {
                if t.sample_counter != last_sample_counter {
                    if let Ok(json) = serde_json::to_string(&t) {
                        let msg = Message::Text(json.into());
                        if write.send(msg).await.is_err() {
                            break;
                        }
                    }
                    last_sample_counter = t.sample_counter;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(16)).await; // ~60fps
        }
    });

    // Handle incoming commands
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(cmd) = serde_json::from_str::<TimestampedCommand>(&text) {
                    let _ = cmd_prod.push_command(cmd).await;
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    tel_task.abort();
}
