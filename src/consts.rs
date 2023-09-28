use lazy_static::lazy_static;

#[cfg(not(debug_assertions))]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(debug_assertions)]
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-dev");

pub const MOTD: &str = "some good 'ol music";

/// The time it takes for Spoticord to disconnect when no music is being played
pub const DISCONNECT_TIME: u64 = 5 * 60;

lazy_static! {
  pub static ref DISCORD_TOKEN: String =
    std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN environment variable");
  pub static ref DATABASE_URL: String =
    std::env::var("DATABASE_URL").expect("missing DATABASE_URL environment variable");
  pub static ref SPOTICORD_ACCOUNTS_URL: String = std::env::var("SPOTICORD_ACCOUNTS_URL")
    .expect("missing SPOTICORD_ACCOUNTS_URL environment variable");
}

#[cfg(feature = "stats")]
lazy_static! {
  pub static ref KV_URL: String =
    std::env::var("KV_URL").expect("missing KV_URL environment variable");
}
