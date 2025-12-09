pub mod packet;
pub mod stdio;

use std::marker::{PhantomData, Unpin};
use std::pin::Pin;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::io::{AsyncRead, Lines};
use tokio_stream::Stream;

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("operation timed out")]
    Timeout,
}

pub struct IpcWriter<W> {
    writer: W,
}

pub struct IpcReader<R, T> {
    lines: Lines<BufReader<R>>,
    _phantom: PhantomData<T>,
}

impl<W> IpcWriter<W>
where
    W: AsyncWriteExt + Unpin,
{
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub async fn send_message<M: Serialize>(&mut self, message: &M) -> Result<(), IpcError> {
        let mut json = serde_json::to_string(message)?;
        json.push('\n');

        match tokio::time::timeout(
            Duration::from_secs(5),
            self.writer.write_all(json.as_bytes()),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                log::error!("ipc write operation timed out");

                return Err(IpcError::Timeout);
            }
        };

        Ok(())
    }
}

impl<R, T> IpcReader<R, T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    pub fn new(reader: R) -> Self {
        let lines = BufReader::new(reader).lines();

        Self {
            lines,
            _phantom: PhantomData,
        }
    }
}

impl<R, T> Stream for IpcReader<R, T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned + Unpin,
{
    type Item = Result<T, IpcError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();

        match Pin::new(&mut this.lines).poll_next_line(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(opt_line) => {
                let result = opt_line
                    .transpose()
                    .map(|line| match line.map_err(IpcError::Io) {
                        Ok(line) if line.is_empty() => Err(IpcError::Io(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "empty line",
                        ))),
                        Ok(line) => serde_json::from_str(&line).map_err(IpcError::Json),
                        Err(err) => Err(err),
                    });

                std::task::Poll::Ready(result)
            }
        }
    }
}

pub fn writer<W>(writer: W) -> IpcWriter<W>
where
    W: AsyncWriteExt + Unpin,
{
    IpcWriter::new(writer)
}

pub fn reader<R, T>(reader: R) -> IpcReader<R, T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    IpcReader::new(reader)
}
