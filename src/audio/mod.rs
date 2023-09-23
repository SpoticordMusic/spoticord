pub mod stream;

use self::stream::Stream;

use librespot::playback::audio_backend::{Sink, SinkAsBytes, SinkError, SinkResult};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use log::error;
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
    if let Err(why) = self.sender.send(SinkEvent::Start) {
      // WARNING: Returning an error causes librespot-playback to exit the process with status 1

      error!("Failed to send start playback event: {why}");
      return Err(SinkError::ConnectionRefused(why.to_string()));
    }

    Ok(())
  }

  fn stop(&mut self) -> SinkResult<()> {
    if let Err(why) = self.sender.send(SinkEvent::Stop) {
      // WARNING: Returning an error causes librespot-playback to exit the process with status 1

      error!("Failed to send start playback event: {why}");
      return Err(SinkError::ConnectionRefused(why.to_string()));
    }

    self.stream.flush().ok();

    Ok(())
  }

  fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
    use zerocopy::AsBytes;

    let AudioPacket::Samples(samples) = packet else {
      return Ok(());
    };
    let samples_f32: &[f32] = &converter.f64_to_f32(&samples);

    let resampled = samplerate::convert(
      44100,
      48000,
      2,
      samplerate::ConverterType::Linear,
      samples_f32,
    )
    .expect("to succeed");

    self.write_bytes(resampled.as_bytes())?;

    Ok(())
  }
}

impl SinkAsBytes for StreamSink {
  fn write_bytes(&mut self, data: &[u8]) -> SinkResult<()> {
    self
      .stream
      .write_all(data)
      .map_err(|why| SinkError::OnWrite(why.to_string()))?;

    Ok(())
  }
}
