use std::time::Duration;

use anyhow::Result;
use librespot::core::SpotifyId;
use log::error;
use poise::serenity_prelude::{
    CommandInteraction, ComponentInteraction, ComponentInteractionCollector, Context,
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse,
    CreateInteractionResponseMessage, EditMessage, Message, futures::StreamExt,
};
use spoticord_shared::{
    lyrics::{Line, Lyrics, SyncType},
    player_info::PlayerInfo,
};
use tokio::task::JoinHandle;

use crate::{
    discord::EmbedColor,
    session::{Session, SessionHandle},
};

const PAGE_LENGTH: usize = 3000;
const TIME_OFFSET: u32 = 1000;

pub struct LyricsEmbed {
    guild_id: String,
    ctx: Context,
    session: SessionHandle,
    message: Message,
    track: SpotifyId,

    lyrics: Option<Lyrics>,
    page: usize,
}

impl LyricsEmbed {
    pub async fn create(
        session: &Session,
        handle: SessionHandle,
        interaction: CommandInteraction,
    ) -> Result<Option<JoinHandle<()>>> {
        let ctx = session.context.clone();

        if !session.active {
            respond_not_playing(&ctx, interaction).await?;

            return Ok(None);
        }

        let Some(player_info) = &session.player_info else {
            respond_not_playing(&ctx, interaction).await?;

            return Ok(None);
        };

        let guild_id = interaction
            .guild_id
            .expect("interaction performed outside of guild context")
            .to_string();
        let lyrics = player_info.lyrics().cloned();

        // Send initial response
        interaction
            .create_response(
                &ctx,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(lyrics_embed(&lyrics, player_info, 0))
                        .components(vec![lyrics_buttons(&guild_id, &lyrics, 0)]),
                ),
            )
            .await?;

        // Retrieve message instead of editing interaction response, as those tokens are only valid for 15 minutes
        let message = interaction.get_response(&ctx).await?;

        let this = Self {
            guild_id: guild_id.clone(),
            ctx: ctx.clone(),
            session: handle,
            message,
            track: player_info.track().track_id(),

            lyrics,
            page: 0,
        };

        let collector = ComponentInteractionCollector::new(&ctx)
            .filter(move |press| {
                let parts = press.data.custom_id.split(':').collect::<Vec<_>>();

                matches!(parts.first(), Some(&"lyrics"))
                    && matches!(parts.last(), Some(id) if id == &guild_id)
            })
            .timeout(Duration::from_secs(3600 * 24));

        let handle = tokio::spawn(this.run(collector));

