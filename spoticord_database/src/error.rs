use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error(transparent)]
    Diesel(diesel::result::Error),

    #[error(transparent)]
    PoolBuild(#[from] diesel_async::pooled_connection::deadpool::BuildError),

    #[error(transparent)]
    Pool(#[from] diesel_async::pooled_connection::deadpool::PoolError),

    #[error("Failed to refresh token")]
    RefreshTokenFailure,

    #[error("The requested record was not found")]
    NotFound,
}

impl From<diesel::result::Error> for DatabaseError {
    fn from(value: diesel::result::Error) -> Self {
        match value {
            diesel::result::Error::NotFound => Self::NotFound,
            other => Self::Diesel(other),
        }
    }
}

pub type Result<T> = ::core::result::Result<T, DatabaseError>;

pub trait DatabaseResultExt<T> {
    fn optional(self) -> Result<Option<T>>;
}

impl<T> DatabaseResultExt<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Err(DatabaseError::NotFound) => Ok(None),
            other => other.map(Some),
        }
    }
}
