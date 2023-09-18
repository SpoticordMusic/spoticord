pub mod stream;

use librespot::{
  connect::spirc::Spirc,
  core::{config::ConnectConfig, session::Session},
  discovery::Credentials,
  playback::{
    config::{Bitrate, PlayerConfig, VolumeCtrl},
    mixer::{self, MixerConfig},
    player::{Player as SpotifyPlayer, PlayerEvent},
  },
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
  audio::{SinkEvent, StreamSink},
  librespot_ext::discovery::CredentialsExt,
  utils,
};

use self::stream::Stream;

pub struct Player {
  stream: Stream,
  session: Option<Session>,
}

impl Player {
  pub fn create() -> Self {
    Self {
      stream: Stream::new(),
      session: None,
    }
  }

  pub async fn start(
    &mut self,
    token: &str,
    device_name: &str,
  ) -> Result<
    (
      Spirc,
      (UnboundedReceiver<PlayerEvent>, UnboundedReceiver<SinkEvent>),
    ),
    Box<dyn std::error::Error>,
  > {
    let username = utils::spotify::get_username(token).await?;

    let player_config = PlayerConfig {
      bitrate: Bitrate::Bitrate96,
      ..Default::default()
    };

    let credentials = Credentials::with_token(username, token);

    // Shutdown old session (cannot be done in the stop function)
    if let Some(session) = self.session.take() {
      session.shutdown()
    }

    // Connect the session
    let (session, _) = Session::connect(Default::default(), credentials, None, false).await?;
    self.session = Some(session.clone());

    let mixer = (mixer::find(Some("softvol")).expect("to exist"))(MixerConfig {
      volume_ctrl: VolumeCtrl::Linear,
      ..Default::default()
    });

    let stream = self.get_stream();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (player, receiver) = SpotifyPlayer::new(
      player_config,
      session.clone(),
      mixer.get_soft_volume(),
      move || Box::new(StreamSink::new(stream, tx)),
    );

    let (spirc, spirc_task) = Spirc::new(
      ConnectConfig {
        name: device_name.into(),
        // 50%
        initial_volume: Some(65535 / 2),
        ..Default::default()
      },
      session.clone(),
      player,
      mixer,
    );

    tokio::spawn(spirc_task);

    Ok((spirc, (receiver, rx)))
  }

  pub fn get_stream(&self) -> Stream {
    self.stream.clone()
  }
}
