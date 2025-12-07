use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use log::error;
use poise::{
    ChoiceParameter,
    serenity_prelude::{
        ButtonStyle, CommandInteraction, ComponentInteraction, ComponentInteractionCollector,
        Context, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
        CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, EditMessage,
        Message, User, futures::StreamExt,
    },
};
use spoticord_shared::player_info::PlayerInfo;
use tokio::sync::mpsc;

use crate::{
    discord::EmbedColor,
    session::{Session, SessionHandle},
};

#[derive(Debug)]
pub enum Command {
    InvokeUpdate(bool),
}

#[derive(Debug, Default, ChoiceParameter)]
pub enum UpdateBehavior {
    #[default]
    #[name = "Automatically update the embed"]
    Default,

    #[name = "Do not update the embed"]
    Static,

    #[name = "Re-send the embed after track changes"]
    Pinned,
}

impl UpdateBehavior {
    pub fn is_static(&self) -> bool {
        matches!(self, Self::Static)
    }

    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::Pinned)
    }
}

pub struct PlaybackEmbed {
    id: u64,
    ctx: Context,
    session: SessionHandle,
    message: Message,

    last_update: Instant,
    update_in: Option<Duration>,
    force_edit: bool,
    update_behavior: UpdateBehavior,

    rx: mpsc::Receiver<Command>,
}

impl PlaybackEmbed {
    pub async fn create(
        session: &Session,
        handle: SessionHandle,
        interaction: CommandInteraction,
        update_behavior: UpdateBehavior,
    ) -> Result<Option<PlaybackEmbedHandle>> {
        let ctx = session.context.clone();

        if !session.active {
            respond_not_playing(&ctx, interaction).await?;

            return Ok(None);
        }

        let owner = session.owner_id.to_user(&ctx).await?;

        let Some(player_info) = &session.player_info else {
            respond_not_playing(&ctx, interaction).await?;

            return Ok(None);
        };

        let ctx_id = interaction.id.get();

        // Send initial reply
        interaction
            .create_response(
                &ctx,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .embed(build_embed(player_info, &owner))
                        .components(vec![build_buttons(ctx_id, player_info.playing())]),
                ),
            )
            .await?;

        // If this is a static embed, we don't need to return any handles
        if update_behavior.is_static() {
            return Ok(None);
        }

        // Retrieve message instead of editing interaction response, as those tokens are only valid for 15 minutes
        let message = interaction.get_response(&ctx).await?;

        let collector = ComponentInteractionCollector::new(&ctx)
            .filter(move |press| press.data.custom_id.starts_with(&ctx_id.to_string()))
            .timeout(Duration::from_secs(3600 * 24));

        let (tx, rx) = mpsc::channel(16);
        let this = Self {
            id: ctx_id,
            ctx,
            session: handle,
            message,

            last_update: Instant::now(),
            update_in: None,
            force_edit: false,
            update_behavior,

            rx,
        };

        tokio::spawn(this.run(collector));

