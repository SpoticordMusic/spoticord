use std::{
  io::{Read, Seek, Write},
  sync::{Arc, Condvar, Mutex},
};

use songbird::input::reader::MediaSource;

const MAX_SIZE: usize = 1 * 1024 * 1024;

#[derive(Clone)]
pub struct Stream {
  inner: Arc<(Mutex<Vec<u8>>, Condvar)>,
}

impl Stream {
  pub fn new() -> Self {
    Self {
      inner: Arc::new((Mutex::new(Vec::new()), Condvar::new())),
    }
  }
}

impl Read for Stream {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    let (mutex, condvar) = &*self.inner;
    let mut buffer = mutex.lock().expect("Mutex was poisoned");

    log::trace!("Read!");

    while buffer.is_empty() {
      buffer = condvar.wait(buffer).expect("Mutex was poisoned");
    }

    let max_read = usize::min(buf.len(), buffer.len());
    buf[0..max_read].copy_from_slice(&buffer[0..max_read]);
    buffer.drain(0..max_read);

    Ok(max_read)
  }
}

impl Write for Stream {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    let (mutex, condvar) = &*self.inner;
    let mut buffer = mutex.lock().expect("Mutex was poisoned");

    while buffer.len() + buf.len() > MAX_SIZE {
      buffer = condvar.wait(buffer).unwrap();
    }

    buffer.extend_from_slice(buf);
    condvar.notify_all();

    Ok(buf.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    Ok(())
  }
}

impl Seek for Stream {
  fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
    Ok(0)
  }
}

impl MediaSource for Stream {
  fn byte_len(&self) -> Option<u64> {
    None
  }

  fn is_seekable(&self) -> bool {
    false
  }
}
