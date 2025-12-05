mod env;

#[cfg(not(debug_assertions))]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(debug_assertions)]
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-dev");

pub fn motd() -> &'static str {
    env::MOTD.as_deref().unwrap_or("some good 'ol music")
}

pub fn discord_token() -> &'static str {
    &env::DISCORD_TOKEN
}

pub fn database_url() -> &'static str {
    &env::DATABASE_URL
}

pub fn link_url() -> &'static str {
    &env::LINK_URL
}

pub fn spotify_client_id() -> &'static str {
    &env::SPOTIFY_CLIENT_ID
}

pub fn spotify_client_secret() -> &'static str {
    &env::SPOTIFY_CLIENT_SECRET
}

#[cfg(feature = "stats")]
pub fn kv_url() -> &'static str {
    &env::KV_URL
}
