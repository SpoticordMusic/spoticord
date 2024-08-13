pub mod error;

mod migrations;
mod models;
mod schema;

use std::sync::Arc;

use chrono::{Duration, Utc};
use diesel::prelude::*;
use diesel_async::{
    pooled_connection::{deadpool::Pool, AsyncDieselConnectionManager},
    AsyncPgConnection, RunQueryDsl,
};
use error::*;
use models::{Account, LinkRequest, User};
use rand::{distributions::Alphanumeric, Rng};
use rspotify::{clients::BaseClient, Token};

#[derive(Clone)]
pub struct Database(Arc<Pool<AsyncPgConnection>>);

impl Database {
    pub async fn connect() -> Result<Self> {
        let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(
            spoticord_config::database_url(),
        );
        let pool = Pool::builder(config).build()?;

        let mut conn = pool.get().await?;
        migrations::run_migrations(&mut conn).await?;

        Ok(Self(Arc::new(pool)))
    }

    // User operations

    pub async fn get_user(&self, user_id: impl AsRef<str>) -> Result<User> {
        use schema::user::dsl::*;

        let mut connection = self.0.get().await?;
        let result = user
            .filter(id.eq(user_id.as_ref()))
            .select(User::as_select())
            .first(&mut connection)
            .await?;

        Ok(result)
    }

    pub async fn create_user(&self, user_id: impl AsRef<str>) -> Result<User> {
        use schema::user::dsl::*;

        let mut connection = self.0.get().await?;
        let result = diesel::insert_into(user)
            .values(id.eq(user_id.as_ref()))
            .returning(User::as_returning())
            .get_result(&mut connection)
            .await?;

        Ok(result)
    }

    pub async fn delete_user(&self, user_id: impl AsRef<str>) -> Result<usize> {
        use schema::user::dsl::*;

        let mut connection = self.0.get().await?;
        let affected = diesel::delete(user)
            .filter(id.eq(user_id.as_ref()))
            .execute(&mut connection)
            .await?;

        Ok(affected)
    }

    pub async fn get_or_create_user(&self, user_id: impl AsRef<str>) -> Result<User> {
        match self.get_user(&user_id).await {
            Err(DatabaseError::NotFound) => self.create_user(user_id).await,
            result => result,
        }
    }

    pub async fn update_device_name(
        &self,
        user_id: impl AsRef<str>,
        _device_name: impl AsRef<str>,
    ) -> Result<()> {
        use schema::user::dsl::*;

        let mut connection = self.0.get().await?;
        diesel::update(user)
            .filter(id.eq(user_id.as_ref()))
            .set(device_name.eq(_device_name.as_ref()))
            .execute(&mut connection)
            .await?;

        Ok(())
    }

    // Account operations

    pub async fn get_account(&self, _user_id: impl AsRef<str>) -> Result<Account> {
        use schema::account::dsl::*;

        let mut connection = self.0.get().await?;
        let result = account
            .select(Account::as_select())
            .filter(user_id.eq(_user_id.as_ref()))
            .first(&mut connection)
            .await?;

        Ok(result)
    }

    pub async fn delete_account(&self, _user_id: impl AsRef<str>) -> Result<usize> {
        use schema::account::dsl::*;

        let mut connection = self.0.get().await?;
        let affected = diesel::delete(account)
            .filter(user_id.eq(_user_id.as_ref()))
            .execute(&mut connection)
            .await?;

        Ok(affected)
    }

    pub async fn update_session_token(
        &self,
        _user_id: impl AsRef<str>,
        _session_token: impl AsRef<str>,
    ) -> Result<()> {
        use schema::account::dsl::*;

        let mut connection = self.0.get().await?;
        diesel::update(account)
            .filter(user_id.eq(_user_id.as_ref()))
            .set(session_token.eq(_session_token.as_ref()))
            .execute(&mut connection)
            .await?;

        Ok(())
    }

    // Request operations

    pub async fn get_request(&self, _user_id: impl AsRef<str>) -> Result<LinkRequest> {
        use schema::link_request::dsl::*;

        let mut connection = self.0.get().await?;
        let result = link_request
            .select(LinkRequest::as_select())
            .filter(user_id.eq(_user_id.as_ref()))
            .first(&mut connection)
            .await?;

        Ok(result)
    }

    /// Create a new link request that expires after an hour
    pub async fn create_request(&self, _user_id: impl AsRef<str>) -> Result<LinkRequest> {
        use schema::link_request::dsl::*;

        let mut connection = self.0.get().await?;
        let _token: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(64)
            .map(char::from)
            .collect();
        let _expires = (Utc::now() + Duration::hours(1)).naive_utc();

        let request = diesel::insert_into(link_request)
            .values((
                user_id.eq(_user_id.as_ref()),
                token.eq(&_token),
                expires.eq(_expires),
            ))
            .on_conflict(user_id)
            .do_update()
            .set((token.eq(&_token), expires.eq(_expires)))
            .returning(LinkRequest::as_returning())
            .get_result(&mut connection)
            .await?;

        Ok(request)
    }

    // Special operations

    /// Retrieve a user's Spotify access token. This token, if expired, will automatically be refreshed
    /// using the refresh token stored in the database. If this succeeds, the access token will be updated.
    pub async fn get_access_token(&self, _user_id: impl AsRef<str>) -> Result<String> {
        use schema::account::dsl::*;

        let mut connection = self.0.get().await?;
        let mut result: Account = account
            .filter(user_id.eq(_user_id.as_ref()))
            .select(Account::as_select())
            .first(&mut connection)
            .await?;

        // If the token has expired, refresh it automatically
        if result.expired_offset(Duration::minutes(1)) {
            let spotify = spoticord_config::get_spotify(Token {
                refresh_token: Some(result.refresh_token),
                ..Default::default()
            });

            let token = match spotify.refetch_token().await {
                Ok(Some(token)) => token,
                _ => {
                    self.delete_account(_user_id.as_ref()).await.ok();

                    return Err(DatabaseError::RefreshTokenFailure);
                }
            };

            result = diesel::update(account)
                .filter(user_id.eq(_user_id.as_ref()))
                .set((
                    access_token.eq(&token.access_token),
                    refresh_token.eq(token.refresh_token.as_deref().unwrap_or("")),
                    expires.eq(&token
                        .expires_at
                        .expect("token expires_at is none, we broke time")
                        .naive_utc()),
                ))
                .returning(Account::as_returning())
                .get_result(&mut connection)
                .await?;
        }

        Ok(result.access_token)
    }
}
