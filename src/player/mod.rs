use std::{io::Write, sync::Arc};

use anyhow::{anyhow, Result};
use librespot::{
  connect::spirc::Spirc,
  core::{
    config::{ConnectConfig, SessionConfig},
    session::Session,
    spotify_id::{SpotifyAudioType, SpotifyId},
  },
  discovery::Credentials,
  playback::{
    config::{Bitrate, PlayerConfig, VolumeCtrl},
    mixer::{self, MixerConfig},
    player::{Player as SpotifyPlayer, PlayerEvent as SpotifyEvent},
  },
  protocol::metadata::{Episode, Track},
};
use log::error;
use protobuf::Message;
use songbird::tracks::TrackHandle;
use tokio::sync::{
  broadcast::{Receiver, Sender},
  mpsc::UnboundedReceiver,
  Mutex,
};

use crate::{
  audio::{stream::Stream, SinkEvent, StreamSink},
  librespot_ext::discovery::CredentialsExt,
  session::pbi::{CurrentTrack, PlaybackInfo},
  utils,
};

enum Event {
  Player(SpotifyEvent),
  Sink(SinkEvent),
  Command(PlayerCommand),
}

#[derive(Clone)]
enum PlayerCommand {
  Next,
  Previous,
  Pause,
  Play,
  Shutdown,
}

#[derive(Clone, Debug)]
pub enum PlayerEvent {
  Pause,
  Play,
  Stopped,
}

#[derive(Clone)]
pub struct Player {
  tx: Sender<PlayerCommand>,

  pbi: Arc<Mutex<Option<PlaybackInfo>>>,
}

impl Player {
  pub async fn create(
    stream: Stream,
    token: &str,
    device_name: &str,
    track: TrackHandle,
  ) -> Result<(Self, Receiver<PlayerEvent>)> {
    let username = utils::spotify::get_username(token).await?;

    let player_config = PlayerConfig {
      bitrate: Bitrate::Bitrate96,
      ..Default::default()
    };

    let credentials = Credentials::with_token(username, token);

    let (session, _) = Session::connect(
      SessionConfig {
        ap_port: Some(9999), // Force the use of ap.spotify.com, which has the lowest latency
        ..Default::default()
      },
      credentials,
      None,
      false,
    )
    .await?;

    let mixer = (mixer::find(Some("softvol")).expect("to exist"))(MixerConfig {
      volume_ctrl: VolumeCtrl::Linear,
      ..Default::default()
    });

    let (tx, rx_sink) = tokio::sync::mpsc::unbounded_channel();
    let (player, rx_player) =
      SpotifyPlayer::new(player_config, session.clone(), mixer.get_soft_volume(), {
        let stream = stream.clone();
        move || Box::new(StreamSink::new(stream, tx))
      });

    let (spirc, spirc_task) = Spirc::new(
      ConnectConfig {
        name: device_name.into(),
        // 50%
        initial_volume: Some(65535 / 2),
        // Default Spotify behaviour
        autoplay: true,
        ..Default::default()
      },
      session.clone(),
      player,
      mixer,
    );

    let (tx, rx) = tokio::sync::broadcast::channel(10);
    let (tx_ev, rx_ev) = tokio::sync::broadcast::channel(10);
    let pbi = Arc::new(Mutex::new(None));

    let player_task = PlayerTask {
      pbi: pbi.clone(),
      session: session.clone(),
      rx_player,
      rx_sink,
      rx,
      tx: tx_ev,
      spirc,
      track,
      stream,
    };

    tokio::spawn(spirc_task);
    tokio::spawn(player_task.run());

    Ok((Self { pbi, tx }, rx_ev))
  }

  pub fn next(&self) {
    self.tx.send(PlayerCommand::Next).ok();
  }

  pub fn prev(&self) {
    self.tx.send(PlayerCommand::Previous).ok();
  }

  pub fn pause(&self) {
    self.tx.send(PlayerCommand::Pause).ok();
  }

  pub fn play(&self) {
    self.tx.send(PlayerCommand::Play).ok();
  }

  pub fn shutdown(&self) {
    self.tx.send(PlayerCommand::Shutdown).ok();
  }

  pub async fn pbi(&self) -> Option<PlaybackInfo> {
    self.pbi.lock().await.as_ref().cloned()
  }
}

struct PlayerTask {
  stream: Stream,
  session: Session,
  spirc: Spirc,
  track: TrackHandle,

  rx_player: UnboundedReceiver<SpotifyEvent>,
  rx_sink: UnboundedReceiver<SinkEvent>,
  rx: Receiver<PlayerCommand>,
  tx: Sender<PlayerEvent>,

  pbi: Arc<Mutex<Option<PlaybackInfo>>>,
}

