pub mod error;
pub mod lyrics_embed;
pub mod manager;
pub mod playback_embed;

use anyhow::Result;
use log::{debug, error, warn};
use songbird::Call;
use spoticord_ipc::packet::{PlayerMessageEvent, PlayerMessageResponse};
use spoticord_shared::player_info::PlayerInfo;
use std::io;
use std::time::Duration;
use std::{fmt::Debug, sync::Arc};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use poise::serenity_prelude::{
    ChannelId, CommandInteraction, CreateEmbed, CreateMessage, GuildId, UserId,
};
use spoticord_ipc::{
    self as ipc, IpcError, IpcWriter,
    packet::{BotMessage, PlayerMessage},
};

use crate::session::lyrics_embed::LyricsEmbed;
use crate::session::playback_embed::{PlaybackEmbed, PlaybackEmbedHandle};
use crate::{
    discord::EmbedColor,
    session::{error::SessionError, manager::SessionManager},
};

#[derive(Debug)]
pub enum SessionCommand {
    Shutdown,
    Disconnect,
    Reactivate {
        new_owner: UserId,
        tx: oneshot::Sender<error::Result<()>>,
    },
    Query(SessionQueryCommand),
    Player(SessionPlayerCommand),

    CreatePlaybackEmbed(
        SessionHandle,
        Box<CommandInteraction>, // Large struct
        playback_embed::UpdateBehavior,
    ),
    CreateLyricsEmbed(SessionHandle, Box<CommandInteraction>),
}

#[derive(Debug)]
pub enum SessionQueryCommand {
    Active(oneshot::Sender<bool>),
    VoiceChannel(oneshot::Sender<ChannelId>),
    Owner(oneshot::Sender<UserId>),
    PlayerInfo(oneshot::Sender<Option<PlayerInfo>>),
}

#[derive(Debug)]
pub enum SessionPlayerCommand {
    Play,
    Pause,
    NextTrack,
    PreviousTrack,
}

pub struct Session {
    pub guild_id: GuildId,
    pub voice_channel_id: ChannelId,
    pub text_channel_id: ChannelId,
    pub owner_id: UserId,

    // State
    context: poise::serenity_prelude::Context,
    manager: Arc<SessionManager>,
    call: Arc<Mutex<Call>>,
    active: bool,
    timeout: Option<tokio::time::Instant>,
    player_info: Option<PlayerInfo>,

    // IPC
    player: IpcWriter<tokio::process::ChildStdin>,
    player_rx: ipc::stdio::PlayerIpcReader,

    // Actor-related
    command_rx: mpsc::Receiver<SessionCommand>,

    // Embeds
    playback_embed: Option<PlaybackEmbedHandle>,
    lyrics_embed: Option<JoinHandle<()>>,
}

