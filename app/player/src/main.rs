mod audio;
mod event_handler;
mod player;

use std::{num::NonZeroU64, time::Duration};

use anyhow::{Result, bail};
use librespot::discovery::Credentials;
use log::{debug, info, warn};
use songbird::{
    Config, ConnectionInfo, CoreEvent, Driver, Event,
    id::{ChannelId, GuildId, UserId},
};
use spoticord_ipc::{
    self as ipc, IpcReader, IpcWriter,
    packet::{BotMessage, PlayerMessage, PlayerMessageEvent, PlayerMessageResponse},
};
use tokio::{
    io::{Stdin, Stdout, stdin, stdout},
    sync::mpsc::Receiver,
};
use tokio_stream::StreamExt;

use crate::{
    event_handler::{CallEvent, CallEventHandler},
    player::{Player, PlayerEvent, PlayerHandle},
};

#[tokio::main(worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    // Force aws-lc-rs as default crypto provider
    // Since multiple dependencies either enable aws_lc_rs or ring, they cause a clash, so we have to
    // explicitly tell rustls to use the aws-lc-rs provider
    _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Setup logging
    unsafe {
        #[cfg(debug_assertions)]
        std::env::set_var("RUST_LOG", "spoticord_player=trace");

        #[cfg(not(debug_assertions))]
        std::env::set_var("RUST_LOG", "spoticord_player=info");
    }

    env_logger::init();

    debug!("Initializing IPC...");

    let mut writer = ipc::writer(stdout());
    let mut reader = ipc::reader(stdin());

    debug!("IPC ready, waiting for initialize command...");

    let connect_info = match tokio::time::timeout(Duration::from_secs(2), reader.next())
        .await
        .expect("IPC timed out") // This operation is crucial, so a timeout would be a critical error, so we panic
    {
        Some(Ok(BotMessage::Initialize {
            guild_id,
            channel_id,
            endpoint,
            token,
            session_id,
            user_id,
        })) => {
            let guild_id = GuildId::from(
                NonZeroU64::new(guild_id).ok_or_else(|| anyhow::anyhow!("Invalid Guild ID"))?,
            );
            let channel_id = ChannelId::from(
                NonZeroU64::new(channel_id).ok_or_else(|| anyhow::anyhow!("Invalid Channel ID"))?,
            );
            let user_id = UserId::from(
                NonZeroU64::new(user_id).ok_or_else(|| anyhow::anyhow!("Invalid User ID"))?,
            );

            ConnectionInfo {
                endpoint,
                token,
                session_id,
                channel_id: Some(channel_id),
                guild_id,
                user_id,
            }
        }
        Some(Ok(_)) => bail!("Invalid initial IPC packet received"),
        Some(Err(why)) => bail!("Received malformed IPC packet: {why}"),
        None => bail!("IPC connection closed"),
    };

    // Connect to voice server

    debug!("Connecting to voice server...");

    let mut driver = Driver::new(Config::default());

    if let Err(why) = driver.connect(connect_info).await {
        writer
            .send_message(&PlayerMessage::Response(PlayerMessageResponse::Error(
                format!("{why}"),
            )))
            .await?;

        return Err(why.into());
    }

    // Set up call events
    let (call_evt, call_evt_rx) = CallEventHandler::create();

    driver.add_global_event(Event::Core(CoreEvent::DriverDisconnect), call_evt.clone());
    driver.add_global_event(Event::Core(CoreEvent::DriverConnect), call_evt.clone());
    driver.add_global_event(Event::Core(CoreEvent::DriverReconnect), call_evt.clone());
    driver.add_global_event(Event::Core(CoreEvent::ClientDisconnect), call_evt);

    // Notify bot that we are ready
    writer
        .send_message(&PlayerMessage::Response(PlayerMessageResponse::Ready))
        .await?;

    info!("Player fully initialized");

    EventLoop::new(driver, call_evt_rx, writer, reader)
        .run()
        .await?;

    info!("Player shutting down");

    // Force shut down the player in the case the bot process is clinging on to our handles
    std::process::exit(0);
}

struct EventLoop {
    driver: Driver,
    call_events: Receiver<CallEvent>,
    writer: IpcWriter<Stdout>,
    reader: IpcReader<Stdin, BotMessage>,

    owner: Option<u64>,
    player: Option<PlayerHandle>,
    player_rx: Option<Receiver<PlayerEvent>>,
}

impl EventLoop {
    fn new(
        driver: Driver,
        call_events: Receiver<CallEvent>,
        writer: IpcWriter<Stdout>,
        reader: IpcReader<Stdin, BotMessage>,
    ) -> Self {
        Self {
            driver,
            call_events,
            writer,
            reader,

            owner: None,
            player: None,
            player_rx: None,
        }
    }

