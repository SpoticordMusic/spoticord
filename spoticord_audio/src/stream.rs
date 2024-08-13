use std::{
    io::{Read, Seek, Write},
    sync::{Arc, Condvar, Mutex},
};

use songbird::input::core::io::MediaSource;

/// The lower the value, the less latency
///
/// Too low of a value results in jittery audio
const BUFFER_SIZE: usize = 64 * 1024;

#[derive(Clone, Default)]
pub struct Stream {
    inner: Arc<(Mutex<Vec<u8>>, Condvar)>,
}

impl Stream {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let (mutex, condvar) = &*self.inner;
        let mut buffer = mutex.lock().expect("Mutex was poisoned");

        // Prevent Discord jitter by filling buffer with zeroes if we don't have any audio
        // (i.e. when you skip too far ahead in a song which hasn't been downloaded yet)
        if buffer.is_empty() {
            buf.fill(0);
            condvar.notify_all();

            return Ok(buf.len());
        }

        let max_read = usize::min(buf.len(), buffer.len());

        buf[0..max_read].copy_from_slice(&buffer[0..max_read]);
        buffer.drain(0..max_read);
        condvar.notify_all();

        Ok(max_read)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let (mutex, condvar) = &*self.inner;
        let mut buffer = mutex.lock().expect("Mutex was poisoned");

        while buffer.len() + buf.len() > BUFFER_SIZE {
            buffer = condvar.wait(buffer).expect("Mutex was poisoned");
        }

        buffer.extend_from_slice(buf);
        condvar.notify_all();

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let (mutex, condvar) = &*self.inner;
        let mut buffer = mutex.lock().expect("Mutex was poisoned");

        buffer.clear();
        condvar.notify_all();

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
