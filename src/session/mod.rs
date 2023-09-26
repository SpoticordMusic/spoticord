pub mod manager;
pub mod pbi;

use self::{
  manager::{SessionCreateError, SessionManager},
  pbi::PlaybackInfo,
};
use crate::{
  audio::stream::Stream,
  consts::DISCONNECT_TIME,
  database::{Database, DatabaseError},
  player::{Player, PlayerEvent},
  utils::embed::Status,
};
use log::*;
use reqwest::StatusCode;
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
  ops::{Deref, DerefMut},
  sync::Arc,
  time::Duration,
};
use tokio::sync::{Mutex, RwLockReadGuard, RwLockWriteGuard};

#[derive(Clone)]
pub struct SpoticordSession(Arc<RwLock<InnerSpoticordSession>>);

impl Drop for SpoticordSession {
  fn drop(&mut self) {
    log::trace!("drop SpoticordSession");
  }
}

struct InnerSpoticordSession {
  owner: Option<UserId>,
  guild_id: GuildId,
  channel_id: ChannelId,
  text_channel_id: ChannelId,

  http: Arc<Http>,

  session_manager: SessionManager,

  call: Arc<Mutex<Call>>,
  track: Option<TrackHandle>,
  player: Option<Player>,

  disconnect_handle: Option<tokio::task::JoinHandle<()>>,

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
      return Err(SessionCreateError::JoinError(why));
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
      player: None,
      disconnect_handle: None,
      disconnected: false,
    };

    let mut instance = Self(Arc::new(RwLock::new(inner)));
    if let Err(why) = instance.create_player(ctx).await {
      songbird.remove(guild_id).await.ok();

      return Err(why);
    }

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
    if let Some(ref player) = self.acquire_read().await.player {
      player.next();
    }
  }

  /// Rewind to the previous track
  pub async fn previous(&mut self) {
    if let Some(ref player) = self.acquire_read().await.player {
      player.prev();
    }
  }

  /// Pause the current track
  pub async fn pause(&mut self) {
    if let Some(ref player) = self.acquire_read().await.player {
      player.pause();
    }
  }

  /// Resume the current track
  pub async fn resume(&mut self) {
    if let Some(ref player) = self.acquire_read().await.player {
      player.play();
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
        return match why {
          DatabaseError::InvalidStatusCode(StatusCode::NOT_FOUND) => {
            Err(SessionCreateError::NoSpotify)
          }
          DatabaseError::InvalidStatusCode(StatusCode::BAD_REQUEST) => {
            Err(SessionCreateError::SpotifyExpired)
          }
          _ => Err(SessionCreateError::DatabaseError),
        };
      }
    };

    let user = match database.get_user(owner_id.to_string()).await {
      Ok(user) => user,
      Err(why) => {
        error!("Failed to get user: {:?}", why);
        return Err(SessionCreateError::DatabaseError);
      }
    };

    // Create stream
    let stream = Stream::new();

    // Create track (paused, fixes audio glitches)
    let (mut track, track_handle) = create_player(Input::new(
      true,
      Reader::Extension(Box::new(stream.clone())),
      Codec::FloatPcm,
      Container::Raw,
      None,
    ));
    track.pause();

    let call = self.call().await;
    let mut call = call.lock().await;

    // Set call audio to track
    call.play_only(track);

    let (player, mut rx) =
      match Player::create(stream, &token, &user.device_name, track_handle.clone()).await {
        Ok(v) => v,
        Err(why) => {
          error!("Failed to start the player: {:?}", why);

          return Err(SessionCreateError::PlayerStartError);
        }
      };

    tokio::spawn({
      let session = self.clone();

      async move {
        loop {
          match rx.recv().await {
            Ok(event) => match event {
              PlayerEvent::Pause => session.start_disconnect_timer().await,
              PlayerEvent::Play => session.stop_disconnect_timer().await,
              PlayerEvent::Stopped => {
                session.player_stopped().await;
                break;
              }
            },
            Err(why) => {
              error!("Communication with player abruptly ended: {why}");
              session.player_stopped().await;

              break;
            }
          }
        }
      }
    });

    // Start DC timer by default, as automatic device switching is now gone
    self.start_disconnect_timer().await;

    let mut inner = self.acquire_write().await;
    inner.track = Some(track_handle);
    inner.player = Some(player);

    Ok(())
  }

  /// Called when the player must stop, but not leave the call
  async fn player_stopped(&self) {
    let mut inner = self.acquire_write().await;

    if let Some(track) = inner.track.take() {
      if let Err(why) = track.stop() {
        error!("Failed to stop track: {:?}", why);
      }
    }

    // Clear owner
    if let Some(owner_id) = inner.owner.take() {
      inner.session_manager.remove_owner(owner_id).await;
    }

    // Disconnect from Spotify
    if let Some(player) = inner.player.take() {
      player.shutdown();
    }

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

  /// Start the disconnect timer, which will disconnect the bot from the voice channel after a
  /// certain amount of time
  async fn start_disconnect_timer(&self) {
    self.stop_disconnect_timer().await;

    let mut inner = self.acquire_write().await;

    // Check if we are already disconnected
    if inner.disconnected {
      return;
    }

    inner.disconnect_handle = Some(tokio::spawn({
      let session = self.clone();

      async move {
        let mut timer = tokio::time::interval(Duration::from_secs(DISCONNECT_TIME));

        // Ignore first (immediate) tick
        timer.tick().await;
        timer.tick().await;

        trace!("Ring ring, time to check :)");

        // Make sure this task has not been aborted, if it has this will automatically stop execution.
        tokio::task::yield_now().await;

        let is_playing = session
          .playback_info()
          .await
          .map(|pbi| pbi.is_playing)
          .unwrap_or(false);

        trace!("is_playing = {is_playing}");

        if !is_playing {
          info!("Player is not playing, disconnecting");
          session
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
    let handle = self.acquire_read().await;
    let player = handle.player.as_ref()?;

    player.pbi().await
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
    // Disconnect from Spotify
    if let Some(player) = self.player.take() {
      player.shutdown();
    }

    self.disconnected = true;
    self
      .session_manager
      .remove_session(self.guild_id, self.owner)
      .await;

    if let Some(track) = self.track.take() {
      if let Err(why) = track.stop() {
        error!("Failed to stop track: {:?}", why);
      }
    };

    let mut call = self.call.lock().await;

    if let Err(why) = call.leave().await {
      error!("Failed to leave voice channel: {:?}", why);
    }

    call.remove_all_global_events();
  }
}

#[async_trait]
impl EventHandler for SpoticordSession {
  async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
    match ctx {
      EventContext::DriverDisconnect(_) => {
        debug!("Driver disconnected, leaving voice channel");
        trace!("Arc strong count: {}", Arc::strong_count(&self.0));
        self.disconnect().await;
      }
      EventContext::ClientDisconnect(who) => {
        trace!("Client disconnected, {}", who.user_id.to_string());
        trace!("Arc strong count: {}", Arc::strong_count(&self.0));

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

impl Drop for InnerSpoticordSession {
  fn drop(&mut self) {
    log::trace!("drop InnerSpoticordSession");
  }
}
