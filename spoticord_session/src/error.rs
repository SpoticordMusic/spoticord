use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    /// The user executed an action inside of a channel that is not supported
    #[error("The specified channel is invalid for this operation")]
    InvalidChannel,

    /// Generic authentication failure
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Cannot perform this action on an active session
    #[error("Cannot perform this action on an active session")]
    AlreadyActive,

    #[error(transparent)]
    Serenity(#[from] serenity::Error),

    #[error(transparent)]
    Database(#[from] spoticord_database::error::DatabaseError),

    #[error(transparent)]
    JoinError(#[from] songbird::error::JoinError),

    #[error(transparent)]
    Librespot(#[from] librespot::core::Error),
}

pub type Result<T> = ::core::result::Result<T, Error>;
