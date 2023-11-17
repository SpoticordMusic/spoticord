pub mod core;
pub mod music;

#[cfg(debug_assertions)]
mod ping;

#[cfg(debug_assertions)]
mod token;

pub use ping::ping;
pub use token::token;
