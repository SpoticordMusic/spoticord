use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use librespot::{
    connect::{ConnectConfig, Spirc},
    core::{Session, SessionConfig, SpotifyId, SpotifyUri, error::ErrorKind},
    discovery::Credentials,
    metadata::Lyrics,
    playback::{
        config::{Bitrate, PlayerConfig, VolumeCtrl},
        mixer::{self, MixerConfig},
        player::{Player as SpotifyPlayer, PlayerEvent as SpotifyPlayerEvent},
    },
};
use log::{debug, error};
use songbird::{Call, input::RawAdapter, tracks::TrackHandle};
use spoticord_shared::player_info::PlayerInfo;
use tokio::sync::{
    mpsc::{self, Receiver, Sender, UnboundedReceiver},
    oneshot,
};

/// Commands sent to the player event loop
#[derive(Debug)]
pub enum PlayerCommand {
    NextTrack,
    PreviousTrack,
    Pause,
    Play,

    GetInfo(oneshot::Sender<Option<PlayerInfo>>),

    Shutdown,
}

/// Events emitted by the player event loop
#[derive(Debug)]
pub enum PlayerEvent {
    Pause,
    Play,
    Stopped,
    Seeked,
    TrackChanged(Box<PlayerInfo>),
    ConnectionReset,
}

pub struct Player {
    session: Session,
    spirc: Spirc,
    track: TrackHandle,

    // State
    info: Option<PlayerInfo>,
    shutdown: Arc<AtomicBool>,

    // Channels
    event_tx: Sender<PlayerEvent>,
    command_rx: Receiver<PlayerCommand>,
    player_rx: UnboundedReceiver<SpotifyPlayerEvent>,
}

impl Player {
    pub async fn create(
        call: &mut Call,
        credentials: Credentials,
        device_name: String,
    ) -> Result<(PlayerHandle, Receiver<PlayerEvent>), librespot::core::Error> {
        let stream = crate::audio::Stream::new();

        // Create songbird audio track
        let adapter = RawAdapter::new(stream.clone(), 44100, 2);
        let track = call.play_only_input(adapter.into());
        _ = track.pause();

        // Create librespot session
        let session = Session::new(SessionConfig::default(), None);
        let mixer = (mixer::find(Some("softvol")).expect("missing softvol mixer"))(MixerConfig {
            volume_ctrl: VolumeCtrl::Log(VolumeCtrl::DEFAULT_DB_RANGE),
            ..Default::default()
        })?;

        let player = SpotifyPlayer::new(
            PlayerConfig {
                bitrate: Bitrate::Bitrate96,
                ..Default::default()
            },
            session.clone(),
            mixer.get_soft_volume(),
            move || Box::new(stream),
        );
        let player_rx = player.get_player_event_channel();

        let mut tries = 0;
        let (spirc, spirc_task) = loop {
            match Spirc::new(
                ConnectConfig {
                    name: device_name.clone(),
                    initial_volume: (0.75f32 * u16::MAX as f32) as u16,
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
                    // Instantly return if credentials are invalid
                    if let ErrorKind::PermissionDenied = why.kind {
                        return Err(why);
                    }

                    tries += 1;
                    if tries > 3 {
                        error!("Failed to connect to Spotify Play: {why}");

                        return Err(why);
                    }

                    tokio::time::sleep(Duration::from_secs(1)).await;

                    continue;
                }
            }
        };

        let (event_tx, event_rx) = mpsc::channel(16);
        let (command_tx, command_rx) = mpsc::channel(16);
        let shutdown = Arc::new(AtomicBool::new(false));

        let player = Self {
            session,
            spirc,
            track,

            info: None,
            shutdown: shutdown.clone(),

            event_tx: event_tx.clone(),
            player_rx,
            command_rx,
        };

        // Launch Spirc task
        tokio::spawn(async move {
            spirc_task.await;

            // If the shutdown flag isn't set, we most likely got kicked from the Spotify AP
            if !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                _ = event_tx.send(PlayerEvent::ConnectionReset).await;
            }
        });

        // Launch internal player event handling
        tokio::spawn(player.run());

        Ok((PlayerHandle { tx: command_tx }, event_rx))
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                command = self.command_rx.recv() => {
                    let Some(command) = command else {
                        break;
                    };

                    self.handle_command(command).await;
                }

