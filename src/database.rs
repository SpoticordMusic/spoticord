use thiserror::Error;

use log::trace;
use reqwest::{header::HeaderMap, Client, Error, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use serenity::prelude::TypeMapKey;

use crate::utils;

#[derive(Debug, Error)]
pub enum DatabaseError {
  #[error("An error has occured during an I/O operation: {0}")]
  IOError(String),

  #[error("An error has occured during a parsing operation: {0}")]
  ParseError(String),

  #[error("An invalid status code was returned from a request: {0}")]
  InvalidStatusCode(StatusCode),

  #[error("An invalid input body was provided: {0}")]
  InvalidInputBody(String),
}

#[derive(Serialize, Deserialize)]
struct GetAccessTokenResponse {
  id: String,
  access_token: String,
}

#[derive(Deserialize)]
pub struct User {
  pub id: String,
  pub device_name: String,
  pub request: Option<Request>,
  pub accounts: Option<Vec<Account>>,
}

#[derive(Deserialize)]
pub struct Account {
  pub user_id: String,
  pub r#type: String,
  pub access_token: String,
  pub refresh_token: String,
  pub expires: u64,
}

#[derive(Deserialize)]
pub struct Request {
  pub token: String,
  pub user_id: String,
  pub expires: u64,
}

pub struct Database {
  base_url: String,
  default_headers: Option<HeaderMap>,
}

// Request options
#[derive(Debug, Clone)]
struct RequestOptions {
  pub method: Method,
  pub path: String,
  pub body: Option<Body>,
  pub headers: Option<HeaderMap>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Body {
  Json(Value),
  Text(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Method {
  Get,
  Post,
  Put,
  Delete,
  Patch,
}

impl Database {
  pub fn new(base_url: impl Into<String>, default_headers: Option<HeaderMap>) -> Self {
    Self {
      base_url: base_url.into(),
      default_headers,
    }
  }

  async fn request(&self, options: RequestOptions) -> Result<Response, Error> {
    let builder = Client::builder();
    let mut headers: HeaderMap = HeaderMap::new();
    let mut url = self.base_url.clone();

    url.push_str(&options.path);

    if let Some(default_headers) = &self.default_headers {
      headers.extend(default_headers.clone());
    }

    if let Some(request_headers) = options.headers {
      headers.extend(request_headers);
    }

    trace!("Requesting {} with headers: {:?}", url, headers);

    let client = builder.default_headers(headers).build()?;

    let mut request = match options.method {
      Method::Get => client.get(url),
      Method::Post => client.post(url),
      Method::Put => client.put(url),
      Method::Delete => client.delete(url),
      Method::Patch => client.patch(url),
    };

    request = if let Some(body) = options.body {
      match body {
        Body::Json(json) => request.json(&json),
        Body::Text(text) => request.body(text),
      }
    } else {
      request
    };

    let response = request.send().await?;

    Ok(response)
  }

  async fn simple_get<T: DeserializeOwned>(
    &self,
    path: impl Into<String>,
  ) -> Result<T, DatabaseError> {
    let response = match self
      .request(RequestOptions {
        method: Method::Get,
        path: path.into(),
        body: None,
        headers: None,
      })
      .await
    {
      Ok(response) => response,
      Err(error) => return Err(DatabaseError::IOError(error.to_string())),
    };

    match response.status() {
      StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED | StatusCode::NO_CONTENT => {}
      status => return Err(DatabaseError::InvalidStatusCode(status)),
    };

    let body = match response.json::<T>().await {
      Ok(body) => body,
      Err(error) => return Err(DatabaseError::ParseError(error.to_string())),
    };

    Ok(body)
  }

  async fn json_post<T: DeserializeOwned>(
    &self,
    value: impl Serialize,
    path: impl Into<String>,
  ) -> Result<T, DatabaseError> {
    let body = json!(value);

    let response = match self
      .request(RequestOptions {
        method: Method::Post,
        path: path.into(),
        body: Some(Body::Json(body)),
        headers: None,
      })
      .await
    {
      Ok(response) => response,
      Err(error) => return Err(DatabaseError::IOError(error.to_string())),
    };

    match response.status() {
      StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED | StatusCode::NO_CONTENT => {}
      status => return Err(DatabaseError::InvalidStatusCode(status)),
    };

    let body = match response.json::<T>().await {
      Ok(body) => body,
      Err(error) => return Err(DatabaseError::ParseError(error.to_string())),
    };

    Ok(body)
  }
}

impl Database {
  // Get Spoticord user
  pub async fn get_user(&self, user_id: impl Into<String>) -> Result<User, DatabaseError> {
    let path = format!("/user/{}", user_id.into());

    self.simple_get(path).await
  }

  // Get the Spotify access token for a user
  pub async fn get_access_token(
    &self,
    user_id: impl Into<String> + Send,
  ) -> Result<String, DatabaseError> {
    let body: GetAccessTokenResponse = self
      .simple_get(format!("/user/{}/spotify/access_token", user_id.into()))
      .await?;

    Ok(body.access_token)
  }

  // Get the Spotify account for a user
  pub async fn get_user_account(
    &self,
    user_id: impl Into<String> + Send,
  ) -> Result<Account, DatabaseError> {
    let body: Account = self
      .simple_get(format!("/account/{}/spotify", user_id.into()))
      .await?;

    Ok(body)
  }

  // Get the Request for a user
  pub async fn get_user_request(
    &self,
    user_id: impl Into<String> + Send,
  ) -> Result<Request, DatabaseError> {
    let body: Request = self
      .simple_get(format!("/request/by-user/{}", user_id.into()))
      .await?;

    Ok(body)
  }

  // Create a Spoticord user
  pub async fn create_user(&self, user_id: impl Into<String>) -> Result<User, DatabaseError> {
    let body = json!({
     "id": user_id.into(),
    });

    let user: User = self.json_post(body, "/user/new").await?;

    Ok(user)
  }

  // Create the link Request for a user
  pub async fn create_user_request(
    &self,
    user_id: impl Into<String> + Send,
  ) -> Result<Request, DatabaseError> {
    let body = json!({
      "user_id": user_id.into(),
      "expires": utils::get_time() + (1000 * 60 * 60)
    });

    let response = match self
      .request(RequestOptions {
        method: Method::Post,
        path: "/request".into(),
        body: Some(Body::Json(body)),
        headers: None,
      })
      .await
    {
      Ok(response) => response,
      Err(err) => return Err(DatabaseError::IOError(err.to_string())),
    };

    match response.status() {
      StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED | StatusCode::NO_CONTENT => {}
      status => return Err(DatabaseError::InvalidStatusCode(status)),
    };

    let body = match response.json::<Request>().await {
      Ok(body) => body,
      Err(error) => return Err(DatabaseError::ParseError(error.to_string())),
    };

    Ok(body)
  }

  pub async fn delete_user_account(
    &self,
    user_id: impl Into<String> + Send,
  ) -> Result<(), DatabaseError> {
    let response = match self
      .request(RequestOptions {
        method: Method::Delete,
        path: format!("/account/{}/spotify", user_id.into()),
        body: None,
        headers: None,
      })
      .await
    {
      Ok(response) => response,
      Err(err) => return Err(DatabaseError::IOError(err.to_string())),
    };

    match response.status() {
      StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED | StatusCode::NO_CONTENT => {}
      status => return Err(DatabaseError::InvalidStatusCode(status)),
    };

    Ok(())
  }

  pub async fn update_user_device_name(
    &self,
    user_id: impl Into<String>,
    name: impl Into<String>,
  ) -> Result<(), DatabaseError> {
    let device_name: String = name.into();

    if device_name.len() > 16 || device_name.len() < 1 {
      return Err(DatabaseError::InvalidInputBody(
        "Invalid device name length".into(),
      ));
    }

    let body = json!({ "device_name": device_name });

    let response = match self
      .request(RequestOptions {
        method: Method::Patch,
        path: format!("/user/{}", user_id.into()),
        body: Some(Body::Json(body)),
        headers: None,
      })
      .await
    {
      Ok(response) => response,
      Err(err) => return Err(DatabaseError::IOError(err.to_string())),
    };

    match response.status() {
      StatusCode::OK | StatusCode::CREATED | StatusCode::ACCEPTED | StatusCode::NO_CONTENT => {
        Ok(())
      }
      status => return Err(DatabaseError::InvalidStatusCode(status)),
    }
  }
}

impl TypeMapKey for Database {
  type Value = Database;
}