impl PlayerTask {
  pub async fn run(mut self) {
    let check_result = |result| {
      if let Err(why) = result {
        error!("Failed to issue track command: {:?}", why);
      }
    };

    loop {
      match self.next().await {
        // Spotify player events
        Some(Event::Player(event)) => match event {
          SpotifyEvent::Playing {
            play_request_id: _,
            track_id,
            position_ms,
            duration_ms,
          } => {
            self
              .update_pbi(track_id, position_ms, duration_ms, true)
              .await;

            self.tx.send(PlayerEvent::Play).ok();
          }

          SpotifyEvent::Paused {
            play_request_id: _,
            track_id,
            position_ms,
            duration_ms,
          } => {
            self
              .update_pbi(track_id, position_ms, duration_ms, false)
              .await;

            self.tx.send(PlayerEvent::Pause).ok();
          }

          SpotifyEvent::Changed {
            old_track_id: _,
            new_track_id,
          } => {
            if let Ok(current) = self.resolve_audio_info(new_track_id).await {
              let mut pbi = self.pbi.lock().await;

              if let Some(pbi) = pbi.as_mut() {
                pbi.update_track(new_track_id, current);
              }
            }
          }

          SpotifyEvent::Stopped {
            play_request_id: _,
            track_id: _,
          } => {
            check_result(self.track.pause());

            self.tx.send(PlayerEvent::Pause).ok();
          }

          _ => {}
        },

        // Audio sink events
        Some(Event::Sink(event)) => match event {
          SinkEvent::Start => {
            check_result(self.track.play());
          }

          SinkEvent::Stop => {
            // EXPERIMENT: It may be beneficial to *NOT* pause songbird here
            // We already have a fallback if no audio is present in the buffer (write all zeroes aka silence)
            // So commenting this out may help prevent a substantial portion of jitter
            // This comes at a cost of more bandwidth, though opus should compress it down to almost nothing

            // check_result(track.pause());

            self.tx.send(PlayerEvent::Pause).ok();
          }
        },

        // The `Player` has instructed us to do something
        Some(Event::Command(command)) => match command {
          PlayerCommand::Next => self.spirc.next(),
          PlayerCommand::Previous => self.spirc.prev(),
          PlayerCommand::Pause => self.spirc.pause(),
          PlayerCommand::Play => self.spirc.play(),
          PlayerCommand::Shutdown => break,
        },

        None => {
          // One of the channels died
          log::debug!("Channel died");
          break;
        }
      }
    }

    self.tx.send(PlayerEvent::Stopped).ok();
  }

  async fn next(&mut self) -> Option<Event> {
    tokio::select! {
      event = self.rx_player.recv() => {
        event.map(Event::Player)
      }

      event = self.rx_sink.recv() => {
        event.map(Event::Sink)
      }

      command = self.rx.recv() => {
        command.ok().map(Event::Command)
      }
    }
  }

  /// Update current playback info, or return early if not necessary
  async fn update_pbi(
    &self,
    spotify_id: SpotifyId,
    position_ms: u32,
    duration_ms: u32,
    playing: bool,
  ) {
    let mut pbi = self.pbi.lock().await;

    if let Some(pbi) = pbi.as_mut() {
      pbi.update_pos_dur(position_ms, duration_ms, playing);
    }

    if !pbi
      .as_ref()
      .map(|pbi| pbi.spotify_id == spotify_id)
      .unwrap_or(true)
    {
      return;
    }

    if let Ok(current) = self.resolve_audio_info(spotify_id).await {
      match pbi.as_mut() {
        Some(pbi) => {
          pbi.update_track(spotify_id, current);
          pbi.update_pos_dur(position_ms, duration_ms, playing);
        }
        None => {
          *pbi = Some(PlaybackInfo::new(
            duration_ms,
            position_ms,
            playing,
            current,
            spotify_id,
          ));
        }
      }
    } else {
      log::error!("Failed to resolve audio info");
    }
  }

  /// Retrieve the metadata for a `SpotifyId`
  async fn resolve_audio_info(&self, spotify_id: SpotifyId) -> Result<CurrentTrack> {
    match spotify_id.audio_type {
      SpotifyAudioType::Track => self.resolve_track_info(spotify_id).await,
      SpotifyAudioType::Podcast => self.resolve_episode_info(spotify_id).await,
      SpotifyAudioType::NonPlayable => Err(anyhow!("Cannot resolve non-playable audio type")),
    }
  }

  /// Retrieve the metadata for a Spotify Track
  async fn resolve_track_info(&self, spotify_id: SpotifyId) -> Result<CurrentTrack> {
    let result = self
      .session
      .mercury()
      .get(format!("hm://metadata/3/track/{}", spotify_id.to_base16()?))
      .await
      .map_err(|_| anyhow!("Mercury metadata request failed"))?;

    if result.status_code != 200 {
      return Err(anyhow!("Mercury metadata request invalid status code"));
    }

    let message = match result.payload.get(0) {
      Some(v) => v,
      None => return Err(anyhow!("Mercury metadata request invalid payload")),
    };

    let proto_track = Track::parse_from_bytes(message)?;

    Ok(CurrentTrack::Track(proto_track))
  }

  /// Retrieve the metadata for a Spotify Podcast
  async fn resolve_episode_info(&self, spotify_id: SpotifyId) -> Result<CurrentTrack> {
    let result = self
      .session
      .mercury()
      .get(format!(
        "hm://metadata/3/episode/{}",
        spotify_id.to_base16()?
      ))
      .await
      .map_err(|_| anyhow!("Mercury metadata request failed"))?;

    if result.status_code != 200 {
      return Err(anyhow!("Mercury metadata request invalid status code"));
    }

    let message = match result.payload.get(0) {
      Some(v) => v,
      None => return Err(anyhow!("Mercury metadata request invalid payload")),
    };

    let proto_episode = Episode::parse_from_bytes(message)?;

    Ok(CurrentTrack::Episode(proto_episode))
  }
}

impl Drop for PlayerTask {
  fn drop(&mut self) {
    log::trace!("drop PlayerTask");

    self.track.stop().ok();
    self.spirc.shutdown();
    self.session.shutdown();
    self.stream.flush().ok();
  }
}
