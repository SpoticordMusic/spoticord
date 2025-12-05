use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use poise::serenity_prelude::{ChannelId, GuildId, UserId};
use songbird::Songbird;
use spoticord_database::Database;
use spoticord_spotify::SpotifyService;

use crate::session::{Session, SessionHandle};

pub struct SessionManager {
    songbird: Arc<Songbird>,
    spotify: SpotifyService,
    database: Database,

    sessions: Mutex<HashMap<GuildId, SessionHandle>>,
    owners: Mutex<HashMap<UserId, Vec<GuildId>>>,
}

impl SessionManager {
    pub fn new(songbird: Arc<Songbird>, spotify: SpotifyService, database: Database) -> Self {
        Self {
            songbird,
            spotify,
            database,

            sessions: Mutex::new(HashMap::new()),
            owners: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_session(&self, guild_id: GuildId) -> Option<SessionHandle> {
        self.sessions
            .lock()
            .expect("mutex poisoned")
            .get(&guild_id)
            .cloned()
    }

    /// Get all guilds that a specific user is owning a session in
    pub fn get_owner_guilds(&self, owner_id: UserId) -> Vec<GuildId> {
        self.owners
            .lock()
            .expect("mutex poisoned")
            .get(&owner_id)
            .cloned()
            .unwrap_or_else(Vec::new)
    }

    /// Remove a user's ownership of a guild session
    pub fn remove_owner(&self, guild_id: GuildId, owner_id: UserId) {
        let mut owners = self.owners.lock().expect("mutex poisoned");

        if let Some(guilds) = owners.get_mut(&owner_id) {
            guilds.retain(|&id| id != guild_id);
        }

        // Remove owners with 0 guilds
        owners.retain(|_, guilds| !guilds.is_empty());
    }

    /// Grant a user ownership to a guild session
    pub fn assign_owner(&self, guild_id: GuildId, owner_id: UserId) {
        self.owners
            .lock()
            .expect("mutex poisoned")
            .entry(owner_id)
            .or_default()
            .push(guild_id);
    }

    /// Remove a session specified by a guild id
    pub fn remove_session(&self, guild_id: GuildId) {
        self.sessions
            .lock()
            .expect("mutex poisoned")
            .remove(&guild_id);

        // Remove user ownership of guild (if there are any)
        let mut owners = self.owners.lock().expect("mutex poisoned");

        for (_, guilds) in owners.iter_mut() {
            guilds.retain(|&id| id != guild_id);
        }

        // Remove owners with 0 guilds
        owners.retain(|_, guilds| !guilds.is_empty());
    }

    pub fn get_all_sessions(&self) -> Vec<SessionHandle> {
        self.sessions
            .lock()
            .expect("mutex poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Disconnects all active sessions and clears out all handles.
    ///
    /// The session manager can still create new sessions after all sessions have been shut down.
    /// Sessions might still be created during shutdown.
    pub async fn shutdown_all(&self) {
        let sessions = self.get_all_sessions();

        for session in sessions {
            session.shutdown().await;
        }

        self.sessions.lock().expect("mutex poisoned").clear();
        self.owners.lock().expect("mutex poisoned").clear();
    }

    pub fn database(&self) -> Database {
        self.database.clone()
    }

    pub fn spotify(&self) -> &SpotifyService {
        &self.spotify
    }

    pub fn songbird(&self) -> Arc<Songbird> {
        self.songbird.clone()
    }
}

// Need direct access to the Arc smart pointer, so we define this function outside of the SessionManager impl
pub async fn create_session(
    context: poise::serenity_prelude::Context,
    manager: Arc<SessionManager>,
    guild_id: GuildId,
    voice_channel_id: ChannelId,
    text_channel_id: ChannelId,
    owner_id: UserId,
) -> super::error::Result<SessionHandle> {
    let handle = Session::create(
        context,
        guild_id,
        voice_channel_id,
        text_channel_id,
        owner_id,
        manager.clone(),
    )
    .await?;

    manager
        .sessions
        .lock()
        .expect("mutex poisoned")
        .insert(guild_id, handle.clone());
    manager
        .owners
        .lock()
        .expect("mutex poisoned")
        .entry(owner_id)
        .or_default()
        .push(guild_id);

    Ok(handle)
}
