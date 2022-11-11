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
pub struct SpoticordSession {
  owner: Arc<RwLock<Option<UserId>>>,
  guild_id: GuildId,
  channel_id: ChannelId,
  text_channel_id: ChannelId,

  http: Arc<Http>,

  session_manager: SessionManager,

  call: Arc<Mutex<Call>>,
  track: TrackHandle,

  playback_info: Arc<RwLock<Option<PlaybackInfo>>>,

  disconnect_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,

  client: Client,
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
    let database = data.get::<Database>().unwrap();
    let session_manager = data.get::<SessionManager>().unwrap().clone();

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

    // Join the voice channel
    let songbird = songbird::get(ctx).await.unwrap().clone();

    let (call, result) = songbird.join(guild_id, channel_id).await;

    if let Err(why) = result {
      error!("Error joining voice channel: {:?}", why);
      return Err(SessionCreateError::JoinError(channel_id, guild_id));
    }

    let mut call_mut = call.lock().await;

    // Spawn player process
    let child = match Command::new(std::env::current_exe().unwrap())
      .args(["--player", &tx_name, &rx_name])
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

    // Set call audio to track
    call_mut.play_only(track);

    let instance = Self {
      owner: Arc::new(RwLock::new(Some(owner_id.clone()))),
      guild_id,
      channel_id,
      text_channel_id,
      http: ctx.http.clone(),
      session_manager: session_manager.clone(),
      call: call.clone(),
      track: track_handle.clone(),
      playback_info: Arc::new(RwLock::new(None)),
      disconnect_handle: Arc::new(Mutex::new(None)),
      client: client.clone(),
    };

    // Clone variables for use in the IPC handler
    let ipc_track = track_handle.clone();
    let ipc_client = client.clone();
    let ipc_context = ctx.clone();
    let mut ipc_instance = instance.clone();

