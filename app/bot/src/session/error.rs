use thiserror::Error;

#[derive(Error, Debug)]
pub enum SessionError {
    /// The user executed an action inside of a channel that is not supported
    #[error("The specified channel is invalid for this operation")]
    InvalidChannel,

    /// Cannot perform this action on an active session
    #[error("Cannot perform this action on an active session")]
    AlreadyActive,

    #[error("An error occured in the player: {0}")]
    PlayerError(String),

    #[error(transparent)]
    Serenity(#[from] poise::serenity_prelude::Error),

    #[error(transparent)]
    Spotify(#[from] spoticord_spotify::Error),

    #[error(transparent)]
    Database(#[from] spoticord_database::error::DatabaseError),

    #[error(transparent)]
    JoinError(#[from] songbird::error::JoinError),

    #[error(transparent)]
    Ipc(#[from] spoticord_ipc::IpcError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = ::core::result::Result<T, SessionError>;
