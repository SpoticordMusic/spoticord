use std::sync::LazyLock;

macro_rules! define_var {
    ($name:ident) => {
        define_var!($name, stringify!($name));
    };

    ($name:ident, $var:expr) => {
        pub static $name: LazyLock<String> = LazyLock::new(|| {
            std::env::var($var)
                .ok() // Prioritize runtime environment variable
                .or_else(|| option_env!($var).map(|s| s.to_string())) // Otherwise fall back to statically compiled environment variable
                .expect(concat!(
                    "missing ",
                    stringify!($name),
                    " environment variable"
                ))
        });
    };
}

macro_rules! define_var_opt {
    ($name:ident) => {
        define_var_opt!($name, stringify!($name));
    };

    ($name:ident, $var:expr) => {
        pub static $name: LazyLock<Option<String>> = LazyLock::new(|| {
            std::env::var($var)
                .ok() // Prioritize runtime environment variable
                .or_else(|| option_env!($var).map(|s| s.to_string())) // Otherwise fall back to statically compiled environment variable
        });
    };
}

define_var!(DISCORD_TOKEN);
define_var!(DATABASE_URL);
define_var!(LINK_URL);

// Spotify
define_var!(SPOTIFY_CLIENT_ID);
define_var!(SPOTIFY_CLIENT_SECRET);

#[cfg(feature = "stats")]
define_var!(KV_URL);

// Optional variables
define_var_opt!(MOTD);
