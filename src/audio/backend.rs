use librespot::playback::audio_backend::{Sink, SinkAsBytes, SinkResult};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use log::{error, trace};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::ipc;
use crate::ipc::packet::IpcPacket;

pub struct StdoutSink {
  client: ipc::Client,
  buffer: Arc<Mutex<Vec<u8>>>,
  is_stopped: Arc<Mutex<bool>>,
  handle: Option<JoinHandle<()>>,
}

const BUFFER_SIZE: usize = 7680;

impl StdoutSink {
  pub fn start_writer(&mut self) {
    // With 48khz, 32-bit float, 2 channels, 1 second of audio is 384000 bytes
    // 384000 / 50 = 7680 bytes per 20ms

    let buffer = self.buffer.clone();
    let is_stopped = self.is_stopped.clone();
    let client = self.client.clone();

    let handle = std::thread::spawn(move || {
      let mut output = std::io::stdout();
      let mut act_buffer = [0u8; BUFFER_SIZE];

      // Use closure to make sure lock is released as fast as possible
      let is_stopped = || {
        let is_stopped = is_stopped.lock().unwrap();
        *is_stopped
      };

      // Start songbird's playback
      client.send(IpcPacket::StartPlayback).unwrap();

      loop {
        if is_stopped() {
          break;
        }

        std::thread::sleep(Duration::from_millis(15));

        let mut buffer = buffer.lock().unwrap();
        let to_drain: usize;

        if buffer.len() < BUFFER_SIZE {
          // Copy the buffer into the action buffer
          // Fill remaining length with zeroes
          act_buffer[..buffer.len()].copy_from_slice(&buffer[..]);
          act_buffer[buffer.len()..].fill(0);

          to_drain = buffer.len();
        } else {
          act_buffer.copy_from_slice(&buffer[..BUFFER_SIZE]);
          to_drain = BUFFER_SIZE;
        }

        output.write_all(&act_buffer).unwrap_or(());
        buffer.drain(..to_drain);
      }
    });

    self.handle = Some(handle);
  }

  pub fn stop_writer(&mut self) -> std::thread::Result<()> {
    // Use closure to avoid deadlocking the mutex
    let set_stopped = |value| {
      let mut is_stopped = self.is_stopped.lock().unwrap();
      *is_stopped = value;
    };

    // Notify thread to stop
    set_stopped(true);

    // Wait for thread to stop
    let result = match self.handle.take() {
      Some(handle) => handle.join(),
      None => Ok(()),
    };

    // Reset stopped value
    set_stopped(false);

    result
  }

  pub fn new(client: ipc::Client) -> Self {
    StdoutSink {
      client,
      is_stopped: Arc::new(Mutex::new(false)),
      buffer: Arc::new(Mutex::new(Vec::new())),
      handle: None,
    }
  }
}

impl Sink for StdoutSink {
  fn start(&mut self) -> SinkResult<()> {
    self.start_writer();

    Ok(())
  }

  fn stop(&mut self) -> SinkResult<()> {
    // Stop the writer thread
    // This is done before pausing songbird, because else the writer thread
    //  might hang on writing to stdout
    if let Err(why) = self.stop_writer() {
      error!("Failed to stop stdout writer: {:?}", why);
    } else {
      trace!("Stopped stdout writer");
    }

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
      self.write_bytes(resampled.as_bytes())?;
    }

    Ok(())
  }
}

impl SinkAsBytes for StdoutSink {
  fn write_bytes(&mut self, data: &[u8]) -> SinkResult<()> {
    let get_buffer_len = || {
      let buffer = self.buffer.lock().unwrap();
      buffer.len()
    };

    while get_buffer_len() > BUFFER_SIZE * 5 {
      std::thread::sleep(Duration::from_millis(15));
    }

    let mut buffer = self.buffer.lock().unwrap();

    buffer.extend_from_slice(data);

    Ok(())
  }
}
