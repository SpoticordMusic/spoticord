#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Database(#[from] spoticord_database::error::DatabaseError),

    #[error(transparent)]
    Spotify(#[from] rspotify::ClientError),

    #[error("Failed to refresh spotify token")]
    RefreshTokenFailure,
}
