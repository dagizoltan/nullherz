use tokio::net::{TcpStream, TcpListener};
use tokio::io::AsyncWriteExt;
use nullherz_traits::{TimestampedCommand, AudioBlock};
use std::sync::Arc;

pub struct TcpIpcProducer {
    stream: Arc<tokio::sync::Mutex<TcpStream>>,
}

impl TcpIpcProducer {
    pub async fn connect(addr: &str) -> Result<Self, std::io::Error> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self { stream: Arc::new(tokio::sync::Mutex::new(stream)) })
    }

    pub fn into_inner(self) -> Result<TcpStream, Self> {
        match Arc::try_unwrap(self.stream) {
            Ok(mutex) => Ok(mutex.into_inner()),
            Err(arc) => Err(Self { stream: arc }),
        }
    }

    pub async fn send_command(&self, cmd: TimestampedCommand) -> Result<(), std::io::Error> {
        let serialized = serde_json::to_vec(&cmd).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let mut stream = self.stream.lock().await;
        stream.write_u32(serialized.len() as u32).await?; // Big-endian by default in write_u32? No, tokio is BigEndian
        stream.write_all(&serialized).await?;
        Ok(())
    }

    pub async fn send_audio_block(&self, block: AudioBlock) -> Result<(), std::io::Error> {
        let mut stream = self.stream.lock().await;
        stream.write_u8(1).await?; // Type: Audio
        stream.write_u32(block.len).await?;
        let data_bytes = bytemuck::cast_slice(&block.data[..block.len as usize]);
        stream.write_all(data_bytes).await?;
        Ok(())
    }
}

pub struct TcpIpcConsumer {
    listener: TcpListener,
}

impl TcpIpcConsumer {
    pub async fn bind(addr: &str) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener })
    }

    pub async fn accept(&self) -> Result<TcpStream, std::io::Error> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }
}
