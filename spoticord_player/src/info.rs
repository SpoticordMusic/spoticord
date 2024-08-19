use std::collections::HashSet;

use librespot::{
    core::SpotifyId,
    metadata::{
        artist::ArtistsWithRole,
        audio::{AudioItem, UniqueFields},
    },
};

#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    audio_item: AudioItem,

    updated_at: u128,
    position: u32,
    playing: bool,
}

impl PlaybackInfo {
    pub fn new(audio_item: AudioItem, position: u32, playing: bool) -> Self {
        Self {
            audio_item,

            updated_at: spoticord_utils::get_time(),
            position,
            playing,
        }
    }

    pub fn track_id(&self) -> SpotifyId {
        self.audio_item.track_id
    }

    pub fn track_id_string(&self) -> String {
        self.audio_item
            .track_id
            .to_base62()
            .expect("invalid spotify id")
    }

    pub fn name(&self) -> String {
        self.audio_item.name.clone()
    }

    pub fn artists(&self) -> Option<ArtistsWithRole> {
        let artists = match &self.audio_item.unique_fields {
            UniqueFields::Track { artists, .. } => artists.clone().0,
            UniqueFields::Episode { .. } => None?,
        };

        let mut seen = HashSet::new();
        let artists = artists
            .into_iter()
            .filter(|item| seen.insert(item.id))
            .collect();

        Some(ArtistsWithRole(artists))
    }

    pub fn show_name(&self) -> Option<String> {
        match &self.audio_item.unique_fields {
            UniqueFields::Episode { show_name, .. } => Some(show_name.to_string()),
            UniqueFields::Track { .. } => None,
        }
    }

    pub fn album_name(&self) -> Option<String> {
        match &self.audio_item.unique_fields {
            UniqueFields::Episode { .. } => None,
            UniqueFields::Track { album, .. } => Some(album.to_string()),
        }
    }

    pub fn thumbnail(&self) -> String {
        self.audio_item
            .covers
            .first()
            .expect("spotify track missing cover image")
            .url
            .to_string()
    }

    pub fn duration(&self) -> u32 {
        self.audio_item.duration_ms
    }

    pub fn url(&self) -> String {
        match &self.audio_item.unique_fields {
            UniqueFields::Episode { .. } => format!(
                "https://open.spotify.com/episode/{}",
                self.track_id_string()
            ),
            UniqueFields::Track { .. } => {
                format!("https://open.spotify.com/track/{}", self.track_id_string())
            }
        }
    }

    /// Get the current playback position, which accounts for time that may have passed since this struct was last updated
    pub fn current_position(&self) -> u32 {
        if self.playing {
            let now = spoticord_utils::get_time();
            let diff = now - self.updated_at;

            self.position + diff as u32
        } else {
            self.position
        }
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn update_playback(&mut self, position: u32, playing: bool) {
        self.position = position;
        self.playing = playing;
        self.updated_at = spoticord_utils::get_time();
    }

    pub fn update_track(&mut self, audio_item: AudioItem) {
        self.audio_item = audio_item;
    }

    pub fn is_episode(&self) -> bool {
        matches!(self.audio_item.unique_fields, UniqueFields::Episode { .. })
    }

    pub fn is_track(&self) -> bool {
        matches!(self.audio_item.unique_fields, UniqueFields::Track { .. })
    }
}
