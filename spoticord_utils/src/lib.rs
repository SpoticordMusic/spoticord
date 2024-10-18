pub mod discord;

use std::time::{SystemTime, UNIX_EPOCH};

pub fn get_time() -> u128 {
    let now = SystemTime::now();
    let since_the_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

    since_the_epoch.as_millis()
}

pub fn time_to_string(time: u32) -> String {
    let hour = 3600;
    let min = 60;

    if time / hour >= 1 {
        format!(
            "{}h{}m{}s",
            time / hour,
            (time % hour) / min,
            (time % hour) % min
        )
    } else if time / min >= 1 {
        format!("{}m{}s", time / min, time % min)
    } else {
        format!("{}s", time)
    }
}
