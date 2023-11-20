use std::{collections::HashMap, sync::Arc};

use poise::serenity_prelude::model::prelude::{ChannelId, GuildId, UserId};
use songbird::error::JoinError;
use thiserror::Error;

use crate::bot::Context;

use super::Session;

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

    #[error("Failed to join voice channel")]
    JoinError(JoinError),

    #[error("Failed to start the player")]
    PlayerStartError,
}

#[derive(Clone)]
pub struct SessionManager(Arc<tokio::sync::RwLock<InnerSessionManager>>);

pub struct InnerSessionManager {
    sessions: HashMap<GuildId, Session>,
    owner_map: HashMap<UserId, GuildId>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self(Arc::new(tokio::sync::RwLock::new(InnerSessionManager {
            owner_map: HashMap::new(),
            sessions: HashMap::new(),
        })))
    }

    /// Creates a new session for the given user in the given guild.
    pub async fn create_session(
        &self,
        ctx: &Context<'_>,
        guild_id: GuildId,
        channel_id: ChannelId,
        text_channel_id: ChannelId,
        owner_id: UserId,
    ) -> Result<(), SessionCreateError> {
        // Create session first to make sure locks are kept for as little time as possible
        let session = Session::new(ctx, guild_id, channel_id, text_channel_id, owner_id).await?;

        let mut manager = self.0.write().await;

        manager.sessions.insert(guild_id, session);
        manager.owner_map.insert(owner_id, guild_id);

        Ok(())
    }

    /// Remove a session
    pub async fn remove_session(&self, guild_id: &GuildId, owner: Option<&UserId>) {
        let mut manager = self.0.write().await;

        // Remove the owner from the owner map (if it exists)
        if let Some(owner) = owner {
            manager.owner_map.remove(owner);
        }

        manager.sessions.remove(guild_id);
    }

    /// Remove owner from owner map.
    /// Used whenever a user stops playing music without leaving the bot.
    pub async fn remove_owner(&self, owner_id: &UserId) {
        self.0.write().await.owner_map.remove(owner_id);
    }

    /// Set the owner of a session
    /// Used when a user joins a session that is already active
    pub async fn set_owner(&self, owner_id: UserId, guild_id: GuildId) {
        self.0.write().await.owner_map.insert(owner_id, guild_id);
    }

    /// Get a session by its guild ID
    pub async fn get_session(&self, guild_id: &GuildId) -> Option<Session> {
        self.0.read().await.sessions.get(guild_id).cloned()
    }

    /// Find a Spoticord session by their current owner's ID
    pub async fn find(&self, owner_id: UserId) -> Option<Session> {
        let manager = self.0.read().await;
        let guild_id = manager.owner_map.get(&owner_id)?;

        manager.sessions.get(guild_id).cloned()
    }

    /// Get the amount of sessions
    #[allow(dead_code)]
    pub async fn get_session_count(&self) -> usize {
        self.0.read().await.sessions.len()
    }

    /// Get the amount of sessions with an owner
    #[allow(dead_code)]
    pub async fn get_active_session_count(&self) -> usize {
        let manager = self.0.read().await;

        let mut count: usize = 0;
        for session in manager.sessions.values() {
            let session = session.0.read().await;

            if session.owner.is_some() {
                count += 1;
            }
        }

        count
    }

    /// Tell all sessions to instantly shut down
    pub async fn shutdown(&self) {
        let sessions = self
            .0
            .read()
            .await
            .sessions
            .values()
            .cloned()
            .collect::<Vec<_>>();

        for session in sessions {
            session.disconnect().await;
        }
    }
}
