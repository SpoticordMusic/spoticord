use lazy_static::lazy_static;

lazy_static! {
    pub static ref DISCORD_TOKEN: String =
        std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN environment variable");
    pub static ref DATABASE_URL: String =
        std::env::var("DATABASE_URL").expect("missing DATABASE_URL environment variable");
    pub static ref LINK_URL: String =
        std::env::var("LINK_URL").expect("missing LINK_URL environment variable");
    pub static ref SPOTIFY_CLIENT_ID: String =
        std::env::var("SPOTIFY_CLIENT_ID").expect("missing SPOTIFY_CLIENT_ID environment variable");
    pub static ref SPOTIFY_CLIENT_SECRET: String = std::env::var("SPOTIFY_CLIENT_SECRET")
        .expect("missing SPOTIFY_CLIENT_SECRET environment variable");
}
