use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum IpcPacket {
  Quit,

  Connect(String, String),
  Disconnect,

  ConnectError(String),

  StartPlayback,
  StopPlayback,

  /// The current Spotify track was changed
  TrackChange(String),

  /// Spotify playback was started/resumed
  Playing(String, u32, u32),

  /// Spotify playback was paused
  Paused(String, u32, u32),

  /// Sent when the user has switched their Spotify device away from Spoticord
  Stopped,
}
