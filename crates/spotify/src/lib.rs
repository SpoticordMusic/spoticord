mod error;

use chrono::Duration;
use rspotify::{prelude::BaseClient, AuthCodeSpotify, Config, Credentials, OAuth, Token};
use spoticord_database::Database;

pub use error::Error;

pub struct SpotifyService {
    client_id: String,
    client_secret: String,

    database: Database,
}

impl SpotifyService {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        database: Database,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),

            database,
        }
    }

    pub async fn get_access_token(&self, user_id: &str) -> Result<String, Error> {
        let mut account = self.database.get_account(user_id).await?;

        if account.expired_offset(Duration::minutes(1)) {
            let spotify = self.get_spotify(Token {
                refresh_token: Some(account.refresh_token),
                ..Default::default()
            });

            let token = match spotify.refetch_token().await {
                Ok(Some(token)) => token,
                _ => {
                    _ = self.database.delete_account(user_id).await;

                    return Err(Error::RefreshTokenFailure);
                }
            };

            let expires_at = token
                .expires_at
                .expect("token expires_at is none, we broke time")
                .naive_utc();

            account = self
                .database
                .update_account_token(
                    user_id,
                    &token.access_token,
                    token.refresh_token.as_deref(),
                    expires_at,
                )
                .await?;
        }

        Ok(account.access_token)
    }

    fn get_spotify(&self, token: Token) -> AuthCodeSpotify {
        AuthCodeSpotify::from_token_with_config(
            token,
            Credentials {
                id: self.client_id.clone(),
                secret: Some(self.client_secret.clone()),
            },
            OAuth::default(),
            Config::default(),
        )
    }
}
