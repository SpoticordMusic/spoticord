use librespot::discovery::Credentials;
use librespot::protocol::authentication::AuthenticationType;

pub trait CredentialsExt {
  fn with_token(username: impl Into<String>, token: impl Into<String>) -> Credentials;
}

impl CredentialsExt for Credentials {
  // Enable the use of a token to connect to Spotify
  // Wouldn't want to ask users for their password would we?
  fn with_token(username: impl Into<String>, token: impl Into<String>) -> Credentials {
    Credentials {
      username: username.into(),
      auth_type: AuthenticationType::AUTHENTICATION_SPOTIFY_TOKEN,
      auth_data: token.into().into_bytes(),
    }
  }
}
