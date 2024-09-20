pub mod info;

use anyhow::Result;
use info::PlaybackInfo;
use librespot::{
    connect::{config::ConnectConfig, spirc::Spirc},
    core::{http_client::HttpClientError, Session as SpotifySession, SessionConfig},
    discovery::Credentials,
    metadata::Lyrics,
    playback::{
        config::{Bitrate, PlayerConfig, VolumeCtrl},
        mixer::{self, MixerConfig},
        player::{Player as SpotifyPlayer, PlayerEvent as SpotifyPlayerEvent},
    },
};
use log::error;
use songbird::{input::RawAdapter, tracks::TrackHandle, Call};
use spoticord_audio::{
    sink::{SinkEvent, StreamSink},
    stream::Stream,
};
use std::{io::Write, sync::Arc};
use tokio::sync::{mpsc, oneshot, Mutex};

#[derive(Debug)]
enum PlayerCommand {
    NextTrack,
    PreviousTrack,
    Pause,
    Play,

    GetPlaybackInfo(oneshot::Sender<Option<PlaybackInfo>>),
    GetLyrics(oneshot::Sender<Option<Lyrics>>),

    Shutdown,
}

#[derive(Debug)]
pub enum PlayerEvent {
    Pause,
    Play,
    Stopped,
    TrackChanged(Box<PlaybackInfo>),
}

pub struct Player {
    session: SpotifySession,
    spirc: Spirc,
    track: TrackHandle,
    stream: Stream,

    playback_info: Option<PlaybackInfo>,

    // Communication
    events: mpsc::Sender<PlayerEvent>,

    commands: mpsc::Receiver<PlayerCommand>,
    spotify_events: mpsc::UnboundedReceiver<SpotifyPlayerEvent>,
    sink_events: mpsc::UnboundedReceiver<SinkEvent>,
}

