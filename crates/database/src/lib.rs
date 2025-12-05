pub mod error;

mod migrations;
mod models;
mod schema;

use std::sync::Arc;

use chrono::{Duration, NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel_async::{
    AsyncConnection, AsyncMigrationHarness, AsyncPgConnection, RunQueryDsl,
    pooled_connection::{AsyncDieselConnectionManager, deadpool::Pool},
};
use diesel_migrations::MigrationHarness;
use rand::{Rng, distr::Alphanumeric};

use crate::{
    error::{DatabaseError, Result},
    models::{Account, LinkRequest, User},
};

#[derive(Clone)]
pub struct Database(Arc<Pool<AsyncPgConnection>>);

impl Database {
    pub async fn connect(url: impl Into<String>) -> Result<Self> {
        let url = url.into();

        // Run migrations
        {
            let connection = AsyncPgConnection::establish(&url).await?;
            let mut harnass = AsyncMigrationHarness::new(connection);

            harnass
                .run_pending_migrations(migrations::MIGRATIONS)
                .map_err(crate::error::DatabaseError::MigrationFailed)?;
        }

        let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(url);
        let pool = Pool::builder(config).build()?;

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

    pub async fn update_account_token(
        &self,
        _user_id: &str,
        _access_token: &str,
        _refresh_token: Option<&str>,
        _expires: NaiveDateTime,
    ) -> Result<Account> {
        use schema::account::dsl::*;

        let mut connection = self.0.get().await?;
        let result = if let Some(refresh_token_val) = _refresh_token {
            diesel::update(account)
                .filter(user_id.eq(_user_id))
                .set((
                    access_token.eq(_access_token),
                    refresh_token.eq(refresh_token_val),
                    expires.eq(_expires),
                ))
                .returning(Account::as_returning())
                .get_result(&mut connection)
                .await?
        } else {
            diesel::update(account)
                .filter(user_id.eq(_user_id))
                .set((access_token.eq(_access_token), expires.eq(_expires)))
                .returning(Account::as_returning())
                .get_result(&mut connection)
                .await?
        };

        Ok(result)
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
        let _token: String = rand::rng()
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
}
