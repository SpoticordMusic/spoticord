use librespot::core::spotify_id::SpotifyId;

use crate::utils::{self, spotify};

#[derive(Clone)]
pub struct PlaybackInfo {
  last_updated: u128,
  position_ms: u32,

  pub track: Option<spotify::Track>,
  pub episode: Option<spotify::Episode>,
  pub spotify_id: Option<SpotifyId>,

  pub duration_ms: u32,
  pub is_playing: bool,
}

impl PlaybackInfo {
  /// Create a new instance of PlaybackInfo
  pub fn new(duration_ms: u32, position_ms: u32, is_playing: bool) -> Self {
    Self {
      last_updated: utils::get_time_ms(),
      track: None,
      episode: None,
      spotify_id: None,
      duration_ms,
      position_ms,
      is_playing,
    }
  }

  /// Update position, duration and playback state
  pub async fn update_pos_dur(&mut self, position_ms: u32, duration_ms: u32, is_playing: bool) {
    self.position_ms = position_ms;
    self.duration_ms = duration_ms;
    self.is_playing = is_playing;

    self.last_updated = utils::get_time_ms();
  }

  /// Update spotify id, track and episode
  pub fn update_track_episode(
    &mut self,
    spotify_id: SpotifyId,
    track: Option<spotify::Track>,
    episode: Option<spotify::Episode>,
  ) {
    self.spotify_id = Some(spotify_id);
    self.track = track;
    self.episode = episode;
  }

  /// Get the current playback position
  pub fn get_position(&self) -> u32 {
    if self.is_playing {
      let now = utils::get_time_ms();
      let diff = now - self.last_updated;

      self.position_ms + diff as u32
    } else {
      self.position_ms
    }
  }

  /// Get the name of the track or episode
  pub fn get_name(&self) -> Option<String> {
    if let Some(track) = &self.track {
      Some(track.name.clone())
    } else if let Some(episode) = &self.episode {
      Some(episode.name.clone())
    } else {
      None
    }
  }

  /// Get the artist(s) or show name of the current track
  pub fn get_artists(&self) -> Option<String> {
    if let Some(track) = &self.track {
      Some(
        track
          .artists
          .iter()
          .map(|a| a.name.clone())
          .collect::<Vec<String>>()
          .join(", "),
      )
    } else if let Some(episode) = &self.episode {
      Some(episode.show.name.clone())
    } else {
      None
    }
  }

  /// Get the album art url
  pub fn get_thumbnail_url(&self) -> Option<String> {
    if let Some(track) = &self.track {
      let mut images = track.album.images.clone();
      images.sort_by(|a, b| b.width.cmp(&a.width));

      Some(images.get(0).unwrap().url.clone())
    } else if let Some(episode) = &self.episode {
      let mut images = episode.show.images.clone();
      images.sort_by(|a, b| b.width.cmp(&a.width));

      Some(images.get(0).unwrap().url.clone())
    } else {
      None
    }
  }
}
