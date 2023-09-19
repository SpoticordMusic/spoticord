use librespot::{
  core::spotify_id::SpotifyId,
  protocol::metadata::{Episode, Track},
};

use crate::utils;

#[derive(Clone)]
pub struct PlaybackInfo {
  last_updated: u128,
  position_ms: u32,

  pub track: CurrentTrack,
  pub spotify_id: SpotifyId,

  pub duration_ms: u32,
  pub is_playing: bool,
}

#[derive(Clone)]
pub enum CurrentTrack {
  Track(Track),
  Episode(Episode),
}

impl PlaybackInfo {
  /// Create a new instance of PlaybackInfo
  pub fn new(
    duration_ms: u32,
    position_ms: u32,
    is_playing: bool,
    track: CurrentTrack,
    spotify_id: SpotifyId,
  ) -> Self {
    Self {
      last_updated: utils::get_time_ms(),
      track,
      spotify_id,
      duration_ms,
      position_ms,
      is_playing,
    }
  }

  /// Update position, duration and playback state
  pub fn update_pos_dur(&mut self, position_ms: u32, duration_ms: u32, is_playing: bool) {
    self.position_ms = position_ms;
    self.duration_ms = duration_ms;
    self.is_playing = is_playing;

    self.last_updated = utils::get_time_ms();
  }

  /// Update spotify id, track and episode
  pub fn update_track(&mut self, track: CurrentTrack) {
    self.track = track;
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
  pub fn get_name(&self) -> String {
    match &self.track {
      CurrentTrack::Track(track) => track.get_name().to_string(),
      CurrentTrack::Episode(episode) => episode.get_name().to_string(),
    }
  }

  /// Get the artist(s) or show name of the current track
  pub fn get_artists(&self) -> String {
    match &self.track {
      CurrentTrack::Track(track) => track
        .get_artist()
        .iter()
        .map(|a| a.get_name().to_string())
        .collect::<Vec<_>>()
        .join(", "),
      CurrentTrack::Episode(episode) => episode.get_show().get_name().to_string(),
    }
  }

  /// Get the album art url
  pub fn get_thumbnail_url(&self) -> Option<String> {
    let file_id = match &self.track {
      CurrentTrack::Track(track) => {
        let mut images = track.get_album().get_cover_group().get_image().to_vec();
        images.sort_by_key(|b| std::cmp::Reverse(b.get_width()));

        images
          .get(0)
          .as_ref()
          .map(|image| image.get_file_id())
          .map(hex::encode)
      }
      CurrentTrack::Episode(episode) => {
        let mut images = episode.get_covers().get_image().to_vec();
        images.sort_by_key(|b| std::cmp::Reverse(b.get_width()));

        images
          .get(0)
          .as_ref()
          .map(|image| image.get_file_id())
          .map(hex::encode)
      }
    };

    file_id.map(|id| format!("https://i.scdn.co/image/{id}"))
  }

  /// Get the type of audio (track or episode)
  #[allow(dead_code)]
  pub fn get_type(&self) -> String {
    match &self.track {
      CurrentTrack::Track(_) => "track".to_string(),
      CurrentTrack::Episode(_) => "episode".to_string(),
    }
  }

  /// Get the public facing url of the track or episode
  #[allow(dead_code)]
  pub fn get_url(&self) -> Option<&str> {
    match &self.track {
      CurrentTrack::Track(track) => track
        .get_external_id()
        .iter()
        .find(|id| id.get_typ() == "spotify")
        .map(|v| v.get_id()),
      CurrentTrack::Episode(episode) => Some(episode.get_external_url()),
    }
  }
}
