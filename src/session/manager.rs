use std::{collections::HashMap, sync::Arc};

use serenity::{
  model::prelude::{ChannelId, GuildId, UserId},
  prelude::{Context, TypeMapKey},
};
use thiserror::Error;

use super::SpoticordSession;

#[derive(Debug, Error)]
pub enum SessionCreateError {
  #[error("The user has not linked their Spotify account")]
  NoSpotifyError,

  #[error("An error has occured while communicating with the database")]
  DatabaseError,

  #[error("Failed to join voice channel {0} ({1})")]
  JoinError(ChannelId, GuildId),

  #[error("Failed to start player process")]
  ForkError,
}

#[derive(Clone)]
pub struct SessionManager {
  sessions: Arc<tokio::sync::RwLock<HashMap<GuildId, Arc<SpoticordSession>>>>,
  owner_map: Arc<tokio::sync::RwLock<HashMap<UserId, GuildId>>>,
}

impl TypeMapKey for SessionManager {
  type Value = SessionManager;
}

impl SessionManager {
  pub fn new() -> SessionManager {
    SessionManager {
      sessions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
      owner_map: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    }
  }

  /// Creates a new session for the given user in the given guild.
  pub async fn create_session(
    &mut self,
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    text_channel_id: ChannelId,
    owner_id: UserId,
  ) -> Result<(), SessionCreateError> {
    // Create session first to make sure locks are kept for as little time as possible
    let session =
      SpoticordSession::new(ctx, guild_id, channel_id, text_channel_id, owner_id).await?;

    let mut sessions = self.sessions.write().await;
    let mut owner_map = self.owner_map.write().await;

    sessions.insert(guild_id, Arc::new(session));
    owner_map.insert(owner_id, guild_id);

    Ok(())
  }

  /// Remove a session
  pub async fn remove_session(&mut self, guild_id: GuildId) {
    let mut sessions = self.sessions.write().await;

    if let Some(session) = sessions.get(&guild_id) {
      if let Some(owner) = session.get_owner().await {
        let mut owner_map = self.owner_map.write().await;
        owner_map.remove(&owner);
      }
    }

    sessions.remove(&guild_id);
  }

  /// Remove owner from owner map.
  /// Used whenever a user stops playing music without leaving the bot.
  pub async fn remove_owner(&mut self, owner_id: UserId) {
    let mut owner_map = self.owner_map.write().await;
    owner_map.remove(&owner_id);
  }

  /// Set the owner of a session
  /// Used when a user joins a session that is already active
  pub async fn set_owner(&mut self, owner_id: UserId, guild_id: GuildId) {
    let mut owner_map = self.owner_map.write().await;
    owner_map.insert(owner_id, guild_id);
  }

  /// Get a session by its guild ID
  pub async fn get_session(&self, guild_id: GuildId) -> Option<Arc<SpoticordSession>> {
    let sessions = self.sessions.read().await;

    sessions.get(&guild_id).cloned()
  }

  /// Find a Spoticord session by their current owner's ID
  pub async fn find(&self, owner_id: UserId) -> Option<Arc<SpoticordSession>> {
    let sessions = self.sessions.read().await;
    let owner_map = self.owner_map.read().await;

    let guild_id = owner_map.get(&owner_id)?;

    sessions.get(&guild_id).cloned()
  }

  /// Get the amount of sessions with an owner
  pub async fn get_active_session_count(&self) -> usize {
    let sessions = self.sessions.read().await;

    let mut count: usize = 0;

    for session in sessions.values() {
      if session.owner.read().await.is_some() {
        count += 1;
      }
    }

    count
  }
}
