#[cfg(not(debug_assertions))]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(debug_assertions)]
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-dev");

pub const MOTD: &str = "some good 'ol music";

/// The time it takes for Spoticord to disconnect when no music is being played
pub const DISCONNECT_TIME: u64 = 5 * 60;
