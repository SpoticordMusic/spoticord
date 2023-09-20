use anyhow::{anyhow, Result};
use log::{error, trace};
use serde_json::Value;

pub async fn get_username(token: impl Into<String>) -> Result<String> {
  let token = token.into();
  let client = reqwest::Client::new();

  let mut retries = 3;

  loop {
    let response = match client
      .get("https://api.spotify.com/v1/me")
      .bearer_auth(&token)
      .send()
      .await
    {
      Ok(response) => response,
      Err(why) => {
        error!("Failed to get username: {}", why);
        return Err(why.into());
      }
    };

    if response.status().as_u16() >= 500 && retries > 0 {
      retries -= 1;
      continue;
    }

    if response.status() != 200 {
      error!("Failed to get username: {}", response.status());
      return Err(anyhow!(
        "Failed to get track info: Invalid status code: {}",
        response.status()
      ));
    }

    let body: Value = match response.json().await {
      Ok(body) => body,
      Err(why) => {
        error!("Failed to parse body: {}", why);
        return Err(why.into());
      }
    };

    if let Value::String(username) = &body["id"] {
      trace!("Got username: {}", username);
      return Ok(username.clone());
    }

    error!("Missing 'id' field in body: {:#?}", body);
    return Err(anyhow!("Failed to parse body: Invalid body received"));
  }
}
