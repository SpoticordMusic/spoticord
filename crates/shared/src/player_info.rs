use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use librespot::{
    core::{SpotifyId, SpotifyUri},
    metadata::audio::{AudioItem, UniqueFields},
};
use serde::{Deserialize, Serialize};

use crate::lyrics::Lyrics;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(remote = "SpotifyId")]
pub struct SpotifyIdDef {
    pub id: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    current_track: PlayerTrack,
    current_lyrics: Option<Lyrics>,

    updated_at: u128,
    position: u32,
    playing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerTrack {
    // Music
    Track {
        #[serde(with = "SpotifyIdDef")]
        id: SpotifyId,
        name: String,
        artists: Vec<Artist>,
        album: String,
        thumbnail: Option<String>,
        duration: u32,
        url: String,
    },

    // Podcasts
    Episode {
        #[serde(with = "SpotifyIdDef")]
        id: SpotifyId,
        name: String,
        show_name: String,
        thumbnail: String,
        duration: u32,
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,

    #[serde(with = "SpotifyIdDef")]
    pub id: SpotifyId,
}

impl TryFrom<AudioItem> for PlayerTrack {
    type Error = ();

    fn try_from(value: AudioItem) -> Result<Self, Self::Error> {
        Ok(match (value.track_id, value.unique_fields) {
            (SpotifyUri::Track { id }, UniqueFields::Track { artists, album, .. }) => Self::Track {
                id,
                name: value.name,
                artists: {
                    let mut seen = HashSet::new();

                    artists
                        .0
                        .clone()
                        .into_iter()
                        .filter(|item| seen.insert(item.id.to_id()))
                        .map(|item| Artist {
                            name: item.name,
                            id: match item.id {
                                SpotifyUri::Artist { id } => id,
                                _ => unreachable!(),
                            },
                        })
                        .collect()
                },
                album,
                thumbnail: value.covers.first().map(|cover| cover.url.clone()),
                duration: value.duration_ms,
                url: format!("https://open.spotify.com/track/{}", id.to_base62()),
            },
            (SpotifyUri::Episode { id }, UniqueFields::Episode { show_name, .. }) => {
                Self::Episode {
                    id,
                    name: value.name,
                    show_name,
                    thumbnail: value
                        .covers
                        .first()
                        .expect("spotify track missing cover image")
                        .url
                        .clone(),
                    duration: value.duration_ms,
                    url: format!("https://open.spotify.com/episode/{}", id.to_base62()),
                }
            }
            _ => Err(())?,
        })
    }
}

impl PlayerTrack {
    pub fn track_id(&self) -> SpotifyId {
        match *self {
            Self::Episode { id, .. } => id,
            Self::Track { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Episode { name, .. } => name,
            Self::Track { name, .. } => name,
        }
    }

    pub fn artists(&self) -> Option<Vec<Artist>> {
        match self {
            Self::Episode { .. } => None,
            Self::Track { artists, .. } => Some(artists.clone()),
        }
    }

    pub fn show_name(&self) -> Option<String> {
        match self {
            Self::Episode { show_name, .. } => Some(show_name.to_string()),
            Self::Track { .. } => None,
        }
    }

    pub fn album_name(&self) -> Option<String> {
        match self {
            Self::Episode { .. } => None,
            Self::Track { album, .. } => Some(album.clone()),
        }
    }

    pub fn thumbnail(&self) -> Option<String> {
        match self {
            Self::Episode { thumbnail, .. } => Some(thumbnail.to_string()),
            Self::Track { thumbnail, .. } => thumbnail.as_ref().cloned(),
        }
    }

    pub fn duration(&self) -> u32 {
        match *self {
            Self::Episode { duration, .. } => duration,
            Self::Track { duration, .. } => duration,
        }
    }

    pub fn url(&self) -> String {
        match self {
            Self::Episode { url, .. } => url.to_string(),
            Self::Track { url, .. } => url.to_string(),
        }
    }
}

impl PlayerInfo {
    pub fn new(
        current_track: AudioItem,
        current_lyrics: Option<Lyrics>,
        position: u32,
        playing: bool,
    ) -> Option<Self> {
        Some(Self {
            current_track: current_track.try_into().ok()?,
            current_lyrics,

            updated_at: get_time(),
            position,
            playing,
        })
    }

    pub fn track(&self) -> &PlayerTrack {
        &self.current_track
    }

    pub fn lyrics(&self) -> Option<&Lyrics> {
        self.current_lyrics.as_ref()
    }

    /// Get the current playback position, which accounts for the time that has passed since this struct was last updated
    pub fn current_position(&self) -> u32 {
        if self.playing {
            self.position + (get_time() - self.updated_at) as u32
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
        self.updated_at = get_time();
    }

    pub fn update_track(&mut self, track: AudioItem, lyrics: Option<Lyrics>) {
        if let Ok(track) = track.try_into() {
            self.current_track = track;
        }

        self.current_lyrics = lyrics;
    }

    pub fn is_episode(&self) -> bool {
        matches!(self.current_track, PlayerTrack::Episode { .. })
    }

    pub fn is_track(&self) -> bool {
        matches!(self.current_track, PlayerTrack::Track { .. })
    }
}

fn get_time() -> u128 {
    let now = SystemTime::now();
    let since_epoch = now
        .duration_since(UNIX_EPOCH)
        .expect("must be the black hole in my room");

    since_epoch.as_millis()
}