impl Session {
    pub async fn create(
        context: poise::serenity_prelude::Context,
        guild_id: GuildId,
        voice_channel_id: ChannelId,
        text_channel_id: ChannelId,
        owner_id: UserId,
        manager: Arc<SessionManager>,
    ) -> error::Result<SessionHandle> {
        let owner = owner_id.to_string();

        // Resolve text channel
        let _ = text_channel_id
            .to_channel(&context)
            .await?
            .guild()
            .ok_or(SessionError::InvalidChannel)?;

        // Grab user information
        let _ = manager.database().get_account(&owner).await?;
        let device_name = manager.database().get_user(&owner).await?.device_name;
        let credentials = manager.spotify().get_access_token(&owner).await?;

        // Create player process and set up IPC
        let mut player_process = if std::env::var("CARGO").is_ok() {
            tokio::process::Command::new("cargo")
                .arg("run")
                .arg("-p")
                .arg("spoticord-player")
                .stdout(std::process::Stdio::piped())
                .stdin(std::process::Stdio::piped())
                .spawn()?
        } else {
            tokio::process::Command::new("spoticord-player")
                .stdout(std::process::Stdio::piped())
                .stdin(std::process::Stdio::piped())
                .spawn()?
        };

        let stdin = player_process.stdin.take().expect("Failed to take stdin");
        let stdout = player_process.stdout.take().expect("Failed to take stdout");

        let mut writer = ipc::writer(stdin);
        let mut reader = ipc::stdio::reader(ipc::reader::<_, PlayerMessage>(stdout));

        // Returning early during initialization without explicitly shutting down the player should be fine,
        // as a closed `stdin` will cause the player to error out anyways (so no need to manually kill it).

        // Join voice call and send initialization command to player
        let (info, call) = manager
            .songbird()
            .join_gateway(guild_id, voice_channel_id)
            .await?;

        // This code needs to run in the bot application, not in the player application
        {
            let mut call = call.lock().await;

            _ = call.deafen(true).await;
        }

        writer
            .send_message(&BotMessage::Initialize {
                guild_id: guild_id.get(),
                channel_id: voice_channel_id.get(),
                endpoint: info.endpoint,
                token: info.token,
                session_id: info.session_id,
                user_id: info.user_id.0.get(),
            })
            .await?;

        // Wait for ready message
        let timeout = if std::env::var("CARGO").is_ok() {
            120 // If we're developing, cargo might have to compile the player first
        } else {
            5
        };

        let Ok(response) =
            tokio::time::timeout(Duration::from_secs(timeout), reader.next_response()).await
        else {
            return Err(SessionError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc timed out during initialization",
            )));
        };

        match response {
            Some(PlayerMessageResponse::Ready) => {}
            Some(PlayerMessageResponse::Error(message)) => {
                return Err(SessionError::PlayerError(message));
            }
            Some(_) => {
                return Err(SessionError::PlayerError(
                    "Unknown response from player during intialization".into(),
                ));
            }
            None => {
                return Err(SessionError::Ipc(IpcError::Io(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ipc connection died",
                ))));
            }
        }

        // Now that the player is ready (connected to Discord), send initial authentication data
        writer
            .send_message(&BotMessage::SetUser {
                user_id: owner_id.get(),
                device_name,
                credentials: credentials.into(),
            })
            .await?;

        // Wait for success response
        let Ok(response) =
            tokio::time::timeout(Duration::from_secs(5), reader.next_response()).await
        else {
            return Err(SessionError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc timed out during initialization",
            )));
        };

        match response {
            Some(PlayerMessageResponse::Connected) => {}
            Some(PlayerMessageResponse::Error(message)) => {
                return Err(SessionError::PlayerError(message));
            }
            Some(_) => {
                return Err(SessionError::PlayerError(
                    "Unknown response from player during intialization".into(),
                ));
            }
            None => {
                return Err(SessionError::Ipc(IpcError::Io(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ipc connection died",
                ))));
            }
        }

        // Session is now set up and ready

        let (command_tx, command_rx) = mpsc::channel(16);

        let session = Self {
            guild_id,
            voice_channel_id,
            text_channel_id,
            owner_id,

            context,
            manager,
            call,
            active: true,
            timeout: None,
            player_info: None,

            player: writer,
            player_rx: reader,
            command_rx,

            playback_embed: None,
            lyrics_embed: None,
        };

        tokio::spawn(session.run());

        Ok(SessionHandle { tx: command_tx })
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                // Handle incoming commands
                command = self.command_rx.recv() => {
                    let Some(command) = command else {
                        break;
                    };

                    if !self.handle_command(command).await {
                        break;
                    }
                }

                // Handle player messages
                message = self.player_rx.next_event() => {
                    let Some(message) = message else {
                        break;
                    };

                    if !self.handle_player_message(message).await {
                                break;
                            }
                }

                _ = async {
                    if let Some(t) = self.timeout {
                        tokio::time::sleep_until(t).await;
                    }
                }, if self.timeout.is_some() => {
                    _ = self
                        .text_channel_id
                        .send_message(
                            &self.context,
                            CreateMessage::new().embed(
                                CreateEmbed::new()
                                    .title("It's a little quiet in here")
                                    .description(
                                        "The bot has been inactive for too long, and has been disconnected.",
                                    )
                                    .color(EmbedColor::Warning),
                            ),
                        )
                        .await;

                    break;
                }
            }
        }

        debug!("Session for guild {} has ended", self.guild_id);

        self.shutdown().await;
    }

    async fn handle_command(&mut self, command: SessionCommand) -> bool {
        match command {
            SessionCommand::Shutdown => {
                self.command_rx.close();

                return false;
            }
            SessionCommand::Disconnect => self.deactivate().await,
            SessionCommand::Reactivate { new_owner, tx } => {
                let result = self.reactivate(new_owner).await;
                let is_fatal = matches!(
                    &result,
                    Err(SessionError::Io(_)) | Err(SessionError::PlayerError(_))
                );

                _ = tx.send(result);

                // Certain errors indicate that the player is no longer considered stable
                // If this happens, we must end the session immediately
                if is_fatal {
                    return false;
                }
            }
            SessionCommand::Query(query) => self.handle_query(query).await,
            SessionCommand::Player(command) => {
                // Command fails if IPC is closed, in which case we just end the session
                if self.handle_player_command(command).await.is_err() {
                    return false;
                }
            }

            SessionCommand::CreatePlaybackEmbed(handle, interaction, behavior) => {
                match PlaybackEmbed::create(self, handle, *interaction, behavior).await {
                    Ok(opt_handle) => {
                        self.playback_embed = opt_handle;
                    }
                    Err(why) => {
                        error!("failed to create playing embed: {why}")
                    }
                };
            }

            SessionCommand::CreateLyricsEmbed(handle, interaction) => {
                match LyricsEmbed::create(self, handle, *interaction).await {
                    Ok(Some(lyrics_embed)) => {
                        if let Some(current) = self.lyrics_embed.take() {
                            current.abort();
                        }

                        self.lyrics_embed = Some(lyrics_embed);
                    }
                    Ok(None) => {}
                    Err(why) => {
                        error!("failed to create lyrics embed: {why}");
                    }
                }
            }
        }

        true
    }

    async fn handle_player_message(&mut self, message: PlayerMessageEvent) -> bool {
        let is_track_update = !matches!(
            message,
            PlayerMessageEvent::Update {
                track_changed: true,
                ..
            }
        );

        match message {
            PlayerMessageEvent::Shutdown => {
                // The player is shutting down, so we need to as well.
                self.command_rx.close();
                return false;
            }
            PlayerMessageEvent::Disconnected => self.deactivate().await,
            PlayerMessageEvent::Error(message) => {
                warn!("Received unexpected player error: {message}");
            }

            PlayerMessageEvent::Update { info, .. } => {
                self.player_info = Some(*info);
            }
        }

        // Update playback embed, setting it to None on failure
        if let Some(playback_embed) = &self.playback_embed
            && playback_embed.invoke_update(is_track_update).await.is_err()
        {
            self.playback_embed = None;
        }

        true
    }

    async fn handle_query(&mut self, query: SessionQueryCommand) {
        match query {
            SessionQueryCommand::Active(responder) => {
                _ = responder.send(self.active);
            }

            SessionQueryCommand::VoiceChannel(responder) => {
                _ = responder.send(self.voice_channel_id);
            }

            SessionQueryCommand::Owner(responder) => {
                _ = responder.send(self.owner_id);
            }

            SessionQueryCommand::PlayerInfo(responder) => {
                _ = responder.send(self.player_info.clone())
            }
        }
    }

    async fn handle_player_command(&mut self, command: SessionPlayerCommand) -> Result<()> {
        let message = match command {
            SessionPlayerCommand::Play => BotMessage::Play,
            SessionPlayerCommand::Pause => BotMessage::Pause,
            SessionPlayerCommand::NextTrack => BotMessage::NextTrack,
            SessionPlayerCommand::PreviousTrack => BotMessage::PreviousTrack,
        };

        self.player.send_message(&message).await?;

        Ok(())
    }

    async fn reactivate(&mut self, new_owner: UserId) -> error::Result<()> {
        use error::SessionError::*;

        let user_id = &*new_owner.to_string();

        if self.active {
            return Err(AlreadyActive);
        }

        // Grab user credentials and info before joining call

        let device_name = self.manager.database().get_user(user_id).await?.device_name;
        let credentials = self.manager.spotify().get_access_token(user_id).await?;

        // Send a new SetUser command to update the owner in the player process
        self.player
            .send_message(&BotMessage::SetUser {
                user_id: new_owner.get(),
                device_name,
                credentials: credentials.into(),
            })
            .await?;

        let Ok(response) =
            tokio::time::timeout(Duration::from_secs(5), self.player_rx.next_response()).await
        else {
            return Err(SessionError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "ipc timed out during reactivation",
            )));
        };

        match response {
            Some(PlayerMessageResponse::Connected) => {}
            Some(PlayerMessageResponse::Error(message)) => {
                return Err(SessionError::PlayerError(message));
            }
            Some(_) => {
                return Err(SessionError::PlayerError(
                    "Unknown response from player during intialization".into(),
                ));
            }
            None => {
                return Err(SessionError::Ipc(IpcError::Io(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ipc connection died",
                ))));
            }
        }

        self.active = true;
        self.owner_id = new_owner;
        self.timeout = None;
        self.manager.assign_owner(self.guild_id, self.owner_id);

        Ok(())
    }

    /// Deactivate the session and allow it to be reclaimed by a new owner
    pub async fn deactivate(&mut self) {
        _ = self.player.send_message(&BotMessage::Disconnect).await;

        self.player_info = None;
        self.active = false;
        self.timeout = Some(Instant::now() + Duration::from_secs(300));
        self.manager.remove_owner(self.guild_id, self.owner_id);
    }

    /// Send shutdown message to player and remove our session from the manager
    pub async fn shutdown(&mut self) {
        _ = self.player.send_message(&BotMessage::Shutdown).await;
        _ = self.call.lock().await.leave().await;

        // Stop lyrics background task
        if let Some(lyrics) = self.lyrics_embed.take() {
            lyrics.abort();
        }

        self.manager.remove_session(self.guild_id);
    }
}

