use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum IpcPacket {
  Quit,

  Connect(String, String),
  Disconnect,

  StartPlayback,
  StopPlayback,
}
