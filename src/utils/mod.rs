use std::time::{SystemTime, UNIX_EPOCH};

pub mod discord;
pub mod embed;
pub mod spotify;

pub fn get_time() -> u64 {
  let now = SystemTime::now();
  let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

  since_the_epoch.as_secs()
}

pub fn get_time_ms() -> u128 {
  let now = SystemTime::now();
  let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

  since_the_epoch.as_millis()
}

pub fn time_to_str(time: u32) -> String {
  let hour = 3600;
  let min = 60;

  if time / hour >= 1 {
    return format!(
      "{}h{}m{}s",
      time / hour,
      (time % hour) / min,
      (time % hour) % min
    );
  } else if time / min >= 1 {
    return format!("{}m{}s", time / min, time % min);
  } else {
    return format!("{}s", time);
  }
}
