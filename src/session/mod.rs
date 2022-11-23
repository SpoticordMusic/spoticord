use self::{
  manager::{SessionCreateError, SessionManager},
  pbi::PlaybackInfo,
};
use crate::{
  consts::DISCONNECT_TIME,
  database::{Database, DatabaseError},
  ipc::{self, packet::IpcPacket, Client},
  utils::{embed::Status, spotify},
};
use ipc_channel::ipc::{IpcError, TryRecvError};
use librespot::core::spotify_id::{SpotifyAudioType, SpotifyId};
use log::*;
use serenity::{
  async_trait,
  http::Http,
  model::prelude::{ChannelId, GuildId, UserId},
  prelude::{Context, RwLock},
};
use songbird::{
  create_player,
  input::{children_to_reader, Codec, Container, Input},
  tracks::TrackHandle,
  Call, Event, EventContext, EventHandler,
};
use std::{
  process::{Command, Stdio},
  sync::Arc,
  time::Duration,
};
use tokio::sync::Mutex;

pub mod manager;
mod pbi;

#[derive(Clone)]
pub struct SpoticordSession(Arc<RwLock<InnerSpoticordSession>>);

struct InnerSpoticordSession {
  owner: Option<UserId>,
  guild_id: GuildId,
  channel_id: ChannelId,
  text_channel_id: ChannelId,

  http: Arc<Http>,

  session_manager: SessionManager,

  call: Arc<Mutex<Call>>,
  track: Option<TrackHandle>,

  playback_info: Option<PlaybackInfo>,

  disconnect_handle: Option<tokio::task::JoinHandle<()>>,

  client: Option<Client>,
}

impl SpoticordSession {
  pub async fn new(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    text_channel_id: ChannelId,
    owner_id: UserId,
  ) -> Result<SpoticordSession, SessionCreateError> {
    // Get the Spotify token of the owner
    let data = ctx.data.read().await;
    let session_manager = data.get::<SessionManager>().unwrap().clone();

    // Join the voice channel
    let songbird = songbird::get(ctx).await.unwrap().clone();

    let (call, result) = songbird.join(guild_id, channel_id).await;

    if let Err(why) = result {
      error!("Error joining voice channel: {:?}", why);
      return Err(SessionCreateError::JoinError(channel_id, guild_id));
    }

    let inner = InnerSpoticordSession {
      owner: Some(owner_id.clone()),
      guild_id,
      channel_id,
      text_channel_id,
      http: ctx.http.clone(),
      session_manager: session_manager.clone(),
      call: call.clone(),
      track: None,
      playback_info: None,
      disconnect_handle: None,
      client: None,
    };

    let mut instance = Self(Arc::new(RwLock::new(inner)));

    instance.create_player(ctx).await?;

    let mut call = call.lock().await;

    // Set up events
    call.add_global_event(
      songbird::Event::Core(songbird::CoreEvent::DriverDisconnect),
      instance.clone(),
    );

    call.add_global_event(
      songbird::Event::Core(songbird::CoreEvent::ClientDisconnect),
      instance.clone(),
    );

    Ok(instance)
  }

  pub async fn update_owner(
    &mut self,
    ctx: &Context,
    owner_id: UserId,
  ) -> Result<(), SessionCreateError> {
    // Get the Spotify token of the owner
    let data = ctx.data.read().await;
    let session_manager = data.get::<SessionManager>().unwrap().clone();

    {
      let mut inner = self.0.write().await;
      inner.owner = Some(owner_id);
    }

    {
      let inner = self.0.clone();
      let inner = inner.read().await;
      session_manager.set_owner(owner_id, inner.guild_id).await;
    }

    // Create the player
    self.create_player(ctx).await?;

    Ok(())
  }

