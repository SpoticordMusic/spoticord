use log::{error, trace};
use serde_json::Value;

pub async fn get_username(token: impl Into<String>) -> Result<String, String> {
  let token = token.into();
  let client = reqwest::Client::new();

  let response = match client
    .get("https://api.spotify.com/v1/me")
    .bearer_auth(token)
    .send()
    .await
  {
    Ok(response) => response,
    Err(why) => {
      error!("Failed to get username: {}", why);
      return Err(format!("{}", why));
    }
  };

  let body: Value = match response.json().await {
    Ok(body) => body,
    Err(why) => {
      error!("Failed to parse body: {}", why);
      return Err(format!("{}", why));
    }
  };

  if let Value::String(username) = &body["id"] {
    trace!("Got username: {}", username);
    return Ok(username.clone());
  }

  error!("Missing 'id' field in body");
  Err("Failed to parse body: Invalid body received".to_string())
}
