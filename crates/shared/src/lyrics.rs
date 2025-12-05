use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub is_dense_typeface: bool,
    pub is_rtl_language: bool,
    pub language: String,
    pub lines: Vec<Line>,
    pub provider: String,
    pub provider_display_name: String,
    pub provider_lyrics_id: String,
    pub sync_lyrics_uri: String,
    pub sync_type: SyncType,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub start_time_ms: String,
    pub end_time_ms: String,
    pub words: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncType {
    Unsynced,
    LineSynced,
}

impl From<librespot::metadata::Lyrics> for Lyrics {
    fn from(value: librespot::metadata::Lyrics) -> Self {
        Self {
            is_dense_typeface: value.lyrics.is_dense_typeface,
            is_rtl_language: value.lyrics.is_rtl_language,
            language: value.lyrics.language,
            lines: value.lyrics.lines.into_iter().map(|v| v.into()).collect(),
            provider: value.lyrics.provider,
            provider_display_name: value.lyrics.provider_display_name,
            provider_lyrics_id: value.lyrics.provider_lyrics_id,
            sync_lyrics_uri: value.lyrics.sync_lyrics_uri,
            sync_type: value.lyrics.sync_type.into(),
        }
    }
}

impl From<librespot::metadata::lyrics::SyncType> for SyncType {
    fn from(value: librespot::metadata::lyrics::SyncType) -> Self {
        match value {
            librespot::metadata::lyrics::SyncType::LineSynced => SyncType::LineSynced,
            librespot::metadata::lyrics::SyncType::Unsynced => SyncType::Unsynced,
        }
    }
}

impl From<librespot::metadata::lyrics::Line> for Line {
    fn from(value: librespot::metadata::lyrics::Line) -> Self {
        Self {
            words: value.words,
            end_time_ms: value.end_time_ms,
            start_time_ms: value.start_time_ms,
        }
    }
}