impl Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("guild_id", &self.guild_id)
            .field("voice_channel_id", &self.voice_channel_id)
            .field("text_channel_id", &self.text_channel_id)
            .field("owner_id", &self.owner_id)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct SessionHandle {
    tx: mpsc::Sender<SessionCommand>,
}

impl SessionHandle {
    pub fn is_valid(&self) -> bool {
        !self.tx.is_closed()
    }

    /// Send a disconnect command to the session, allowing another user to take over the player
    pub async fn disconnect(&self) {
        _ = self.tx.send(SessionCommand::Disconnect).await;
    }

    /// Send a shutdown command to the session, shutting down the player and disconnecting the bot from the call
    pub async fn shutdown(&self) {
        _ = self.tx.send(SessionCommand::Shutdown).await;
    }

    /// Assign a new owner to the current session and reconnect to Spotify
    pub async fn reactivate(&self, new_owner: UserId) -> Result<error::Result<()>> {
        let (tx, rx) = oneshot::channel();

        self.tx
            .send(SessionCommand::Reactivate { new_owner, tx })
            .await
            .map_err(|_| anyhow::anyhow!("Session has been shut down"))?;

        Ok(rx.await?)
    }

    /// Start or resume playback
    pub async fn play(&self) {
        _ = self
            .tx
            .send(SessionCommand::Player(SessionPlayerCommand::Play))
            .await;
    }

