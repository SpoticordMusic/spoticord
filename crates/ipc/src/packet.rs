use std::fmt::{Debug, Display};

use serde::{Deserialize, Serialize};
use spoticord_shared::player_info::PlayerInfo;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum BotMessage {
    /// The initial "handshake" message that informs the player how to connect to Discord
    Initialize {
        guild_id: u64,
        channel_id: u64,
        endpoint: String,
        token: String,
        session_id: String,
        user_id: u64,
    },
    /// Create a new librespot session for the target user, shutting down any active sessions running currently
    SetUser {
        /// The user's Discord id.
        user_id: u64,
        // The device name.
        device_name: String,
        /// The access token.
        credentials: SecretString,
    },
    /// Requests the player to disconnect from Spotify.
    Disconnect,
    /// Requests the player to stop and shut down.
    Shutdown,

    /// Start or resume playback
    Play,
    /// Pause playback
    Pause,
    /// Advance to the next track in the queue
    NextTrack,
    /// Go back to the previous track in the queue
    PreviousTrack,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum PlayerMessage {
    /// Reports that the player is ready and waiting for a user.
    Ready,
    /// Reports that the player has successfully connected to Spotify.
    Connected,
    /// Reports that the player has been disconnected from Spotify.
    Disconnected,
    /// Reports that the player is shutting down.
    Shutdown,
    /// Reports an error.
    Error(String),
    /// Reports the full player info.
    Update {
        info: Box<PlayerInfo>,
        track_changed: bool,
    },
}

#[derive(Serialize, Deserialize)]
pub struct SecretString(String);

impl SecretString {
    pub fn expose(self) -> String {
        self.0
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<secret>")
    }
}

impl Display for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<secret>")
    }
}