  async fn create_player(&mut self, ctx: &Context) -> Result<(), SessionCreateError> {
    let owner_id = match self.owner().await.clone() {
      Some(owner_id) => owner_id,
      None => return Err(SessionCreateError::NoOwnerError),
    };

    let data = ctx.data.read().await;
    let database = data.get::<Database>().unwrap();

    let token = match database.get_access_token(owner_id.to_string()).await {
      Ok(token) => token,
      Err(why) => {
        if let DatabaseError::InvalidStatusCode(code) = why {
          if code == 404 {
            return Err(SessionCreateError::NoSpotifyError);
          }
        }

        return Err(SessionCreateError::DatabaseError);
      }
    };

    let user = match database.get_user(owner_id.to_string()).await {
      Ok(user) => user,
      Err(why) => {
        error!("Failed to get user: {:?}", why);
        return Err(SessionCreateError::DatabaseError);
      }
    };

    // Create IPC oneshot server
    let (server, tx_name, rx_name) = match ipc::Server::create() {
      Ok(server) => server,
      Err(why) => {
        error!("Failed to create IPC server: {:?}", why);
        return Err(SessionCreateError::ForkError);
      }
    };

    // Spawn player process
    let child = match Command::new(std::env::current_exe().unwrap())
      .args([
        "--player",
        &tx_name,
        &rx_name,
        "--debug-guild-id",
        &self.guild_id().await.to_string(),
      ])
      .stdout(Stdio::piped())
      .stderr(Stdio::inherit())
      .spawn()
    {
      Ok(child) => child,
      Err(why) => {
        error!("Failed to start player process: {:?}", why);
        return Err(SessionCreateError::ForkError);
      }
    };

    // Establish bi-directional IPC channel
    let client = match server.accept() {
      Ok(client) => client,
      Err(why) => {
        error!("Failed to accept IPC connection: {:?}", why);

        return Err(SessionCreateError::ForkError);
      }
    };

    // Pipe player audio to the voice channel
    let reader = children_to_reader::<f32>(vec![child]);

    // Create track (paused, fixes audio glitches)
    let (mut track, track_handle) =
      create_player(Input::new(true, reader, Codec::Pcm, Container::Raw, None));
    track.pause();

    let call = self.call().await;
    let mut call = call.lock().await;

    // Set call audio to track
    call.play_only(track);

    // Handle IPC packets
    // This will automatically quit once the IPC connection is closed
    tokio::spawn({
      let track = track_handle.clone();
      let client = client.clone();
      let ctx = ctx.clone();
      let instance = self.clone();
      let inner = self.0.clone();

      async move {
        let check_result = |result| {
          if let Err(why) = result {
            error!("Failed to issue track command: {:?}", why);
          }
        };

        loop {
          // Required for IpcPacket::TrackChange to work
          tokio::task::yield_now().await;

          let msg = match client.try_recv() {
            Ok(msg) => msg,
            Err(why) => {
              if let TryRecvError::Empty = why {
                // No message, wait a bit and try again
                tokio::time::sleep(Duration::from_millis(25)).await;

                continue;
              } else if let TryRecvError::IpcError(why) = &why {
                if let IpcError::Disconnected = why {
                  trace!("IPC connection closed, exiting IPC handler");
                  break;
                }
              }

              error!("Failed to receive IPC message: {:?}", why);
              break;
            }
          };

          trace!("Received IPC message: {:?}", msg);

          match msg {
            // Session connect error
            IpcPacket::ConnectError(why) => {
              error!("Failed to connect to Spotify: {:?}", why);

              // Notify the user in the text channel
              if let Err(why) = instance
                .text_channel_id()
                .await
                .send_message(&instance.http().await, |message| {
                  message.embed(|embed| {
                    embed.title("Failed to connect to Spotify");
                    embed.description(why);
                    embed.color(Status::Error as u64);

                    embed
                  });

                  message
                })
                .await
              {
                error!("Failed to send error message: {:?}", why);
              }

              // Clean up session
              instance.player_stopped().await;

              break;
            }

            // Sink requests playback to start/resume
            IpcPacket::StartPlayback => {
              check_result(track.play());
            }

            // Sink requests playback to pause
            IpcPacket::StopPlayback => {
              check_result(track.pause());
            }

            // A new track has been set by the player
            IpcPacket::TrackChange(track) => {
              // Convert to SpotifyId
              let track_id = SpotifyId::from_uri(&track).unwrap();

              let instance = instance.clone();
              let ctx = ctx.clone();

              // Fetch track info
              // This is done in a separate task to avoid blocking the IPC handler
              tokio::spawn(async move {
                if let Err(why) = instance.update_track(&ctx, &owner_id, track_id).await {
                  error!("Failed to update track: {:?}", why);

                  instance.player_stopped().await;
                }
              });
            }

            // The player has started playing a track
            IpcPacket::Playing(track, position_ms, duration_ms) => {
              // Convert to SpotifyId
              let track_id = SpotifyId::from_uri(&track).unwrap();

              let was_none = instance
                .update_playback(duration_ms, position_ms, true)
                .await;

              if was_none {
                // Stop player if update track fails
                if let Err(why) = instance.update_track(&ctx, &owner_id, track_id).await {
                  error!("Failed to update track: {:?}", why);

                  instance.player_stopped().await;
                  return;
                }
              }
            }

            IpcPacket::Paused(track, position_ms, duration_ms) => {
              instance.start_disconnect_timer().await;

              // Convert to SpotifyId
              let track_id = SpotifyId::from_uri(&track).unwrap();

              let was_none = instance
                .update_playback(duration_ms, position_ms, false)
                .await;

              if was_none {
                // Stop player if update track fails

                if let Err(why) = instance.update_track(&ctx, &owner_id, track_id).await {
                  error!("Failed to update track: {:?}", why);

                  instance.player_stopped().await;
                  return;
                }
              }
            }

            IpcPacket::Stopped => {
              check_result(track.pause());

              {
                let mut inner = inner.write().await;
                inner.playback_info.take();
              }

              instance.start_disconnect_timer().await;
            }

            // Ignore other packets
            _ => {}
          }
        }
      }
    });

    // Inform the player process to connect to Spotify
    if let Err(why) = client.send(IpcPacket::Connect(token, user.device_name)) {
      error!("Failed to send IpcPacket::Connect packet: {:?}", why);
    }

    // Update inner client and track
    let mut inner = self.0.write().await;
    inner.track = Some(track_handle);
    inner.client = Some(client);

    Ok(())
  }

