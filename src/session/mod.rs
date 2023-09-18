pub mod manager;
pub mod pbi;

use self::{
  manager::{SessionCreateError, SessionManager},
  pbi::PlaybackInfo,
};
use crate::{
  audio::SinkEvent,
  consts::DISCONNECT_TIME,
  database::{Database, DatabaseError},
  player::Player,
  utils::{embed::Status, spotify},
};
use librespot::{
  connect::spirc::Spirc,
  core::spotify_id::{SpotifyAudioType, SpotifyId},
  playback::player::PlayerEvent,
};
use log::*;
use serenity::{
  async_trait,
  http::Http,
  model::prelude::{ChannelId, GuildId, UserId},
  prelude::{Context, RwLock},
};
use songbird::{
  create_player,
  input::{Codec, Container, Input, Reader},
  tracks::TrackHandle,
  Call, Event, EventContext, EventHandler,
};
use std::{
  io::Write,
  ops::{Deref, DerefMut},
  sync::Arc,
  time::Duration,
};
use tokio::sync::{Mutex, RwLockReadGuard, RwLockWriteGuard};

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

  spirc: Option<Spirc>,

  player: Option<Player>,

  /// Whether the session has been disconnected
  /// If this is true then this instance should no longer be used and dropped
  disconnected: bool,
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
    let session_manager = data
      .get::<SessionManager>()
      .expect("to contain a value")
      .clone();

    // Join the voice channel
    let songbird = songbird::get(ctx).await.expect("to be present").clone();

    let (call, result) = songbird.join(guild_id, channel_id).await;

    if let Err(why) = result {
      error!("Error joining voice channel: {:?}", why);
      return Err(SessionCreateError::JoinError(channel_id, guild_id));
    }

    let inner = InnerSpoticordSession {
      owner: Some(owner_id),
      guild_id,
      channel_id,
      text_channel_id,
      http: ctx.http.clone(),
      session_manager: session_manager.clone(),
      call: call.clone(),
      track: None,
      playback_info: None,
      disconnect_handle: None,
      spirc: None,
      player: None,
      disconnected: false,
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
    let session_manager = data
      .get::<SessionManager>()
      .expect("to contain a value")
      .clone();

    {
      let mut inner = self.acquire_write().await;
      inner.owner = Some(owner_id);
    }

    {
      let guild_id = self.acquire_read().await.guild_id;
      session_manager.set_owner(owner_id, guild_id).await;
    }

    // Create the player
    self.create_player(ctx).await?;

    Ok(())
  }

  /// Advance to the next track
  pub async fn next(&mut self) {
    if let Some(ref spirc) = self.acquire_read().await.spirc {
      spirc.next();
    }
  }

  /// Rewind to the previous track
  pub async fn previous(&mut self) {
    if let Some(ref spirc) = self.acquire_read().await.spirc {
      spirc.prev();
    }
  }

  /// Pause the current track
  pub async fn pause(&mut self) {
    if let Some(ref spirc) = self.acquire_read().await.spirc {
      spirc.pause();
    }
  }

  /// Resume the current track
  pub async fn resume(&mut self) {
    if let Some(ref spirc) = self.acquire_read().await.spirc {
      spirc.play();
    }
  }

  async fn create_player(&mut self, ctx: &Context) -> Result<(), SessionCreateError> {
    let owner_id = match self.owner().await {
      Some(owner_id) => owner_id,
      None => return Err(SessionCreateError::NoOwner),
    };

    let data = ctx.data.read().await;
    let database = data.get::<Database>().expect("to contain a value");

    let token = match database.get_access_token(owner_id.to_string()).await {
      Ok(token) => token,
      Err(why) => {
        if let DatabaseError::InvalidStatusCode(code) = why {
          if code == 404 {
            return Err(SessionCreateError::NoSpotify);
          } else if code == 400 {
            return Err(SessionCreateError::SpotifyExpired);
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

    // Create player
    let mut player = Player::create();

    // Create track (paused, fixes audio glitches)
    let (mut track, track_handle) = create_player(Input::new(
      true,
      Reader::Extension(Box::new(player.get_stream())),
      Codec::Pcm,
      Container::Raw,
      None,
    ));
    track.pause();

    let call = self.call().await;
    let mut call = call.lock().await;

    // Set call audio to track
    call.play_only(track);

    let (spirc, (mut player_rx, mut sink_rx)) = match player.start(&token, &user.device_name).await
    {
      Ok(v) => v,
      Err(why) => {
        error!("Failed to start the player: {:?}", why);

        return Err(SessionCreateError::PlayerStartError);
      }
    };

    // Handle events
    tokio::spawn({
      let track = track_handle.clone();
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

          // Check if the session has been disconnected
          let disconnected = {
            let inner = inner.read().await;
            inner.disconnected
          };
          if disconnected {
            break;
          }

          tokio::select! {
            event = player_rx.recv() => {
              let Some(event) = event else { break; };

              match event {
                PlayerEvent::Playing {
                  play_request_id: _,
                  track_id,
                  position_ms,
                  duration_ms,
                } => {
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

                PlayerEvent::Paused {
                  play_request_id: _,
                  track_id,
                  position_ms,
                  duration_ms,
                } => {
                  instance.start_disconnect_timer().await;

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

                PlayerEvent::Changed {
                  old_track_id: _,
                  new_track_id,
                } => {
                  let instance = instance.clone();
                  let ctx = ctx.clone();

                  // Fetch track info
                  // This is done in a separate task to avoid blocking the IPC handler
                  tokio::spawn(async move {
                    if let Err(why) = instance.update_track(&ctx, &owner_id, new_track_id).await {
                      error!("Failed to update track: {:?}", why);

                      instance.player_stopped().await;
                    }
                  });
                }

                PlayerEvent::Stopped {
                  play_request_id: _,
                  track_id: _,
                } => {
                  check_result(track.pause());

                  {
                    let mut inner = inner.write().await;
                    inner.playback_info.take();
                  }

                  instance.start_disconnect_timer().await;
                }

                _ => {}
              };
            }

            event = sink_rx.recv() => {
              let Some(event) = event else { break; };

              let check_result = |result| {
                if let Err(why) = result {
                  error!("Failed to issue track command: {:?}", why);
                }
              };


              match event {
                SinkEvent::Start => {
                  check_result(track.play());
                }

                SinkEvent::Stop => {
                  // EXPERIMENT: It may be beneficial to *NOT* pause songbird here
                  // We already have a fallback if no audio is present in the buffer (write all zeroes aka silence)
                  // So commenting this out may help prevent a substantial portion of jitter
                  // This comes at a cost of more bandwidth, though opus should compress it down to almost nothing

                  // check_result(track.pause());
                }
              }
            }
          };
        }

        // Clean up session
        if !inner.read().await.disconnected {
          instance.player_stopped().await;
        }
      }
    });

    // Update inner client and track
    let mut inner = self.acquire_write().await;
    inner.track = Some(track_handle);
    inner.spirc = Some(spirc);
    inner.player = Some(player);

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
        pbi.spotify_id.is_none() || pbi.spotify_id != Some(spotify_id)
      } else {
        false
      }
    };

    if !should_update {
      return Ok(());
    }

    let data = ctx.data.read().await;
    let database = data.get::<Database>().expect("to contain a value");

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
    let mut inner = self.acquire_write().await;

    if let Some(pbi) = inner.playback_info.as_mut() {
      pbi.update_track_episode(spotify_id, track, episode);
    }

    Ok(())
  }

  /// Called when the player must stop, but not leave the call
  async fn player_stopped(&self) {
    let mut inner = self.acquire_write().await;

    if let Some(spirc) = inner.spirc.take() {
      spirc.shutdown();
    }

    if let Some(track) = inner.track.take() {
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
      let mut inner = self.acquire_write().await;
      inner.disconnect_no_abort().await;
    }

    self.stop_disconnect_timer().await;
  }

  // Update playback info (duration, position, playing state)
  async fn update_playback(&self, duration_ms: u32, position_ms: u32, playing: bool) -> bool {
    let is_none = {
      let pbi = self.playback_info().await;

      pbi.is_none()
    };

    {
      let mut inner = self.acquire_write().await;

      if is_none {
        inner.playback_info = Some(PlaybackInfo::new(duration_ms, position_ms, playing));
      } else {
        // Update position, duration and playback state
        inner
          .playback_info
          .as_mut()
          .expect("to contain a value")
          .update_pos_dur(position_ms, duration_ms, playing);
      };
    };

    if playing {
      self.stop_disconnect_timer().await;
    }

    is_none
  }

  /// Start the disconnect timer, which will disconnect the bot from the voice channel after a
  /// certain amount of time
  async fn start_disconnect_timer(&self) {
    self.stop_disconnect_timer().await;

    let arc_handle = self.0.clone();
    let mut inner = self.acquire_write().await;

    // Check if we are already disconnected
    if inner.disconnected {
      return;
    }

    inner.disconnect_handle = Some(tokio::spawn({
      let inner = arc_handle.clone();
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

  /// Stop the disconnect timer (if one is running)
  async fn stop_disconnect_timer(&self) {
    let mut inner = self.acquire_write().await;
    if let Some(handle) = inner.disconnect_handle.take() {
      handle.abort();
    }
  }

  /// Disconnect from the VC and send a message to the text channel
  pub async fn disconnect_with_message(&self, content: &str) {
    {
      let mut inner = self.acquire_write().await;

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
    self.stop_disconnect_timer().await;
  }

  /* Inner getters */

  /// Get the owner
  pub async fn owner(&self) -> Option<UserId> {
    self.acquire_read().await.owner
  }

  /// Get the session manager
  pub async fn session_manager(&self) -> SessionManager {
    self.acquire_read().await.session_manager.clone()
  }

  /// Get the guild id
  pub async fn guild_id(&self) -> GuildId {
    self.acquire_read().await.guild_id
  }

  /// Get the channel id
  pub async fn channel_id(&self) -> ChannelId {
    self.acquire_read().await.channel_id
  }

  /// Get the channel id
  #[allow(dead_code)]
  pub async fn text_channel_id(&self) -> ChannelId {
    self.acquire_read().await.text_channel_id
  }

  /// Get the playback info
  pub async fn playback_info(&self) -> Option<PlaybackInfo> {
    self.acquire_read().await.playback_info.clone()
  }

  pub async fn call(&self) -> Arc<Mutex<Call>> {
    self.acquire_read().await.call.clone()
  }

  #[allow(dead_code)]
  pub async fn http(&self) -> Arc<Http> {
    self.acquire_read().await.http.clone()
  }

  async fn acquire_read(&self) -> ReadLock {
    ReadLock(self.0.read().await)
  }

  async fn acquire_write(&self) -> WriteLock {
    WriteLock(self.0.write().await)
  }
}

struct ReadLock<'a>(RwLockReadGuard<'a, InnerSpoticordSession>);

impl<'a> Deref for ReadLock<'a> {
  type Target = RwLockReadGuard<'a, InnerSpoticordSession>;

  #[inline]
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<'a> DerefMut for ReadLock<'a> {
  #[inline]
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

struct WriteLock<'a>(RwLockWriteGuard<'a, InnerSpoticordSession>);

impl<'a> Deref for WriteLock<'a> {
  type Target = RwLockWriteGuard<'a, InnerSpoticordSession>;

  #[inline]
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<'a> DerefMut for WriteLock<'a> {
  #[inline]
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

impl InnerSpoticordSession {
  /// Internal version of disconnect, which does not abort the disconnect timer
  async fn disconnect_no_abort(&mut self) {
    // Flush stream so that it is not permanently blocking the thread
    if let Some(player) = self.player.take() {
      player.get_stream().flush().ok();
    }

    self.disconnected = true;
    self
      .session_manager
      .remove_session(self.guild_id, self.owner)
      .await;

    let mut call = self.call.lock().await;

    if let Some(spirc) = self.spirc.take() {
      spirc.shutdown();
    }

    if let Some(track) = self.track.take() {
      if let Err(why) = track.stop() {
        error!("Failed to stop track: {:?}", why);
      }
    }

    call.remove_all_global_events();

    if let Err(why) = call.leave().await {
      error!("Failed to leave voice channel: {:?}", why);
    }
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
