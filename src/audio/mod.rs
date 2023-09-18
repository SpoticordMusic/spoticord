use librespot::playback::audio_backend::{Sink, SinkAsBytes, SinkError, SinkResult};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use log::error;
use std::io::{Stdout, Write};
use tokio::sync::mpsc::UnboundedSender;

use crate::ipc;
use crate::ipc::packet::IpcPacket;
use crate::player::stream::Stream;

pub struct StdoutSink {
  client: ipc::Client,
  output: Option<Box<Stdout>>,
}

impl StdoutSink {
  pub fn new(client: ipc::Client) -> Self {
    StdoutSink {
      client,
      output: None,
    }
  }
}

impl Sink for StdoutSink {
  fn start(&mut self) -> SinkResult<()> {
    if let Err(why) = self.client.send(IpcPacket::StartPlayback) {
      error!("Failed to send start playback packet: {}", why);
      return Err(SinkError::ConnectionRefused(why.to_string()));
    }

    self.output.get_or_insert(Box::new(std::io::stdout()));

    Ok(())
  }

  fn stop(&mut self) -> SinkResult<()> {
    if let Err(why) = self.client.send(IpcPacket::StopPlayback) {
      error!("Failed to send stop playback packet: {}", why);
      return Err(SinkError::ConnectionRefused(why.to_string()));
    }

    self
      .output
      .take()
      .ok_or_else(|| SinkError::NotConnected("StdoutSink is not connected".to_string()))?
      .flush()
      .map_err(|why| SinkError::OnWrite(why.to_string()))?;

    Ok(())
  }

  fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
    use zerocopy::AsBytes;

    if let AudioPacket::Samples(samples) = packet {
      let samples_f32: &[f32] = &converter.f64_to_f32(&samples);

      let resampled = samplerate::convert(
        44100,
        48000,
        2,
        samplerate::ConverterType::Linear,
        samples_f32,
      )
      .expect("to succeed");

      let samples_i16 =
        &converter.f64_to_s16(&resampled.iter().map(|v| *v as f64).collect::<Vec<f64>>());

      self.write_bytes(samples_i16.as_bytes())?;
    }

    Ok(())
  }
}

impl SinkAsBytes for StdoutSink {
  fn write_bytes(&mut self, data: &[u8]) -> SinkResult<()> {
    self
      .output
      .as_deref_mut()
      .ok_or_else(|| SinkError::NotConnected("StdoutSink is not connected".to_string()))?
      .write_all(data)
      .map_err(|why| SinkError::OnWrite(why.to_string()))?;

    Ok(())
  }
}

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
      error!("Failed to send start playback event: {why}");
      return Err(SinkError::ConnectionRefused(why.to_string()));
    }

    Ok(())
  }

  fn stop(&mut self) -> SinkResult<()> {
    if let Err(why) = self.sender.send(SinkEvent::Stop) {
      error!("Failed to send start playback event: {why}");
      return Err(SinkError::ConnectionRefused(why.to_string()));
    }

    Ok(())
  }

  fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
    use zerocopy::AsBytes;

    let AudioPacket::Samples(samples) = packet else { return Ok(()); };
    let samples_f32: &[f32] = &converter.f64_to_f32(&samples);

    let resampled = samplerate::convert(
      44100,
      48000,
      2,
      samplerate::ConverterType::Linear,
      samples_f32,
    )
    .expect("to succeed");

    let samples_i16 =
      &converter.f64_to_s16(&resampled.iter().map(|v| *v as f64).collect::<Vec<f64>>());

    self.write_bytes(samples_i16.as_bytes())?;

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
