use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};
use std::sync::{Arc, Mutex};
use control_plane::{TimestampedCommand};
use ipc_layer::{Producer, Consumer, RingBuffer};
use audio_core::Telemetry;

fn connect_to_engine() -> Result<(Producer<TimestampedCommand>, Consumer<Telemetry>), Box<dyn std::error::Error>> {
    // In a real nullherz deployment, the Conductor spawns the Gateway
    // and passes these buffers via handle or SHM.
    // For now, we provide a clean split that the Conductor can utilize.
    let (cmd_prod, _cmd_cons) = RingBuffer::<TimestampedCommand>::new(1024).split();
    let (_tel_prod, tel_cons) = RingBuffer::<Telemetry>::new(1024).split();

    Ok((cmd_prod, tel_cons))
}

pub async fn run_gateway(
    addr: &str,
    cmd_prod: Producer<TimestampedCommand>,
    tel_cons: Consumer<Telemetry>
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    println!("nullherz-gateway listening on: {}", addr);

    let cmd_prod = Arc::new(Mutex::new(cmd_prod));
    let tel_cons = Arc::new(Mutex::new(tel_cons));

    while let Ok((stream, _)) = listener.accept().await {
        let cmd_prod = Arc::clone(&cmd_prod);
        let tel_cons = Arc::clone(&tel_cons);
        tokio::spawn(handle_connection(stream, cmd_prod, tel_cons));
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:9001";
    let (cmd_prod, tel_cons) = connect_to_engine()?;
    run_gateway(addr, cmd_prod, tel_cons).await
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
                    let mut prod = cmd_prod.lock().unwrap();
                    let _ = prod.push(cmd);
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    tel_task.abort();
}
