use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};
use std::sync::{Arc, Mutex};
use control_plane::{TimestampedCommand};
use ipc_layer::{Producer, Consumer, RingBuffer};
use audio_core::Telemetry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9001";
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("nullherz-gateway listening on: {}", addr);

    // In a real scenario, these would be connected to the AudioEngine.
    // For this demonstration, we'll create some dummy buffers.
    let (cmd_prod, _cmd_cons) = RingBuffer::<TimestampedCommand>::new(1024).split();
    let (_tel_prod, tel_cons) = RingBuffer::<Telemetry>::new(1024).split();

    let cmd_prod = Arc::new(Mutex::new(cmd_prod));
    let tel_cons = Arc::new(Mutex::new(tel_cons));

    while let Ok((stream, _)) = listener.accept().await {
        let cmd_prod = Arc::clone(&cmd_prod);
        let tel_cons = Arc::clone(&tel_cons);
        tokio::spawn(handle_connection(stream, cmd_prod, tel_cons));
    }

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    cmd_prod: Arc<Mutex<Producer<TimestampedCommand>>>,
    tel_cons: Arc<Mutex<Consumer<Telemetry>>>
) {
    let ws_stream = accept_async(stream).await.expect("Error during the websocket handshake occurred");
    println!("New WebSocket connection");

    let (mut write, mut read) = ws_stream.split();

    // Spawn a task to broadcast telemetry
    let tel_task = tokio::spawn(async move {
        loop {
            let tel = {
                let mut cons = tel_cons.lock().unwrap();
                cons.pop()
            };

            if let Some(t) = tel {
                let json = serde_json::to_string(&t).unwrap();
                if write.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    // Handle incoming commands
    while let Some(msg) = read.next().await {
        let msg = msg.expect("Error reading message");
        if let Message::Text(text) = msg {
            if let Ok(cmd) = serde_json::from_str::<TimestampedCommand>(&text) {
                let mut prod = cmd_prod.lock().unwrap();
                let _ = prod.push(cmd);
            }
        }
    }

    tel_task.abort();
}
