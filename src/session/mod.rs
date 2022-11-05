use self::manager::{SessionCreateError, SessionManager};
use crate::{
  database::{Database, DatabaseError},
  ipc::{self, packet::IpcPacket, Client},
  utils::{self, spotify},
};
use ipc_channel::ipc::{IpcError, TryRecvError};
use librespot::core::spotify_id::{SpotifyAudioType, SpotifyId};
use log::*;
use serenity::{
  async_trait,
  model::prelude::{ChannelId, GuildId, UserId},
  prelude::{Context, RwLock},
};
use songbird::{
  create_player,
  input::{children_to_reader, Input},
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

#[derive(Clone)]
pub struct PlaybackInfo {
  last_updated: u128,
  position_ms: u32,

  pub track: Option<spotify::Track>,
  pub episode: Option<spotify::Episode>,
  pub spotify_id: Option<SpotifyId>,

  pub duration_ms: u32,
  pub is_playing: bool,
}

impl PlaybackInfo {
  fn new(duration_ms: u32, position_ms: u32, is_playing: bool) -> Self {
    Self {
      last_updated: utils::get_time_ms(),
      track: None,
      episode: None,
      spotify_id: None,
      duration_ms,
      position_ms,
      is_playing,
    }
  }

  // Update position, duration and playback state
  async fn update_pos_dur(&mut self, position_ms: u32, duration_ms: u32, is_playing: bool) {
    self.position_ms = position_ms;
    self.duration_ms = duration_ms;
    self.is_playing = is_playing;

    self.last_updated = utils::get_time_ms();
  }

  // Update spotify id, track and episode
  fn update_track_episode(
    &mut self,
    spotify_id: SpotifyId,
    track: Option<spotify::Track>,
    episode: Option<spotify::Episode>,
  ) {
    self.spotify_id = Some(spotify_id);
    self.track = track;
    self.episode = episode;
  }

  pub fn get_position(&self) -> u32 {
    if self.is_playing {
      let now = utils::get_time_ms();
      let diff = now - self.last_updated;

      self.position_ms + diff as u32
    } else {
      self.position_ms
    }
  }

  pub fn get_name(&self) -> Option<String> {
    if let Some(track) = &self.track {
      Some(track.name.clone())
    } else if let Some(episode) = &self.episode {
      Some(episode.name.clone())
    } else {
      None
    }
  }

  pub fn get_artists(&self) -> Option<String> {
    if let Some(track) = &self.track {
      Some(
        track
          .artists
          .iter()
          .map(|a| a.name.clone())
          .collect::<Vec<String>>()
          .join(", "),
      )
    } else if let Some(episode) = &self.episode {
      Some(episode.show.name.clone())
    } else {
      None
    }
  }

  pub fn get_thumbnail_url(&self) -> Option<String> {
    if let Some(track) = &self.track {
      let mut images = track.album.images.clone();
      images.sort_by(|a, b| b.width.cmp(&a.width));

      Some(images.get(0).unwrap().url.clone())
    } else if let Some(episode) = &self.episode {
      let mut images = episode.show.images.clone();
      images.sort_by(|a, b| b.width.cmp(&a.width));

      Some(images.get(0).unwrap().url.clone())
    } else {
      None
    }
  }
}

#[derive(Clone)]
pub struct SpoticordSession {
  owner: Arc<RwLock<Option<UserId>>>,
  guild_id: GuildId,
  channel_id: ChannelId,

  session_manager: SessionManager,

  call: Arc<Mutex<Call>>,
  track: TrackHandle,

  playback_info: Arc<RwLock<Option<PlaybackInfo>>>,

  client: Client,
}

impl SpoticordSession {
  pub async fn new(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
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
    let (mut track, track_handle) = create_player(Input::float_pcm(true, reader));
    track.pause();

    // Set call audio to track
    call_mut.play_only(track);

    let instance = Self {
      owner: Arc::new(RwLock::new(Some(owner_id.clone()))),
      guild_id,
      channel_id,
      session_manager: session_manager.clone(),
      call: call.clone(),
      track: track_handle.clone(),
      playback_info: Arc::new(RwLock::new(None)),
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
            ipc_instance.player_stopped().await;
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
  }

  // Disconnect from voice channel and remove session from manager
  pub async fn disconnect(&self) {
    info!("Disconnecting from voice channel {}", self.channel_id);

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
