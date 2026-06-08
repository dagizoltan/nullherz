use audio_core::AudioProcessor;
use std::sync::mpsc;
use std::sync::{Arc};

pub struct BroadcastSidecar {
    is_active: bool,
    #[allow(dead_code)]
    sample_rate: f32,
    tx: Option<mpsc::SyncSender<ipc_layer::AudioBlock>>,
    rt: Arc<tokio::runtime::Runtime>,
}

impl BroadcastSidecar {
    pub fn new(sample_rate: f32) -> Self {
        let rt = Arc::new(tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap());

        let (tx, rx) = std::sync::mpsc::sync_channel::<ipc_layer::AudioBlock>(64);

        rt.spawn(async move {
            use tokio_tungstenite::connect_async;
            use futures_util::SinkExt;

            let url = "ws://127.0.0.1:9001/broadcast";
            if let Ok((mut ws_stream, _)) = connect_async(url).await {
                while let Ok(block) = rx.recv() {
                    let data = &block.data[..block.len as usize];
                    let mut bytes = Vec::with_capacity(data.len() * 4);
                    for &f in data {
                        bytes.extend_from_slice(&f.to_le_bytes());
                    }
                    if ws_stream.send(tokio_tungstenite::tungstenite::Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
            }
        });

        Self {
            is_active: false,
            sample_rate,
            tx: Some(tx),
            rt,
        }
    }
}

impl AudioProcessor for BroadcastSidecar {
    fn process(&mut self, inputs: &[&[f32]], _out: &mut [&mut [f32]], _context: &mut audio_core::processors::ProcessContext) {
        if !self.is_active || inputs.len() < 2 { return; }

        let left = inputs[0];
        let right = inputs[1];

        // Package as interleaved stereo for the stream into a pre-allocated block
        let mut block = ipc_layer::AudioBlock {
            data: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            len: (left.len() * 2).min(ipc_layer::MAX_BLOCK_SIZE) as u32,
        };

        let num_samples = left.len().min(ipc_layer::MAX_BLOCK_SIZE / 2);
        for i in 0..num_samples {
            block.data[i * 2] = left[i];
            block.data[i * 2 + 1] = right[i];
        }
        block.len = (num_samples * 2) as u32;

        if let Some(ref tx) = self.tx {
            // try_send is RT-safe for sync_channel as it won't block
            let _ = tx.try_send(block);
        }
    }

    fn apply_command(&mut self, cmd: &control_plane::Command) {
        match cmd {
            control_plane::Command::Play => self.is_active = true,
            control_plane::Command::Stop => self.is_active = false,
            _ => {}
        }
    }
}
