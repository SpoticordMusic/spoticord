use std::error::Error;

use librespot::core::spotify_id::SpotifyId;
use log::{error, trace};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Artist {
  pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
  pub url: String,
  pub height: u32,
  pub width: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Album {
  pub name: String,
  pub images: Vec<Image>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalUrls {
  pub spotify: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Track {
  pub name: String,
  pub artists: Vec<Artist>,
  pub album: Album,
  pub external_urls: ExternalUrls,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Show {
  pub name: String,
  pub images: Vec<Image>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Episode {
  pub name: String,
  pub show: Show,
  pub external_urls: ExternalUrls,
}

pub async fn get_username(token: impl Into<String>) -> Result<String, String> {
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
        return Err(format!("{}", why));
      }
    };

    if response.status().as_u16() >= 500 && retries > 0 {
      retries -= 1;
      continue;
    }

    if response.status() != 200 {
      error!("Failed to get username: {}", response.status());
      return Err(format!(
        "Failed to get track info: Invalid status code: {}",
        response.status()
      ));
    }

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

    error!("Missing 'id' field in body: {:#?}", body);
    return Err("Failed to parse body: Invalid body received".to_string());
  }
}

pub async fn get_track_info(
  token: impl Into<String>,
  track: SpotifyId,
) -> Result<Track, Box<dyn Error>> {
  let token = token.into();
  let client = reqwest::Client::new();

  let mut retries = 3;

  loop {
    let response = client
      .get(format!(
        "https://api.spotify.com/v1/tracks/{}",
        track.to_base62()?
      ))
      .bearer_auth(&token)
      .send()
      .await?;

    if response.status().as_u16() >= 500 && retries > 0 {
      retries -= 1;
      continue;
    }

    if response.status() != 200 {
      return Err(
        format!(
          "Failed to get track info: Invalid status code: {}",
          response.status()
        )
        .into(),
      );
    }

    return Ok(response.json().await?);
  }
}

pub async fn get_episode_info(
  token: impl Into<String>,
  episode: SpotifyId,
) -> Result<Episode, Box<dyn Error>> {
  let token = token.into();
  let client = reqwest::Client::new();

  let mut retries = 3;

  loop {
    let response = client
      .get(format!(
        "https://api.spotify.com/v1/episodes/{}",
        episode.to_base62()?
      ))
      .bearer_auth(&token)
      .send()
      .await?;

    if response.status().as_u16() >= 500 && retries > 0 {
      retries -= 1;
      continue;
    }

    if response.status() != 200 {
      return Err(
        format!(
          "Failed to get episode info: Invalid status code: {}",
          response.status()
        )
        .into(),
      );
    }

    return Ok(response.json().await?);
  }
}