    /// Pause playback
    pub async fn pause(&self) {
        _ = self
            .tx
            .send(SessionCommand::Player(SessionPlayerCommand::Pause))
            .await;
    }

    /// Advance to the next track in the queue
    pub async fn next_track(&self) {
        _ = self
            .tx
            .send(SessionCommand::Player(SessionPlayerCommand::NextTrack))
            .await;
    }

    /// Go back to the previous track in the queue
    pub async fn previous_track(&self) {
        _ = self
            .tx
            .send(SessionCommand::Player(SessionPlayerCommand::PreviousTrack))
            .await;
    }

    pub async fn active(&self) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::Query(SessionQueryCommand::Active(tx)))
            .await
            .map_err(|_| anyhow::anyhow!("Session has been shut down"))?;

        Ok(rx.await?)
    }

    pub async fn get_voice_channel_id(&self) -> Result<ChannelId> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::Query(SessionQueryCommand::VoiceChannel(tx)))
            .await
            .map_err(|_| anyhow::anyhow!("Session has been shut down"))?;

        Ok(rx.await?)
    }

    pub async fn get_owner_id(&self) -> Result<UserId> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::Query(SessionQueryCommand::Owner(tx)))
            .await
            .map_err(|_| anyhow::anyhow!("Session has been shut down"))?;

        Ok(rx.await?)
    }

    pub async fn get_player_info(&self) -> Result<Option<PlayerInfo>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::Query(SessionQueryCommand::PlayerInfo(tx)))
            .await
            .map_err(|_| anyhow::anyhow!("Session has been shut down"))?;

        Ok(rx.await?)
    }

    /// Create a playback embed as a response to an interaction
    ///
    /// This playback embed will automatically update when certain events happen
    pub async fn create_playback_embed(
        &self,
        interaction: &CommandInteraction,
        update_behavior: playback_embed::UpdateBehavior,
    ) -> Result<()> {
        self.tx
            .send(SessionCommand::CreatePlaybackEmbed(
                self.clone(),
                interaction.to_owned().into(),
                update_behavior,
            ))
            .await?;

        Ok(())
    }

    /// Create a lyrics embed as a response to an interaction
    ///
    /// This lyrics embed will automatically retrieve the lyrics and update the embed accordingly
    pub async fn create_lyrics_embed(&self, interaction: &CommandInteraction) -> Result<()> {
        self.tx
            .send(SessionCommand::CreateLyricsEmbed(
                self.clone(),
                interaction.to_owned().into(),
            ))
            .await?;

        Ok(())
    }
}