                event = self.player_rx.recv() => {
                    let Some(event) = event else {
                        break;
                    };

                    self.handle_player_event(event).await;
                }
            }
        }

        debug!("Player is shutting down");

        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        _ = self.spirc.shutdown();
    }

    async fn handle_command(&mut self, command: PlayerCommand) {
        match command {
            PlayerCommand::NextTrack => _ = self.spirc.next(),
            PlayerCommand::PreviousTrack => _ = self.spirc.prev(),
            PlayerCommand::Pause => _ = self.spirc.pause(),
            PlayerCommand::Play => _ = self.spirc.play(),

            PlayerCommand::GetInfo(tx) => _ = tx.send(self.info.clone()),

            PlayerCommand::Shutdown => {
                debug!("Shutting down command receiver");

                self.command_rx.close();
            }
        }
    }

    async fn handle_player_event(&mut self, event: SpotifyPlayerEvent) {
        use SpotifyPlayerEvent as Event;

        match event {
            Event::PositionCorrection { position_ms, .. }
            | Event::PositionChanged { position_ms, .. }
            | Event::Seeked { position_ms, .. } => {
                if let Some(info) = self.info.as_mut() {
                    info.update_playback(position_ms, info.playing());
                }

                _ = self.event_tx.send(PlayerEvent::Seeked).await;
            }
            Event::Playing { position_ms, .. } => {
                if let Err(why) = self.track.play() {
                    error!("Failed to play songbird track: {why}");

                    // This is a fatal error, so shut down the player
                    self.command_rx.close();
                    return;
                }

                if let Some(info) = self.info.as_mut() {
                    info.update_playback(position_ms, true);
                }

                _ = self.event_tx.send(PlayerEvent::Play).await;
            }
            Event::Paused { position_ms, .. } => {
                if let Err(why) = self.track.pause() {
                    error!("Failed to pause songbird track: {why}");
                }

                if let Some(info) = self.info.as_mut() {
                    info.update_playback(position_ms, false);
                }

                _ = self.event_tx.send(PlayerEvent::Pause).await;
            }
            Event::Stopped { .. } => {
                if let Err(why) = self.track.pause() {
                    error!("Failed to pause songbird track: {why}");
                }

                self.info = None;

                _ = self.event_tx.send(PlayerEvent::Pause).await;
            }
            Event::SessionDisconnected { .. } => {
                if let Err(why) = self.track.pause() {
                    error!("Failed to pause songbird track: {why}");
                }

                self.info = None;

                _ = self.event_tx.send(PlayerEvent::Stopped).await;
                self.command_rx.close();
            }
            Event::TrackChanged { audio_item } => {
                let id = match audio_item.track_id {
                    SpotifyUri::Track { id } => id,
                    SpotifyUri::Episode { id } => id,
                    _ => SpotifyId { id: 0 },
                };

                let lyrics = Lyrics::get(&self.session, &id).await.ok().map(|l| l.into());

                if let Some(info) = self.info.as_mut() {
                    info.update_track(*audio_item, lyrics);
                    info.update_playback(0, info.playing());
                } else {
                    self.info = PlayerInfo::new(*audio_item, lyrics, 0, false);
                }

                _ = self
                    .event_tx
                    .send(PlayerEvent::TrackChanged(Box::new(
                        self.info.clone().expect("player info is None"),
                    )))
                    .await;
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
pub struct PlayerHandle {
    tx: mpsc::Sender<PlayerCommand>,
}

impl PlayerHandle {
    pub async fn next_track(&self) -> Result<(), mpsc::error::SendError<PlayerCommand>> {
        self.tx.send(PlayerCommand::NextTrack).await
    }

    pub async fn previous_track(&self) -> Result<(), mpsc::error::SendError<PlayerCommand>> {
        self.tx.send(PlayerCommand::PreviousTrack).await
    }

    pub async fn pause(&self) -> Result<(), mpsc::error::SendError<PlayerCommand>> {
        self.tx.send(PlayerCommand::Pause).await
    }

    pub async fn play(&self) -> Result<(), mpsc::error::SendError<PlayerCommand>> {
        self.tx.send(PlayerCommand::Play).await
    }

    pub async fn shutdown(&self) -> Result<(), mpsc::error::SendError<PlayerCommand>> {
        self.tx.send(PlayerCommand::Shutdown).await
    }

    pub async fn get_info(&self) -> anyhow::Result<Option<PlayerInfo>> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(PlayerCommand::GetInfo(tx)).await?;

        Ok(rx.await?)
    }
}
