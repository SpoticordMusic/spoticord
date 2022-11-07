use librespot::playback::audio_backend::{Sink, SinkAsBytes, SinkResult};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use std::io::Write;

use crate::ipc;
use crate::ipc::packet::IpcPacket;

pub struct StdoutSink {
  client: ipc::Client,
}

impl StdoutSink {
  pub fn new(client: ipc::Client) -> Self {
    StdoutSink { client }
  }
}

impl Sink for StdoutSink {
  fn start(&mut self) -> SinkResult<()> {
    // TODO: Handle error
    self.client.send(IpcPacket::StartPlayback).unwrap();

    Ok(())
  }

  fn stop(&mut self) -> SinkResult<()> {
    // Stop songbird's playback
    self.client.send(IpcPacket::StopPlayback).unwrap();

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
        &samples_f32,
      )
      .unwrap();

      let samples_i16 =
        &converter.f64_to_s16(&resampled.iter().map(|v| *v as f64).collect::<Vec<f64>>());

      self.write_bytes(samples_i16.as_bytes())?;
    }

    Ok(())
  }
}

impl SinkAsBytes for StdoutSink {
  fn write_bytes(&mut self, data: &[u8]) -> SinkResult<()> {
    std::io::stdout().write_all(data).unwrap();

    Ok(())
  }
}