impl Player {
    pub async fn create(
        credentials: Credentials,
        call: Arc<Mutex<Call>>,
        device_name: impl Into<String>,
    ) -> Result<(PlayerHandle, mpsc::Receiver<PlayerEvent>)> {
        let (event_tx, event_rx) = mpsc::channel(16);

        let mut call_lock = call.lock().await;
        let stream = Stream::new();

        // Create songbird audio track
        let adapter = RawAdapter::new(stream.clone(), 44100, 2);
        let track = call_lock.play_only_input(adapter.into());
        track.pause()?;

        // Free call lock before creating session
        drop(call_lock);

        // Create librespot audio streamer
        let session = SpotifySession::new(SessionConfig::default(), None);
        let mixer = (mixer::find(Some("softvol")).expect("missing softvol mixer"))(MixerConfig {
            volume_ctrl: VolumeCtrl::Log(VolumeCtrl::DEFAULT_DB_RANGE),
            ..Default::default()
        });

        let (tx_sink, rx_sink) = mpsc::unbounded_channel();
        let player = SpotifyPlayer::new(
            PlayerConfig {
                // 96kbps causes audio key errors, so enjoy the quality upgrade
                bitrate: Bitrate::Bitrate160,
                ..Default::default()
            },
            session.clone(),
            mixer.get_soft_volume(),
            {
                let stream = stream.clone();
                move || Box::new(StreamSink::new(stream, tx_sink))
            },
        );
        let rx_player = player.get_player_event_channel();

        let device_name = device_name.into();
        let mut tries = 0;

        let (spirc, spirc_task) = loop {
            match Spirc::new(
                ConnectConfig {
                    name: device_name.clone(),
                    initial_volume: Some((0.75f32 * u16::MAX as f32) as u16),
                    ..Default::default()
                },
                session.clone(),
                credentials.clone(),
                player.clone(),
                mixer.clone(),
            )
            .await
            {
                Ok(spirc) => break spirc,
                Err(why) => {
                    tries += 1;
                    if tries > 3 {
                        error!("Failed to connect to Spirc: {why}");

                        return Err(why.into());
                    }

                    continue;
                }
            }
        };

        let (tx, rx) = mpsc::channel(16);
        let player = Self {
            session,
            spirc,
            track,
            stream,

            playback_info: None,

            events: event_tx,

            commands: rx,
            spotify_events: rx_player,
            sink_events: rx_sink,
        };

        // Launch it all!
        tokio::spawn(spirc_task);
        tokio::spawn(player.run());

        Ok((PlayerHandle { commands: tx }, event_rx))
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                opt_command = self.commands.recv() => {
                    let command = match opt_command {
                        Some(command) => command,
                        None => break,
                    };

                    self.handle_command(command).await;
                },

                Some(event) = self.spotify_events.recv() => {
                    self.handle_spotify_event(event).await;
                },

                Some(event) = self.sink_events.recv() => {
                    self.handle_sink_event(event).await;
                }

                else => break,
            }
        }
    }

    async fn handle_command(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::NextTrack => _ = self.spirc.next(),
            PlayerCommand::PreviousTrack => _ = self.spirc.prev(),
            PlayerCommand::Pause => _ = self.spirc.pause(),
            PlayerCommand::Play => _ = self.spirc.play(),

            PlayerCommand::GetPlaybackInfo(tx) => _ = tx.send(self.playback_info.clone()),
            PlayerCommand::GetLyrics(tx) => self.get_lyrics(tx).await,

            PlayerCommand::Shutdown => self.commands.close(),
        };
    }

    async fn handle_spotify_event(&mut self, event: SpotifyPlayerEvent) {
        match event {
            SpotifyPlayerEvent::PositionCorrection { position_ms, .. }
            | SpotifyPlayerEvent::Seeked { position_ms, .. } => {
                if let Some(playback_info) = self.playback_info.as_mut() {
                    playback_info.update_playback(position_ms, true);
                }
            }
            SpotifyPlayerEvent::Playing { position_ms, .. } => {
                _ = self.events.send(PlayerEvent::Play).await;

                if let Some(playback_info) = self.playback_info.as_mut() {
                    playback_info.update_playback(position_ms, true);
                }
            }
            SpotifyPlayerEvent::Paused { position_ms, .. } => {
                _ = self.events.send(PlayerEvent::Pause).await;

                if let Some(playback_info) = self.playback_info.as_mut() {
                    playback_info.update_playback(position_ms, false);
                }
            }
            SpotifyPlayerEvent::Stopped { .. } | SpotifyPlayerEvent::SessionDisconnected { .. } => {
                if let Err(why) = self.track.pause() {
                    error!("Failed to pause songbird track: {why}");
                }

                _ = self.events.send(PlayerEvent::Pause).await;

                self.playback_info = None;
            }
            SpotifyPlayerEvent::TrackChanged { audio_item } => {
                if let Some(playback_info) = self.playback_info.as_mut() {
                    playback_info.update_track(*audio_item);
                } else {
                    self.playback_info = Some(PlaybackInfo::new(*audio_item, 0, false));
                }

                _ = self
                    .events
                    .send(PlayerEvent::TrackChanged(Box::new(
                        self.playback_info.clone().expect("playback info is None"),
                    )))
                    .await;
            }
            _ => {}
        }
    }

    async fn handle_sink_event(&self, event: SinkEvent) {
        if let SinkEvent::Start = event {
            if let Err(why) = self.track.play() {
                error!("Failed to resume songbird track: {why}");
            }
        }
    }

    /// Grab the lyrics for the current active track from Spotify.
    ///
    /// This might return None if nothing is being played, or the current song does not have any lyrics.
    async fn get_lyrics(&self, tx: oneshot::Sender<Option<Lyrics>>) {
        let Some(playback_info) = &self.playback_info else {
            _ = tx.send(None);
            return;
        };

        let lyrics = match Lyrics::get(&self.session, &playback_info.track_id()).await {
            Ok(lyrics) => lyrics,
            Err(why) => {
                // Ignore 404 errors
                match why.error.downcast_ref::<HttpClientError>() {
                    Some(HttpClientError::StatusCode(code)) if code.as_u16() == 404 => {}
                    _ => error!("Failed to get lyrics: {why}"),
                }

                _ = tx.send(None);
                return;
            }
        };

        _ = tx.send(Some(lyrics));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        _ = self.spirc.shutdown();
        _ = self.stream.flush();
    }
}

#[derive(Clone, Debug)]
pub struct PlayerHandle {
    commands: mpsc::Sender<PlayerCommand>,
}

impl PlayerHandle {
    pub fn is_valid(&self) -> bool {
        !self.commands.is_closed()
    }

    pub async fn next_track(&self) {
        _ = self.commands.send(PlayerCommand::NextTrack).await;
    }

    pub async fn previous_track(&self) {
        _ = self.commands.send(PlayerCommand::PreviousTrack).await;
    }

    pub async fn pause(&self) {
        _ = self.commands.send(PlayerCommand::Pause).await;
    }

    pub async fn play(&self) {
        _ = self.commands.send(PlayerCommand::Play).await;
    }

    pub async fn playback_info(&self) -> Result<Option<PlaybackInfo>> {
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(PlayerCommand::GetPlaybackInfo(tx))
            .await?;

        Ok(rx.await?)
    }

    pub async fn get_lyrics(&self) -> Result<Option<Lyrics>> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(PlayerCommand::GetLyrics(tx)).await?;

        Ok(rx.await?)
    }

    pub async fn shutdown(&self) {
        _ = self.commands.send(PlayerCommand::Shutdown).await;
    }
}
