use super::{Session, SessionHandle};
use crate::error::Result;
use serenity::all::{ChannelId, GuildId, UserId};
use songbird::Songbird;
use spoticord_database::Database;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct SessionManager {
    songbird: Arc<Songbird>,
    database: Database,

    sessions: Arc<Mutex<HashMap<GuildId, SessionHandle>>>,
    owners: Arc<Mutex<HashMap<UserId, SessionHandle>>>,
}

pub enum SessionQuery {
    Guild(GuildId),
    Owner(UserId),
}

impl SessionManager {
    pub fn new(songbird: Arc<Songbird>, database: Database) -> Self {
        Self {
            songbird,
            database,

            sessions: Arc::new(Mutex::new(HashMap::new())),
            owners: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create_session(
        &self,
        context: &serenity::all::Context,
        guild_id: GuildId,
        voice_channel_id: ChannelId,
        text_channel_id: ChannelId,
        owner: UserId,
    ) -> Result<SessionHandle> {
        let handle = Session::create(
            self.clone(),
            context,
            guild_id,
            voice_channel_id,
            text_channel_id,
            owner,
        )
        .await?;

        self.sessions
            .lock()
            .expect("mutex poisoned")
            .insert(guild_id, handle.clone());
        self.owners
            .lock()
            .expect("mutex poisoned")
            .insert(owner, handle.clone());

        Ok(handle)
    }

    pub fn get_session(&self, query: SessionQuery) -> Option<SessionHandle> {
        match query {
            SessionQuery::Guild(guild) => self
                .sessions
                .lock()
                .expect("mutex poisoned")
                .get(&guild)
                .cloned(),
            SessionQuery::Owner(owner) => self
                .owners
                .lock()
                .expect("mutex poisoned")
                .get(&owner)
                .cloned(),
        }
    }

    pub fn remove_session(&self, query: SessionQuery) {
        match query {
            SessionQuery::Guild(guild) => {
                self.sessions.lock().expect("mutex poisoned").remove(&guild)
            }
            SessionQuery::Owner(owner) => {
                self.owners.lock().expect("mutex poisoned").remove(&owner)
            }
        };
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
            session.disconnect().await;
        }

        self.owners.lock().expect("mutex poisoned").clear();
        self.sessions.lock().expect("mutex poisoned").clear();
    }

    pub fn songbird(&self) -> Arc<Songbird> {
        self.songbird.clone()
    }

    pub fn database(&self) -> Database {
        self.database.clone()
    }
}
