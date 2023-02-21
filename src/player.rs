use std::{process::exit, time::Duration};

use ipc_channel::ipc::{IpcError, TryRecvError};
use librespot::{
  connect::spirc::Spirc,
  core::{
    config::{ConnectConfig, SessionConfig},
    session::Session,
  },
  discovery::Credentials,
  playback::{
    config::{Bitrate, PlayerConfig},
    mixer::{self, MixerConfig},
    player::{Player, PlayerEvent},
  },
};
use log::{debug, error, warn};
use serde_json::json;

use crate::{
  audio::backend::StdoutSink,
  ipc::{self, packet::IpcPacket},
  librespot_ext::discovery::CredentialsExt,
  utils,
};

pub struct SpoticordPlayer {
  client: ipc::Client,
  session: Option<Session>,
  spirc: Option<Spirc>,
}

impl SpoticordPlayer {
  pub fn new(client: ipc::Client) -> Self {
    Self {
      client,
      session: None,
      spirc: None,
    }
  }

  pub async fn start(&mut self, token: impl Into<String>, device_name: impl Into<String>) {
    let token = token.into();

    // Get the username (required for librespot)
    let username = utils::spotify::get_username(&token)
      .await
      .expect("to get the username");

    let session_config = SessionConfig::default();
    let player_config = PlayerConfig {
      bitrate: Bitrate::Bitrate96,
      ..PlayerConfig::default()
    };

    // Log in using the token
    let credentials = Credentials::with_token(username, &token);

    // Shutdown old session (cannot be done in the stop function)
    if let Some(session) = self.session.take() {
      session.shutdown();
    }

    // Connect the session
    let (session, _) = match Session::connect(session_config, credentials, None, false).await {
      Ok((session, credentials)) => (session, credentials),
      Err(why) => {
        error!("Failed to create Spotify session: {}", why);

        self
          .client
          .send(IpcPacket::ConnectError(why.to_string()))
          .ok();
        return;
      }
    };

    // Store session for later use
    self.session = Some(session.clone());

    // Volume mixer
    let mixer = (mixer::find(Some("softvol")).expect("to exist"))(MixerConfig {
      volume_ctrl: librespot::playback::config::VolumeCtrl::Linear,
      ..MixerConfig::default()
    });

    let client = self.client.clone();

    // Create the player
    let (player, mut receiver) = Player::new(
      player_config,
      session.clone(),
      mixer.get_soft_volume(),
      move || Box::new(StdoutSink::new(client)),
    );

    let (spirc, spirc_task) = Spirc::new(
      ConnectConfig {
        name: device_name.into(),
        // 50%
        initial_volume: Some(65535 / 2),
        ..ConnectConfig::default()
      },
      session.clone(),
      player,
      mixer,
    );

    let device_id = session.device_id().to_owned();
    let ipc = self.client.clone();

    // IPC Handler
    tokio::spawn(async move {
      let client = reqwest::Client::new();

      let mut retries = 10;

      // Try to switch to the device
      loop {
        match client
          .put("https://api.spotify.com/v1/me/player")
          .bearer_auth(token.clone())
          .json(&json!({
            "device_ids": [device_id],
          }))
          .send()
          .await
        {
          Ok(resp) => {
            if resp.status() == 202 {
              debug!("Successfully switched to device");
              break;
            }

            retries -= 1;

            if retries == 0 {
              error!("Failed to switch to device");
              ipc
                .send(IpcPacket::ConnectError(
                  "Switch to Spoticord device timed out".to_string(),
                ))
                .ok();
              break;
            }
          }
          Err(why) => {
            error!("Failed to set device: {}", why);
            ipc.send(IpcPacket::ConnectError(why.to_string())).ok();
            break;
          }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      }

      // Do IPC stuff with these events
      loop {
        let event = match receiver.recv().await {
          Some(event) => event,
          None => break,
        };

        match event {
          PlayerEvent::Playing {
            play_request_id: _,
            track_id,
            position_ms,
            duration_ms,
          } => {
            if let Err(why) = ipc.send(IpcPacket::Playing(
              track_id.to_uri().expect("to not fail"),
              position_ms,
              duration_ms,
            )) {
              error!("Failed to send playing packet: {}", why);
            }
          }

          PlayerEvent::Paused {
            play_request_id: _,
            track_id,
            position_ms,
            duration_ms,
          } => {
            if let Err(why) = ipc.send(IpcPacket::Paused(
              track_id.to_uri().expect("to not fail"),
              position_ms,
              duration_ms,
            )) {
              error!("Failed to send paused packet: {}", why);
            }
          }

          PlayerEvent::Changed {
            old_track_id: _,
            new_track_id,
          } => {
            if let Err(why) = ipc.send(IpcPacket::TrackChange(
              new_track_id.to_uri().expect("to not fail"),
            )) {
              error!("Failed to send track change packet: {}", why);
            }
          }

          PlayerEvent::Stopped {
            play_request_id: _,
            track_id: _,
          } => {
            if let Err(why) = ipc.send(IpcPacket::Stopped) {
              error!("Failed to send player stopped packet: {}", why);
            }
          }

          _ => {}
        };
      }

      debug!("Player stopped");
    });

    self.spirc = Some(spirc);
    tokio::spawn(spirc_task);
  }

  pub fn next(&mut self) {
    if let Some(spirc) = &self.spirc {
      spirc.next();
    }
  }

  pub fn previous(&mut self) {
    if let Some(spirc) = &self.spirc {
      spirc.prev();
    }
  }

  pub fn pause(&mut self) {
    if let Some(spirc) = &self.spirc {
      spirc.pause();
    }
  }

  pub fn resume(&mut self) {
    if let Some(spirc) = &self.spirc {
      spirc.play();
    }
  }

  pub fn stop(&mut self) {
    if let Some(spirc) = self.spirc.take() {
      spirc.shutdown();
    }
  }
}

pub async fn main() {
  let args = std::env::args().collect::<Vec<String>>();

  let tx_name = args[2].clone();
  let rx_name = args[3].clone();

  // Create IPC communication channel
  let client = ipc::Client::connect(tx_name, rx_name).expect("Failed to connect to IPC");

  // Create the player
  let mut player = SpoticordPlayer::new(client.clone());

  loop {
    let message = match client.try_recv() {
      Ok(message) => message,
      Err(why) => {
        if let TryRecvError::Empty = why {
          // No message, wait a bit and try again
          tokio::time::sleep(Duration::from_millis(25)).await;

          continue;
        } else if let TryRecvError::IpcError(IpcError::Disconnected) = &why {
          debug!("IPC connection closed, goodbye");
          break;
        }

        error!("Failed to receive message: {}", why);
        break;
      }
    };

    match message {
      IpcPacket::Connect(token, device_name) => {
        debug!("Connecting to Spotify with device name {}", device_name);

        player.start(token, device_name).await;
      }

      IpcPacket::Disconnect => {
        debug!("Disconnecting from Spotify");

        player.stop();
      }

      IpcPacket::Next => {
        player.next();
      }

      IpcPacket::Previous => {
        player.previous();
      }

      IpcPacket::Pause => {
        player.pause();
      }

      IpcPacket::Resume => {
        player.resume();
      }

      IpcPacket::Quit => {
        debug!("Received quit packet, exiting");

        exit(0);
      }

      _ => {
        warn!("Received unknown packet: {:?}", message);
      }
    }
  }
}