  /// Update current track
  async fn update_track(
    &self,
    ctx: &Context,
    owner_id: &UserId,
    spotify_id: SpotifyId,
  ) -> Result<(), String> {
    let should_update = {
      let pbi = self.playback_info().await;

      if let Some(pbi) = pbi {
        pbi.spotify_id.is_none() || pbi.spotify_id.unwrap() != spotify_id
      } else {
        false
      }
    };

    if !should_update {
      return Ok(());
    }

    let data = ctx.data.read().await;
    let database = data.get::<Database>().unwrap();

    let token = match database.get_access_token(&owner_id.to_string()).await {
      Ok(token) => token,
      Err(why) => {
        error!("Failed to get access token: {:?}", why);
        return Err("Failed to get access token".to_string());
      }
    };

    let mut track: Option<spotify::Track> = None;
    let mut episode: Option<spotify::Episode> = None;

    if spotify_id.audio_type == SpotifyAudioType::Track {
      let track_info = match spotify::get_track_info(&token, spotify_id).await {
        Ok(track) => track,
        Err(why) => {
          error!("Failed to get track info: {:?}", why);
          return Err("Failed to get track info".to_string());
        }
      };

      trace!("Received track info: {:?}", track_info);

      track = Some(track_info);
    } else if spotify_id.audio_type == SpotifyAudioType::Podcast {
      let episode_info = match spotify::get_episode_info(&token, spotify_id).await {
        Ok(episode) => episode,
        Err(why) => {
          error!("Failed to get episode info: {:?}", why);
          return Err("Failed to get episode info".to_string());
        }
      };

      trace!("Received episode info: {:?}", episode_info);

      episode = Some(episode_info);
    }

    // Update track/episode
    let mut inner = self.0.write().await;

    if let Some(pbi) = inner.playback_info.as_mut() {
      pbi.update_track_episode(spotify_id, track, episode);
    }

    Ok(())
  }

  /// Called when the player must stop, but not leave the call
  async fn player_stopped(&self) {
    let mut inner = self.0.write().await;

    if let Some(client) = inner.client.take() {
      // Ask player to quit (will cause defunct process)
      if let Err(why) = client.send(IpcPacket::Quit) {
        error!("Failed to send quit packet: {:?}", why);
      }
    }

    if let Some(track) = inner.track.take() {
      // Stop the playback, and freeing the child handle, removing the defunct process
      if let Err(why) = track.stop() {
        error!("Failed to stop track: {:?}", why);
      }
    }

    // Clear owner
    if let Some(owner_id) = inner.owner.take() {
      inner.session_manager.remove_owner(owner_id).await;
    }

    // Clear playback info
    inner.playback_info = None;

    // Unlock to prevent deadlock in start_disconnect_timer
    drop(inner);

    // Disconnect automatically after some time
    self.start_disconnect_timer().await;
  }

  // Disconnect from voice channel and remove session from manager
  pub async fn disconnect(&self) {
    info!(
      "[{}] Disconnecting from voice channel {}",
      self.guild_id().await,
      self.channel_id().await
    );

    // We must run disconnect_no_abort within a read lock
    // This is because `SessionManager::remove_session` will acquire a
    //  read lock to read the current owner.
    // This would deadlock if we have an active write lock
    {
      let inner = self.0.read().await;
      inner.disconnect_no_abort().await;
    }

    // Stop the disconnect timer, if one is running
    let mut inner = self.0.write().await;
    if let Some(handle) = inner.disconnect_handle.take() {
      handle.abort();
    }
  }

