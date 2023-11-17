pub mod core;
pub mod music;

#[cfg(debug_assertions)]
mod ping;
#[cfg(debug_assertions)]
pub use ping::ping;

#[cfg(debug_assertions)]
mod token;
#[cfg(debug_assertions)]
pub use token::token;
