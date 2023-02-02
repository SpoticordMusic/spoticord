use std::{collections::HashMap, sync::Arc};

use serenity::{
  model::prelude::{ChannelId, GuildId, UserId},
  prelude::{Context, TypeMapKey},
};
use thiserror::Error;

use super::SpoticordSession;

#[derive(Debug, Error)]
pub enum SessionCreateError {
  #[error("This session has no owner assigned")]
  NoOwner,

  #[error("The user has not linked their Spotify account")]
  NoSpotify,

  #[error("The application no longer has access to the user's Spotify account")]
  SpotifyExpired,

  #[error("An error has occured while communicating with the database")]
  DatabaseError,

  #[error("Failed to join voice channel {0} ({1})")]
  JoinError(ChannelId, GuildId),

  #[error("Failed to start player process")]
  ForkError,
}

#[derive(Clone)]
pub struct SessionManager(Arc<tokio::sync::RwLock<InnerSessionManager>>);

impl TypeMapKey for SessionManager {
  type Value = SessionManager;
}

pub struct InnerSessionManager {
  sessions: HashMap<GuildId, SpoticordSession>,
  owner_map: HashMap<UserId, GuildId>,
}

impl InnerSessionManager {
  pub fn new() -> Self {
    Self {
      sessions: HashMap::new(),
      owner_map: HashMap::new(),
    }
  }

  /// Creates a new session for the given user in the given guild.
  pub async fn create_session(
    &mut self,
    session: SpoticordSession,
    guild_id: GuildId,
    owner_id: UserId,
  ) -> Result<(), SessionCreateError> {
    self.sessions.insert(guild_id, session);
    self.owner_map.insert(owner_id, guild_id);

    Ok(())
  }

  /// Remove a session
  pub async fn remove_session(&mut self, guild_id: GuildId, owner: Option<UserId>) {
    // Remove the owner from the owner map (if it exists)
    if let Some(owner) = owner {
      self.owner_map.remove(&owner);
    }

    self.sessions.remove(&guild_id);
  }

  /// Remove owner from owner map.
  /// Used whenever a user stops playing music without leaving the bot.
  pub fn remove_owner(&mut self, owner_id: UserId) {
    self.owner_map.remove(&owner_id);
  }

  /// Set the owner of a session
  /// Used when a user joins a session that is already active
  pub fn set_owner(&mut self, owner_id: UserId, guild_id: GuildId) {
    self.owner_map.insert(owner_id, guild_id);
  }

  /// Get a session by its guild ID
  pub fn get_session(&self, guild_id: GuildId) -> Option<SpoticordSession> {
    self.sessions.get(&guild_id).cloned()
  }

  /// Find a Spoticord session by their current owner's ID
  pub fn find(&self, owner_id: UserId) -> Option<SpoticordSession> {
    let guild_id = self.owner_map.get(&owner_id)?;

    self.sessions.get(guild_id).cloned()
  }

  /// Get the amount of sessions
  pub fn get_session_count(&self) -> usize {
    self.sessions.len()
  }

  /// Get the amount of sessions with an owner
  pub async fn get_active_session_count(&self) -> usize {
    let mut count: usize = 0;

    for session in self.sessions.values() {
      let session = session.0.read().await;

      if session.owner.is_some() {
        count += 1;
      }
    }

    count
  }
}

impl SessionManager {
  pub fn new() -> Self {
    Self(Arc::new(tokio::sync::RwLock::new(
      InnerSessionManager::new(),
    )))
  }

  /// Creates a new session for the given user in the given guild.
  pub async fn create_session(
    &self,
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    text_channel_id: ChannelId,
    owner_id: UserId,
  ) -> Result<(), SessionCreateError> {
    // Create session first to make sure locks are kept for as little time as possible
    let session =
      SpoticordSession::new(ctx, guild_id, channel_id, text_channel_id, owner_id).await?;

    self
      .0
      .write()
      .await
      .create_session(session, guild_id, owner_id)
      .await
  }

  /// Remove a session
  pub async fn remove_session(&self, guild_id: GuildId, owner: Option<UserId>) {
    self.0.write().await.remove_session(guild_id, owner).await;
  }

  /// Remove owner from owner map.
  /// Used whenever a user stops playing music without leaving the bot.
  pub async fn remove_owner(&self, owner_id: UserId) {
    self.0.write().await.remove_owner(owner_id);
  }

  /// Set the owner of a session
  /// Used when a user joins a session that is already active
  pub async fn set_owner(&self, owner_id: UserId, guild_id: GuildId) {
    self.0.write().await.set_owner(owner_id, guild_id);
  }

  /// Get a session by its guild ID
  pub async fn get_session(&self, guild_id: GuildId) -> Option<SpoticordSession> {
    self.0.read().await.get_session(guild_id)
  }

  /// Find a Spoticord session by their current owner's ID
  pub async fn find(&self, owner_id: UserId) -> Option<SpoticordSession> {
    self.0.read().await.find(owner_id)
  }

  /// Get the amount of sessions
  pub async fn get_session_count(&self) -> usize {
    self.0.read().await.get_session_count()
  }

  /// Get the amount of sessions with an owner
  pub async fn get_active_session_count(&self) -> usize {
    self.0.read().await.get_active_session_count().await
  }
}
