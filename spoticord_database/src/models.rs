use chrono::Utc;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = super::schema::user)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: String,
    pub device_name: String,
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = super::schema::account)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Account {
    pub user_id: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
    pub session_token: Option<String>,
    pub expires: chrono::NaiveDateTime,
}

impl Account {
    pub fn expired(&self) -> bool {
        Utc::now().naive_utc() > self.expires
    }

    pub fn expired_offset(&self, offset: chrono::Duration) -> bool {
        Utc::now().naive_utc() > self.expires - offset
    }
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = super::schema::link_request)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct LinkRequest {
    pub token: String,
    pub user_id: String,
    pub expires: chrono::NaiveDateTime,
}

impl LinkRequest {
    pub fn expired(&self) -> bool {
        Utc::now().naive_utc() > self.expires
    }

    pub fn expired_offset(&self, offset: chrono::Duration) -> bool {
        Utc::now().naive_utc() > self.expires - offset
    }
}
