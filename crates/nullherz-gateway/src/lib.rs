use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};
use std::sync::{Arc, Mutex};
use control_plane::{TimestampedCommand};
use ipc_layer::{Consumer, RingBuffer};
use audio_core::Telemetry;

pub fn connect_to_engine() -> Result<(ipc_layer::NonRtProducer<TimestampedCommand>, Consumer<Telemetry>), Box<dyn std::error::Error>> {
    // In a real nullherz deployment, the Conductor spawns the Gateway
    // and passes these buffers via handle or SHM.
    // For now, we provide a clean split that the Conductor can utilize.
    let (cmd_prod, _cmd_cons) = RingBuffer::<TimestampedCommand>::new(1024).split();
    let cmd_prod = ipc_layer::NonRtProducer::new(cmd_prod);
    let (_tel_prod, tel_cons) = RingBuffer::<Telemetry>::new(1024).split();

    Ok((cmd_prod, tel_cons))
}

pub async fn run_gateway(
    addr: &str,
    cmd_prod: ipc_layer::NonRtProducer<TimestampedCommand>,
    tel_cons: Consumer<Telemetry>
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    println!("nullherz-gateway listening on: {}", addr);

    let tel_cons = Arc::new(Mutex::new(tel_cons));

    while let Ok((stream, _)) = listener.accept().await {
        let cmd_prod_clone = cmd_prod.clone();
        let tel_cons_clone = Arc::clone(&tel_cons);
        tokio::spawn(handle_connection(stream, cmd_prod_clone, tel_cons_clone));
    }

    Ok(())
}


async fn handle_connection(
    stream: TcpStream,
    cmd_prod: ipc_layer::NonRtProducer<TimestampedCommand>,
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
                if let Ok(json) = serde_json::to_string(&t) {
                    if write.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
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
                    let _ = cmd_prod.push(cmd).await;
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    tel_task.abort();
}
