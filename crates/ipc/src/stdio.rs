use tokio::{io::AsyncRead, sync::mpsc::UnboundedReceiver};
use tokio_stream::StreamExt;

use crate::{
    IpcReader,
    packet::{PlayerMessage, PlayerMessageEvent, PlayerMessageResponse},
};

pub struct PlayerIpcReader {
    rx_cmd: UnboundedReceiver<PlayerMessageResponse>,
    rx_event: UnboundedReceiver<PlayerMessageEvent>,
}

impl PlayerIpcReader {
    pub async fn next_response(&mut self) -> Option<PlayerMessageResponse> {
        self.rx_cmd.recv().await
    }

    pub async fn next_event(&mut self) -> Option<PlayerMessageEvent> {
        self.rx_event.recv().await
    }
}

pub fn reader<R>(mut reader: IpcReader<R, PlayerMessage>) -> PlayerIpcReader
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (tx_cmd, rx_cmd) = tokio::sync::mpsc::unbounded_channel();
    let (tx_event, rx_event) = tokio::sync::mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(Ok(message)) = reader.next().await {
            match message {
                PlayerMessage::Response(command) => _ = tx_cmd.send(command),
                PlayerMessage::Event(event) => _ = tx_event.send(event),
            }
        }
    });

    PlayerIpcReader { rx_cmd, rx_event }
}
