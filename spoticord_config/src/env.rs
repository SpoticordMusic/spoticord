use std::sync::LazyLock;

pub static DISCORD_TOKEN: LazyLock<String> = LazyLock::new(|| {
    std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN environment variable")
});
pub static DATABASE_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("DATABASE_URL").expect("missing DATABASE_URL environment variable")
});
pub static LINK_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("LINK_URL").expect("missing LINK_URL environment variable"));
pub static SPOTIFY_CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    std::env::var("SPOTIFY_CLIENT_ID").expect("missing SPOTIFY_CLIENT_ID environment variable")
});
pub static SPOTIFY_CLIENT_SECRET: LazyLock<String> = LazyLock::new(|| {
    std::env::var("SPOTIFY_CLIENT_SECRET")
        .expect("missing SPOTIFY_CLIENT_SECRET environment variable")
});

// Locked behind `stats` feature
pub static KV_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("KV_URL").expect("missing KV_URL environment variable"));
