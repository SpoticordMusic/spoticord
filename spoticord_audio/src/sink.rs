use crate::stream::Stream;
use librespot::playback::audio_backend::{Sink, SinkAsBytes, SinkError, SinkResult};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use std::io::Write;
use tokio::sync::mpsc::UnboundedSender;

pub enum SinkEvent {
    Start,
    Stop,
}

pub struct StreamSink {
    stream: Stream,
    sender: UnboundedSender<SinkEvent>,
}

impl StreamSink {
    pub fn new(stream: Stream, sender: UnboundedSender<SinkEvent>) -> Self {
        Self { stream, sender }
    }
}

impl Sink for StreamSink {
    fn start(&mut self) -> SinkResult<()> {
        if let Err(_why) = self.sender.send(SinkEvent::Start) {
            // WARNING: Returning an error causes librespot-playback to panic

            // return Err(SinkError::ConnectionRefused(_why.to_string()));
        }

        Ok(())
    }

    fn stop(&mut self) -> SinkResult<()> {
        if let Err(_why) = self.sender.send(SinkEvent::Stop) {
            // WARNING: Returning an error causes librespot-playback to panic

            // return Err(SinkError::ConnectionRefused(_why.to_string()));
        }

        self.stream.flush().ok();

        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        use zerocopy::IntoBytes;

        let AudioPacket::Samples(samples) = packet else {
            return Ok(());
        };

        self.write_bytes(converter.f64_to_f32(&samples).as_bytes())?;

        Ok(())
    }
}

impl SinkAsBytes for StreamSink {
    fn write_bytes(&mut self, data: &[u8]) -> SinkResult<()> {
        self.stream
            .write_all(data)
            .map_err(|why| SinkError::OnWrite(why.to_string()))?;

        Ok(())
    }
}