    async fn run(mut self) -> Result<()> {
        loop {
            tokio::select! {
                // Handle incoming IPC messages
                message = self.reader.next() => {
                    let Some(message) = message else {
                        debug!("IPC connection closed, shutting down");

                        self.stop_player().await;
                        break;
                    };

                    let message = match message {
                        Ok(message) => message,
                        Err(why) => bail!("Pipe read error: {why}"),
                    };

                    if !self.handle_ipc_command(message).await {
                        break;
                    }
                }

                // Handle Discord call events (player connect, disconnect, etc)
                event = self.call_events.recv() => {
                    let Some(event) = event else {
                        debug!("Discord voice connection dropped, shutting down");

                        self.stop_player().await;
                        break;
                    };

                    if !self.handle_call_event(event).await {
                        break;
                    }
                }

                // Handle player events (if a player is active)
                event = async {
                    let Some(player_rx) = self.player_rx.as_mut() else {
                        unreachable!();
                    };

                    player_rx.recv().await
                }, if self.player_rx.is_some() => {
                    let Some(event) = event else {
                        warn!("Player event receiver died unexpectedly");

                        self.stop_player().await;

                        continue;
                    };

                    self.handle_player_event(event).await;
                }
            }
        }

        _ = self.driver.leave();
        _ = self
            .writer
            .send_message(&PlayerMessage::Event(PlayerMessageEvent::Shutdown))
            .await;

        // Wait for response or channel shutdown, timing out after 3 seconds
        _ = tokio::time::timeout(Duration::from_secs(3), self.reader.next()).await;

        Ok(())
    }

    async fn handle_ipc_command(&mut self, message: BotMessage) -> bool {
        match message {
            BotMessage::SetUser {
                user_id,
                credentials,
                device_name,
            } => {
                // Create a new player, replacing the current one if one already exists
                self.stop_player_silent().await;

                // Create new player, sending an error back to the bot on failure
                let (player, player_rx) = match Player::create(
                    &mut self.driver,
                    Credentials::with_access_token(credentials.expose()),
                    device_name,
                )
                .await
                {
                    Ok(player) => player,
                    Err(why) => {
                        _ = self
                            .writer
                            .send_message(&PlayerMessage::Response(PlayerMessageResponse::Error(
                                format!("{why}"),
                            )))
                            .await;

                        return true;
                    }
                };

                info!("Player connected to Spotify");

                _ = self
                    .writer
                    .send_message(&PlayerMessage::Response(PlayerMessageResponse::Connected))
                    .await;

                self.player = Some(player);
                self.player_rx = Some(player_rx);
                self.owner = Some(user_id);
            }

            BotMessage::Initialize { .. } => {
                warn!("Duplicate initialize command received, ignoring")
            }

            BotMessage::Disconnect => self.stop_player_silent().await,

            BotMessage::Shutdown => {
                self.stop_player().await;

                return false;
            }

            BotMessage::Play => {
                if let Some(player) = &self.player {
                    _ = player.play().await;
                }
            }

            BotMessage::Pause => {
                if let Some(player) = &self.player {
                    _ = player.pause().await;
                }
            }

            BotMessage::NextTrack => {
                if let Some(player) = &self.player {
                    _ = player.next_track().await;
                }
            }

            BotMessage::PreviousTrack => {
                if let Some(player) = &self.player {
                    _ = player.previous_track().await;
                }
            }
        }

        true
    }

    async fn handle_call_event(&mut self, event: CallEvent) -> bool {
        match event {
            CallEvent::DriverDisconnect => {
                info!("Bot disconnected from voice channel, stopping player");

                return false;
            }

            CallEvent::ClientDisconnect(user) => {
                // If user is owner, stop playback, end librespot session, continue loop
                debug!(
                    "User disconnected: {user}, current owner: {:?}, player active: {}",
                    self.owner,
                    self.player.is_some()
                );

                if let Some(owner) = self.owner
                    && owner == user.0
                {
                    info!("Shutting down player because owner disconnected");

                    self.stop_player().await;
                }
            }
        }

        true
    }

    async fn handle_player_event(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::TrackChanged(info) => {
                _ = self
                    .writer
                    .send_message(&PlayerMessage::Event(PlayerMessageEvent::Update {
                        info,
                        track_changed: true,
                    }))
                    .await;
            }

            PlayerEvent::Play | PlayerEvent::Pause | PlayerEvent::Seeked => {
                let Some(player) = &self.player else {
                    return;
                };

                let Ok(Some(info)) = player.get_info().await else {
                    return;
                };

                _ = self
                    .writer
                    .send_message(&PlayerMessage::Event(PlayerMessageEvent::Update {
                        info: Box::new(info),
                        track_changed: false,
                    }))
                    .await;
            }

            PlayerEvent::Stopped | PlayerEvent::ConnectionReset => {
                _ = self
                    .writer
                    .send_message(&PlayerMessage::Event(PlayerMessageEvent::Disconnected))
                    .await;
            }
        }
    }

    async fn stop_player(&mut self) {
        self.stop_player_silent().await;

        _ = self
            .writer
            .send_message(&PlayerMessage::Event(PlayerMessageEvent::Disconnected))
            .await;
    }

    async fn stop_player_silent(&mut self) {
        if let Some(player) = self.player.take() {
            _ = player.shutdown().await;
        }

        self.player_rx = None;
        self.owner = None;
    }
}