  // Update playback info (duration, position, playing state)
  async fn update_playback(&self, duration_ms: u32, position_ms: u32, playing: bool) -> bool {
    let is_none = {
      let pbi = self.playback_info().await;

      pbi.is_none()
    };

    let mut inner = self.0.write().await;

    if is_none {
      inner.playback_info = Some(PlaybackInfo::new(duration_ms, position_ms, playing));
    } else {
      // Update position, duration and playback state
      inner
        .playback_info
        .as_mut()
        .unwrap()
        .update_pos_dur(position_ms, duration_ms, playing);
    };

    is_none
  }

  /// Start the disconnect timer, which will disconnect the bot from the voice channel after a
  /// certain amount of time
  async fn start_disconnect_timer(&self) {
    let inner_arc = self.0.clone();
    let mut inner = inner_arc.write().await;

    // Abort the previous timer, if one is running
    if let Some(handle) = inner.disconnect_handle.take() {
      handle.abort();
    }

    inner.disconnect_handle = Some(tokio::spawn({
      let inner = inner_arc.clone();
      let instance = self.clone();

      async move {
        let mut timer = tokio::time::interval(Duration::from_secs(DISCONNECT_TIME));

        // Ignore first (immediate) tick
        timer.tick().await;
        timer.tick().await;

        // Make sure this task has not been aborted, if it has this will automatically stop execution.
        tokio::task::yield_now().await;

        let is_playing = {
          let inner = inner.read().await;

          if let Some(ref pbi) = inner.playback_info {
            pbi.is_playing
          } else {
            false
          }
        };

        if !is_playing {
          info!("Player is not playing, disconnecting");
          instance
            .disconnect_with_message(
              "The player has been inactive for too long, and has been disconnected.",
            )
            .await;
        }
      }
    }));
  }

  pub async fn disconnect_with_message(&self, content: &str) {
    {
      let inner = self.0.read().await;

      // Firstly we disconnect
      inner.disconnect_no_abort().await;

      // Then we send the message
      if let Err(why) = inner
        .text_channel_id
        .send_message(&inner.http, |message| {
          message.embed(|embed| {
            embed.title("Disconnected from voice channel");
            embed.description(content);
            embed.color(Status::Warning as u64);

            embed
          })
        })
        .await
      {
        error!("Failed to send disconnect message: {:?}", why);
      }
    }

    // Finally we stop and remove the disconnect timer
    let mut inner = self.0.write().await;

    // Stop the disconnect timer, if one is running
    if let Some(handle) = inner.disconnect_handle.take() {
      handle.abort();
    }
  }

  /* Inner getters */

  /// Get the owner
  pub async fn owner(&self) -> Option<UserId> {
    self.0.read().await.owner
  }

  /// Get the session manager
  pub async fn session_manager(&self) -> SessionManager {
    self.0.read().await.session_manager.clone()
  }

  /// Get the guild id
  pub async fn guild_id(&self) -> GuildId {
    self.0.read().await.guild_id
  }

  /// Get the channel id
  pub async fn channel_id(&self) -> ChannelId {
    self.0.read().await.channel_id
  }

  /// Get the channel id
  pub async fn text_channel_id(&self) -> ChannelId {
    self.0.read().await.text_channel_id
  }

  /// Get the playback info
  pub async fn playback_info(&self) -> Option<PlaybackInfo> {
    self.0.read().await.playback_info.clone()
  }

  pub async fn call(&self) -> Arc<Mutex<Call>> {
    self.0.read().await.call.clone()
  }

  pub async fn http(&self) -> Arc<Http> {
    self.0.read().await.http.clone()
  }
}

impl InnerSpoticordSession {
  /// Internal version of disconnect, which does not abort the disconnect timer
  async fn disconnect_no_abort(&self) {
    self.session_manager.remove_session(self.guild_id).await;

    let mut call = self.call.lock().await;

    if let Some(ref track) = self.track {
      track.stop().unwrap_or(());
    }

    call.remove_all_global_events();

    if let Err(why) = call.leave().await {
      error!("Failed to leave voice channel: {:?}", why);
    }
  }
}

impl Drop for InnerSpoticordSession {
  fn drop(&mut self) {
    trace!("Dropping inner session");
  }
}

#[async_trait]
impl EventHandler for SpoticordSession {
  async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
    match ctx {
      EventContext::DriverDisconnect(_) => {
        debug!("Driver disconnected, leaving voice channel");
        self.disconnect().await;
      }
      EventContext::ClientDisconnect(who) => {
        trace!("Client disconnected, {}", who.user_id.to_string());

        if let Some(session) = self
          .session_manager()
          .await
          .find(UserId(who.user_id.0))
          .await
        {
          if session.guild_id().await == self.guild_id().await
            && session.channel_id().await == self.channel_id().await
          {
            self.player_stopped().await;
          }
        }
      }
      _ => {}
    }

    return None;
  }
}