        Ok(Some(handle))
    }

    async fn run(mut self, collector: ComponentInteractionCollector) {
        let mut stream = collector.stream();
        let mut interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if !self.handle_tick().await {
                        break;
                    }
                }

                press = stream.next() => {
                    let Some(press) = press else {
                        break;
                    };

                    // Immediately acknowledge, we don't have to inform the user about the update
                    _ = press.create_response(&self.ctx, CreateInteractionResponse::Acknowledge).await;

                    if !self.handle_press(press).await {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_tick(&mut self) -> bool {
        if !self.session.is_valid() {
            // Session died
            return false;
        }

        if !matches!(self.session.active().await, Ok(true)) {
            // If the session is currently not active, just wait until it becomes active again
            return true;
        }

        let Ok(Some(info)) = self.session.get_player_info().await else {
            // If we're not playing anything, just wait until we are
            return true;
        };

        if info.track().track_id() != self.track {
            // We're playing another track, reload the lyrics!

            self.lyrics = info.lyrics().cloned();
            self.page = 0;
            self.track = info.track().track_id();

            if let Err(why) = self
                .message
                .edit(
                    &self.ctx,
                    EditMessage::new()
                        .embed(lyrics_embed(&self.lyrics, &info, self.page))
                        .components(vec![lyrics_buttons(
                            &self.guild_id,
                            &self.lyrics,
                            self.page,
                        )]),
                )
                .await
            {
                error!("failed to update lyrics: {why}");

                return false;
            }

            return true;
        }

        // We're still playing the same song, check if we need to update the page

        let Some(lyrics) = &self.lyrics else {
            // No lyrics in current song, just continue waiting until we have a track with lyrics
            return true;
        };

        if !matches!(lyrics.sync_type, SyncType::LineSynced) {
            // Only synced lyrics can be automatically updated
            return true;
        }

        let new_page = page_at_position(lyrics, info.current_position()).unwrap_or(0);

        if new_page != self.page {
            // We've arrived on a new page

            self.page = new_page;

            if let Err(why) = self
                .message
                .edit(
                    &self.ctx,
                    EditMessage::new()
                        .embed(lyrics_embed(&self.lyrics, &info, new_page))
                        .components(vec![lyrics_buttons(&self.guild_id, &self.lyrics, new_page)]),
                )
                .await
            {
                error!("failed to update lyrics: {why}");

                return false;
            }
        }

        true
    }

    async fn handle_press(&mut self, press: ComponentInteraction) -> bool {
        let next = match press.data.custom_id.split(':').nth(1) {
            Some("next") => true,
            Some("prev") => false,
            _ => return true,
        };

        let Some(lyrics) = &self.lyrics else {
            return true;
        };

        if !matches!(lyrics.sync_type, SyncType::Unsynced) {
            // Only allow manual swapping if lyrics are unsynced

            return true;
        }

        let length = lyrics
            .lines
            .iter()
            .fold(0, |acc, line| acc + line.words.len());
        let pages = length / PAGE_LENGTH + if length % PAGE_LENGTH > 0 { 1 } else { 0 };

        let Ok(Some(info)) = self.session.get_player_info().await else {
            return true;
        };

        match next {
            true if self.page < pages - 1 => self.page += 1,
            false if self.page > 0 => self.page -= 1,
            _ => return true,
        };

        if let Err(why) = self
            .message
            .edit(
                &self.ctx,
                EditMessage::new()
                    .embed(lyrics_embed(&self.lyrics, &info, self.page))
                    .components(vec![lyrics_buttons(
                        &self.guild_id,
                        &self.lyrics,
                        self.page,
                    )]),
            )
            .await
        {
            error!("failed to update lyrics: {why}");

            return false;
        }

        true
    }
}

async fn respond_not_playing(context: &Context, interaction: CommandInteraction) -> Result<()> {
    interaction
        .create_response(
            context,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .embed(not_playing_embed())
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

fn not_playing_embed() -> CreateEmbed {
    CreateEmbed::new()
        .title("Cannot get lyrics")
        .description("I'm currently not playing any music in this server.")
        .color(EmbedColor::Error)
}

fn lyrics_embed(lyrics: &Option<Lyrics>, player_info: &PlayerInfo, page: usize) -> CreateEmbed {
    match (lyrics, player_info.track().artists()) {
        (Some(lyrics), Some(artists)) => {
            let length = lyrics
                .lines
                .iter()
                .fold(0, |acc, line| acc + line.words.len());

            let page =
                &into_pages(&lyrics.lines)[if page * PAGE_LENGTH > length { 0 } else { page }];

            let title = format!(
                "{} - {}",
                player_info.track().name(),
                artists
                    .into_iter()
                    .map(|artist| artist.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            let description = page
                .iter()
                .map(|page| page.words.replace('♪', "\n♪\n"))
                .collect::<Vec<_>>()
                .join("\n");

            let mut footer = format!("Lyrics provided by {}", lyrics.provider_display_name);

            if matches!(lyrics.sync_type, SyncType::LineSynced) {
                footer.push_str(" | Synced to song");
            }

            CreateEmbed::new()
                .title(title)
                .description(description)
                .footer(CreateEmbedFooter::new(footer))
                .color(EmbedColor::Info)
        }
        _ => CreateEmbed::new()
            .title("No lyrics available")
            .description("The current track has no lyrics available. Just enjoy the tunes!")
            .color(EmbedColor::Info),
    }
}

fn lyrics_buttons(id: &str, lyrics: &Option<Lyrics>, page: usize) -> CreateActionRow {
    let (can_prev, can_next) = match lyrics {
        Some(lyrics) => match lyrics.sync_type {
            SyncType::Unsynced => {
                // Only unsynced lyrics can be navigated through

                let length = lyrics
                    .lines
                    .iter()
                    .fold(0, |acc, line| acc + line.words.len());
                let pages = length / PAGE_LENGTH + if length % PAGE_LENGTH > 0 { 1 } else { 0 };

                (page > 0, page < pages - 1)
            }
            SyncType::LineSynced => (false, false),
        },
        None => (false, false),
    };

    CreateActionRow::Buttons(vec![
        CreateButton::new(format!("lyrics:prev:{id}"))
            .disabled(!can_prev)
            .label("<"),
        CreateButton::new(format!("lyrics:next:{id}"))
            .disabled(!can_next)
            .label(">"),
    ])
}

fn into_pages(lines: &[Line]) -> Vec<Vec<Line>> {
    let mut result = vec![];
    let mut current = vec![];
    let mut current_position = 0;

    for line in lines {
        if current_position + line.words.len() > PAGE_LENGTH {
            result.push(current);
            current = vec![line.clone()];
            current_position = line.words.len();
            continue;
        }

        current.push(line.clone());
        current_position += line.words.len();
    }

    result.push(current);
    result
}

fn page_at_position(lyrics: &Lyrics, position: u32) -> Option<usize> {
    let pages = into_pages(&lyrics.lines);

    for (i, line) in pages.iter().enumerate() {
        if let Some(first) = line.first() {
            let Ok(time) = first
                .start_time_ms
                .parse::<u32>()
                .map(|v| v.saturating_sub(TIME_OFFSET))
            else {
                return None;
            };

            if position < time {
                return Some(if i == 0 { 0 } else { i - 1 });
            }
        }

        if let (Some(first), Some(last)) = (line.first(), line.last()) {
            let (Ok(first), Ok(last)) = (
                first
                    .start_time_ms
                    .parse::<u32>()
                    .map(|v| v.saturating_sub(TIME_OFFSET)),
                last.start_time_ms
                    .parse::<u32>()
                    .map(|v| v.saturating_sub(TIME_OFFSET)),
            ) else {
                return None;
            };

            if position >= first && position <= last {
                return Some(i);
            }
        }
    }

    Some(pages.len() - 1)
}