    // Handle IPC packets
    // This will automatically quit once the IPC connection is closed
    tokio::spawn(async move {
      let check_result = |result| {
        if let Err(why) = result {
          error!("Failed to issue track command: {:?}", why);
        }
      };

      loop {
        // Required for IpcPacket::TrackChange to work
        tokio::task::yield_now().await;

        let msg = match ipc_client.try_recv() {
          Ok(msg) => msg,
          Err(why) => {
            if let TryRecvError::Empty = why {
              // No message, wait a bit and try again
              tokio::time::sleep(Duration::from_millis(25)).await;

              continue;
            } else if let TryRecvError::IpcError(why) = &why {
              if let IpcError::Disconnected = why {
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
            if let Err(why) = ipc_instance
              .text_channel_id
              .send_message(&ipc_instance.http, |message| {
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
            ipc_instance.player_stopped().await;

            break;
          }

          // Sink requests playback to start/resume
          IpcPacket::StartPlayback => {
            check_result(ipc_track.play());
          }

          // Sink requests playback to pause
          IpcPacket::StopPlayback => {
            check_result(ipc_track.pause());
          }

          // A new track has been set by the player
          IpcPacket::TrackChange(track) => {
            // Convert to SpotifyId
            let track_id = SpotifyId::from_uri(&track).unwrap();

            let mut instance = ipc_instance.clone();
            let context = ipc_context.clone();

            // Fetch track info
            // This is done in a separate task to avoid blocking the IPC handler
            tokio::spawn(async move {
              if let Err(why) = instance.update_track(&context, &owner_id, track_id).await {
                error!("Failed to update track: {:?}", why);

                instance.player_stopped().await;
              }
            });
          }

          // The player has started playing a track
          IpcPacket::Playing(track, position_ms, duration_ms) => {
            // Convert to SpotifyId
            let track_id = SpotifyId::from_uri(&track).unwrap();

            let was_none = ipc_instance
              .update_playback(duration_ms, position_ms, true)
              .await;

            if was_none {
              // Stop player if update track fails
              if let Err(why) = ipc_instance
                .update_track(&ipc_context, &owner_id, track_id)
                .await
              {
                error!("Failed to update track: {:?}", why);

                ipc_instance.player_stopped().await;
                return;
              }
            }
          }

          IpcPacket::Paused(track, position_ms, duration_ms) => {
            ipc_instance.start_disconnect_timer().await;

            // Convert to SpotifyId
            let track_id = SpotifyId::from_uri(&track).unwrap();

            let was_none = ipc_instance
              .update_playback(duration_ms, position_ms, false)
              .await;

            if was_none {
              // Stop player if update track fails
              if let Err(why) = ipc_instance
                .update_track(&ipc_context, &owner_id, track_id)
                .await
              {
                error!("Failed to update track: {:?}", why);

                ipc_instance.player_stopped().await;
                return;
              }
            }
          }

          IpcPacket::Stopped => {
            check_result(ipc_track.pause());

            ipc_instance.playback_info.write().await.take();
            ipc_instance.start_disconnect_timer().await;
          }

          // Ignore other packets
          _ => {}
        }
      }
    });

    // Set up events
    call_mut.add_global_event(
      songbird::Event::Core(songbird::CoreEvent::DriverDisconnect),
      instance.clone(),
    );

    call_mut.add_global_event(
      songbird::Event::Core(songbird::CoreEvent::ClientDisconnect),
      instance.clone(),
    );

    // Inform the player process to connect to Spotify
    if let Err(why) = client.send(IpcPacket::Connect(token, user.device_name)) {
      error!("Failed to send IpcPacket::Connect packet: {:?}", why);
    }

    Ok(instance)
  }

  pub async fn update_owner(
    &self,
    ctx: &Context,
    owner_id: UserId,
  ) -> Result<(), SessionCreateError> {
    // Get the Spotify token of the owner
    let data = ctx.data.read().await;
    let database = data.get::<Database>().unwrap();
    let mut session_manager = data.get::<SessionManager>().unwrap().clone();

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

    {
      let mut owner = self.owner.write().await;
      *owner = Some(owner_id);
    }

    session_manager.set_owner(owner_id, self.guild_id).await;

    // Inform the player process to connect to Spotify
    if let Err(why) = self
      .client
      .send(IpcPacket::Connect(token, user.device_name))
    {
      error!("Failed to send IpcPacket::Connect packet: {:?}", why);
    }

    Ok(())
  }

  // Update current track
  async fn update_track(
    &self,
    ctx: &Context,
    owner_id: &UserId,
    spotify_id: SpotifyId,
  ) -> Result<(), String> {
    let should_update = {
      let pbi = self.playback_info.read().await;

      if let Some(pbi) = &*pbi {
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

    let mut pbi = self.playback_info.write().await;

    if let Some(pbi) = &mut *pbi {
      pbi.update_track_episode(spotify_id, track, episode);
    }

    Ok(())
  }

  /// Called when the player must stop, but not leave the call
  async fn player_stopped(&mut self) {
    if let Err(why) = self.track.pause() {
      error!("Failed to pause track: {:?}", why);
    }

    // Disconnect from Spotify
    if let Err(why) = self.client.send(IpcPacket::Disconnect) {
      error!("Failed to send disconnect packet: {:?}", why);
    }

    // Clear owner
    let mut owner = self.owner.write().await;
    if let Some(owner_id) = owner.take() {
      self.session_manager.remove_owner(owner_id).await;
    }

    // Clear playback info
    let mut playback_info = self.playback_info.write().await;
    *playback_info = None;

    // Disconnect automatically after some time
    self.start_disconnect_timer().await;
  }

  /// Internal version of disconnect, which does not abort the disconnect timer
  async fn disconnect_no_abort(&self) {
    self
      .session_manager
      .clone()
      .remove_session(self.guild_id)
      .await;

    let mut call = self.call.lock().await;

    self.track.stop().unwrap_or(());
    call.remove_all_global_events();

    if let Err(why) = call.leave().await {
      error!("Failed to leave voice channel: {:?}", why);
    }
  }

  // Disconnect from voice channel and remove session from manager
  pub async fn disconnect(&self) {
    info!("Disconnecting from voice channel {}", self.channel_id);

    self.disconnect_no_abort().await;

    // Stop the disconnect timer, if one is running
    let mut dc_handle = self.disconnect_handle.lock().await;

    if let Some(handle) = dc_handle.take() {
      handle.abort();
    }
  }

  /// Disconnect from voice channel with a message
  pub async fn disconnect_with_message(&self, content: &str) {
    self.disconnect_no_abort().await;

    if let Err(why) = self
      .text_channel_id
      .send_message(&self.http, |message| {
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

    // Stop the disconnect timer, if one is running
    let mut dc_handle = self.disconnect_handle.lock().await;

    if let Some(handle) = dc_handle.take() {
      handle.abort();
    }
  }

  // Update playback info (duration, position, playing state)
  async fn update_playback(&self, duration_ms: u32, position_ms: u32, playing: bool) -> bool {
    let is_none = {
      let pbi = self.playback_info.read().await;

      pbi.is_none()
    };

    if is_none {
      let mut pbi = self.playback_info.write().await;
      *pbi = Some(PlaybackInfo::new(duration_ms, position_ms, playing));
    } else {
      let mut pbi = self.playback_info.write().await;

      // Update position, duration and playback state
      pbi
        .as_mut()
        .unwrap()
        .update_pos_dur(position_ms, duration_ms, playing)
        .await;
    };

    is_none
  }

  /// Start the disconnect timer, which will disconnect the bot from the voice channel after a
  /// certain amount of time
  async fn start_disconnect_timer(&self) {
    let pbi = self.playback_info.clone();
    let instance = self.clone();

    let mut handle = self.disconnect_handle.lock().await;

    // Abort the previous timer, if one is running
    if let Some(handle) = handle.take() {
      handle.abort();
    }

    *handle = Some(tokio::spawn(async move {
      let mut timer = tokio::time::interval(Duration::from_secs(DISCONNECT_TIME));

      // Ignore first (immediate) tick
      timer.tick().await;
      timer.tick().await;

      // Make sure this task has not been aborted, if it has this will automatically stop execution.
      tokio::task::yield_now().await;

      let is_playing = {
        let pbi = pbi.read().await;

        if let Some(pbi) = &*pbi {
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
    }));
  }

  // Get the playback info for the current track
  pub async fn get_playback_info(&self) -> Option<PlaybackInfo> {
    self.playback_info.read().await.clone()
  }

  // Get the current owner of this session
  pub async fn get_owner(&self) -> Option<UserId> {
    let owner = self.owner.read().await;

    *owner
  }

  // Get the server id this session is playing in
  pub fn get_guild_id(&self) -> GuildId {
    self.guild_id
  }

  // Get the channel id this session is playing in
  pub fn get_channel_id(&self) -> ChannelId {
    self.channel_id
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

        if let Some(session) = self.session_manager.find(UserId(who.user_id.0)).await {
          if session.get_guild_id() == self.guild_id && session.get_channel_id() == self.channel_id
          {
            // Clone because haha immutable references
            self.clone().player_stopped().await;
          }
        }
      }
      _ => {}
    }

    return None;
  }
}
