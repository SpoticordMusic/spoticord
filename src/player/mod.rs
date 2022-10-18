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
    player::Player,
  },
};
use log::{debug, error, info, trace, warn};
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
}

impl SpoticordPlayer {
  pub fn create(client: ipc::Client) -> Self {
    Self {
      client,
      session: None,
    }
  }

  pub async fn start(&mut self, token: impl Into<String>, device_name: impl Into<String>) {
    let token = token.into();

    // Get the username (required for librespot)
    let username = utils::spotify::get_username(&token).await.unwrap();

    let session_config = SessionConfig::default();
    let player_config = PlayerConfig {
      bitrate: Bitrate::Bitrate96,
      ..PlayerConfig::default()
    };

    // Log in using the token
    let credentials = Credentials::with_token(username, &token);

    // Connect the session
    let (session, _) = match Session::connect(session_config, credentials, None, false).await {
      Ok((session, credentials)) => (session, credentials),
      Err(why) => panic!("Failed to connect: {}", why),
    };

    // Store session for later use
    self.session = Some(session.clone());

    // Volume mixer
    let mixer = (mixer::find(Some("softvol")).unwrap())(MixerConfig::default());

    let client = self.client.clone();

    // Create the player
    let (player, _) = Player::new(
      player_config,
      session.clone(),
      mixer.get_soft_volume(),
      move || Box::new(StdoutSink::new(client)),
    );

    let mut receiver = player.get_player_event_channel();

    let (_, spirc_run) = Spirc::new(
      ConnectConfig {
        name: device_name.into(),
        initial_volume: Some(65535),
        ..ConnectConfig::default()
      },
      session.clone(),
      player,
      mixer,
    );

    let device_id = session.device_id().to_owned();

    // IPC Handler
    tokio::spawn(async move {
      let client = reqwest::Client::new();

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
              info!("Successfully switched to device");
              break;
            }
          }
          Err(why) => {
            debug!("Failed to set device: {}", why);
            break;
          }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      }

      // TODO: Do IPC stuff with these events
      loop {
        let event = match receiver.recv().await {
          Some(event) => event,
          None => break,
        };

        trace!("Player event: {:?}", event);
      }

      info!("Player stopped");
    });

    tokio::spawn(spirc_run);
  }

  pub fn stop(&mut self) {
    if let Some(session) = self.session.take() {
      session.shutdown();
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
  let mut player = SpoticordPlayer::create(client.clone());

  loop {
    let message = match client.recv() {
      Ok(message) => message,
      Err(why) => {
        error!("Failed to receive message: {}", why);
        break;
      }
    };

    match message {
      IpcPacket::Connect(token, device_name) => {
        info!("Connecting to Spotify with device name {}", device_name);

        player.start(token, device_name).await;
      }

      IpcPacket::Disconnect => {
        info!("Disconnecting from Spotify");

        player.stop();
      }

      IpcPacket::Quit => {
        debug!("Received quit packet, exiting");

        player.stop();
        break;
      }

      _ => {
        warn!("Received unknown packet: {:?}", message);
      }
    }
  }

  info!("We're done here, shutting down...");
}