        Ok(Some(PlaybackEmbedHandle { tx }))
    }

    async fn run(mut self, collector: ComponentInteractionCollector) {
        let mut stream = collector.stream();

        loop {
            tokio::select! {
                command = self.rx.recv() => {
                    let Some(command) = command else {
                        break;
                    };

                    if !self.handle_command(command).await {
                        break;
                    }
                }

                press = stream.next() => {
                    let Some(press) = press else {
                        break;
                    };

                    self.handle_press(press).await;
                }

                _ = async {
                    if let Some(update_in) = self.update_in.take()
                    {
                        tokio::time::sleep(update_in).await;
                    }
                }, if self.update_in.is_some() => {
                    if !self.update_embed(self.force_edit).await {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_command(&mut self, command: Command) -> bool {
        match command {
            Command::InvokeUpdate(force_edit) => {
                // Throttle updates to once every 2 seconds
                if self.last_update.elapsed() < Duration::from_secs(2) {
                    // If an update is already queued, just return early
                    if self.update_in.is_some() {
                        return true;
                    }

                    self.update_in = Some(Duration::from_secs(2) - self.last_update.elapsed());
                    self.force_edit = force_edit;
                } else if !self.update_embed(force_edit).await {
                    return false;
                }
            }
        }

        true
    }

    async fn handle_press(&self, press: ComponentInteraction) {
        let Ok((player_info, owner)) = self.get_info().await else {
            _ = press
                .create_response(
                    &self.ctx,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .embed(
                                CreateEmbed::new()
                                    .title("Cannot perform action")
                                    .description(
                                        "I'm currently not playing any music in this server",
                                    )
                                    .color(EmbedColor::Error),
                            )
                            .ephemeral(true),
                    ),
                )
                .await;

            return;
        };

        // TODO: Allow non owners to mutate if authorized using for example `session.can_mutate(press.user.id)`

        if press.user.id != owner.id {
            _ = press
                .create_response(
                    &self.ctx,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .embed(
                                CreateEmbed::new()
                                    .title("Cannot perform action")
                                    .description("Only the host may use the media buttons")
                                    .color(EmbedColor::Error),
                            )
                            .ephemeral(true),
                    ),
                )
                .await;

            return;
        }

        match press.data.custom_id.split('-').next_back() {
            Some("next") => self.session.next_track().await,
            Some("prev") => self.session.previous_track().await,
            Some("pause") => {
                if player_info.playing() {
                    self.session.pause().await
                } else {
                    self.session.play().await
                }
            }

            _ => {}
        }

        _ = press
            .create_response(&self.ctx, CreateInteractionResponse::Acknowledge)
            .await;
    }

    async fn get_info(&self) -> Result<(PlayerInfo, User)> {
        let owner = self
            .session
            .get_owner_id()
            .await?
            .to_user(&self.ctx)
            .await?;
        let player_info = self
            .session
            .get_player_info()
            .await?
            .ok_or_else(|| anyhow!("No player info present"))?;

        Ok((player_info, owner))
    }

    async fn update_embed(&mut self, force_edit: bool) -> bool {
        self.update_in = None;

        let Ok(owner) = self.session.get_owner_id().await else {
            _ = self.update_not_playing().await;

            return false;
        };

        let Ok(Some(player_info)) = self.session.get_player_info().await else {
            _ = self.update_not_playing().await;

            return false;
        };

        let owner = match owner.to_user(&self.ctx).await {
            Ok(owner) => owner,
            Err(why) => {
                error!("failed to resolve owner: {why}");

                return false;
            }
        };

        let should_pin = !force_edit && self.update_behavior.is_pinned();

        if should_pin {
            _ = self.message.delete(&self.ctx).await;

            match self
                .message
                .channel_id
                .send_message(
                    &self.ctx,
                    CreateMessage::new()
                        .embed(build_embed(&player_info, &owner))
                        .components(vec![build_buttons(self.id, player_info.playing())]),
                )
                .await
            {
                Ok(message) => self.message = message,
                Err(why) => {
                    error!("failed to update playback embed: {why}");

                    return false;
                }
            };
        } else if let Err(why) = self
            .message
            .edit(
                &self.ctx,
                EditMessage::new()
                    .embed(build_embed(&player_info, &owner))
                    .components(vec![build_buttons(self.id, player_info.playing())]),
            )
            .await
        {
            error!("failed to update playback embed: {why}");

            return false;
        }

        true
    }

    async fn update_not_playing(&mut self) -> Result<()> {
        // If pinned, try to delete old message and send new one
        if self.update_behavior.is_pinned() {
            _ = self.message.delete(&self.ctx).await;

            self.message = self
                .message
                .channel_id
                .send_message(&self.ctx, CreateMessage::new().embed(not_playing_embed()))
                .await?;

            return Ok(());
        }

        self.message
            .edit(&self.ctx, EditMessage::new().embed(not_playing_embed()))
            .await?;

        Ok(())
    }
}

pub struct PlaybackEmbedHandle {
    tx: mpsc::Sender<Command>,
}

impl PlaybackEmbedHandle {
    pub async fn invoke_update(&self, force_edit: bool) -> Result<()> {
        self.tx.send(Command::InvokeUpdate(force_edit)).await?;

        Ok(())
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
        .title("Cannot display song details")
        .description("I'm currently not playing any music in this server.")
        .color(EmbedColor::Error)
}

fn build_embed(player_info: &PlayerInfo, owner: &User) -> CreateEmbed {
    let mut description = String::new();

    description += &format!(
        "## [{}]({})\n",
        player_info.track().name(),
        player_info.track().url()
    );

    if let Some(artists) = player_info.track().artists() {
        let artists = artists
            .iter()
            .map(|artist| {
                format!(
                    "[{}](https://open.spotify.com/artist/{})",
                    artist.name,
                    artist.id.to_base62()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        description += &format!("By {artists}\n");
    }

    if let Some(album_name) = player_info.track().album_name() {
        description += &format!("Album: **{album_name}**\n");
    }

    if let Some(show_name) = player_info.track().show_name() {
        description += &format!("On {show_name}\n");
    }

    description += "\n";

    let position = player_info.current_position();
    let index = position * 20 / player_info.track().duration();

    description += if player_info.playing() {
        "▶️ "
    } else {
        "⏸️ "
    };

    for i in 0..20 {
        if i == index {
            description.push('🔵');
        } else {
            description.push('▬');
        }
    }

    description += "\n:alarm_clock: ";
    description += &format!(
        "{} / {}",
        time_to_string(position / 1000),
        time_to_string(player_info.track().duration() / 1000)
    );

    let mut embed = CreateEmbed::new()
        .author(
            CreateEmbedAuthor::new("Currently Playing")
                .icon_url("https://spoticord.com/spotify-logo.png"),
        )
        .description(description)
        .footer(
            CreateEmbedFooter::new(owner.global_name.as_ref().unwrap_or(&owner.name))
                .icon_url(owner.face()),
        )
        .color(EmbedColor::Info);

    if let Some(thumbnail) = player_info.track().thumbnail() {
        embed = embed.thumbnail(thumbnail);
    }

    embed
}

fn build_buttons(id: u64, playing: bool) -> CreateActionRow {
    let prev_button_id = format!("{id}-prev");
    let next_button_id = format!("{id}-next");
    let pause_button_id = format!("{id}-pause");

    let prev_button = CreateButton::new(prev_button_id)
        .style(ButtonStyle::Primary)
        .label("<<");

    let next_button = CreateButton::new(next_button_id)
        .style(ButtonStyle::Primary)
        .label(">>");

    let pause_button = CreateButton::new(pause_button_id)
        .style(if playing {
            ButtonStyle::Danger
        } else {
            ButtonStyle::Success
        })
        .label(if playing { "Pause" } else { "Play" });

    CreateActionRow::Buttons(vec![prev_button, pause_button, next_button])
}

fn time_to_string(time: u32) -> String {
    let hour = 3600;
    let min = 60;

    if time / hour >= 1 {
        format!(
            "{}h{}m{}s",
            time / hour,
            (time % hour) / min,
            (time % hour) % min
        )
    } else if time / min >= 1 {
        format!("{}m{}s", time / min, time % min)
    } else {
        format!("{}s", time)
    }
}
