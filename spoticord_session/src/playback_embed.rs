use anyhow::{anyhow, Result};
use log::{error, trace};
use serenity::{
    all::{
        ButtonStyle, CommandInteraction, ComponentInteraction, ComponentInteractionCollector,
        Context, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
        CreateInteractionResponse, CreateInteractionResponseFollowup,
        CreateInteractionResponseMessage, EditMessage, Message, User,
    },
    futures::StreamExt,
};
use spoticord_player::{info::PlaybackInfo, PlayerHandle};
use spoticord_utils::discord::Colors;
use std::{ops::ControlFlow, time::Duration};
use tokio::{sync::mpsc, time::Instant};

use crate::{Session, SessionHandle};

#[derive(Debug)]
pub enum Command {
    InvokeUpdate,
}

pub struct PlaybackEmbed {
    id: u64,
    ctx: Context,
    session: SessionHandle,
    message: Message,

    last_update: Instant,
    update_in: Option<Duration>,

    rx: mpsc::Receiver<Command>,
}

impl PlaybackEmbed {
    pub async fn create(
        session: &Session,
        handle: SessionHandle,
        interaction: CommandInteraction,
    ) -> Result<Option<PlaybackEmbedHandle>> {
        let ctx = session.context.clone();

        if !session.active {
            respond_not_playing(&ctx, interaction).await?;

            return Ok(None);
        }

        let owner = session.owner.to_user(&ctx).await?;

        let Some(playback_info) = session.player.playback_info().await? else {
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
                        .embed(build_embed(&playback_info, &owner))
                        .components(vec![build_buttons(ctx_id, playback_info.playing())]),
                ),
            )
            .await?;

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
            rx,
        };

        tokio::spawn(this.run(collector));

        Ok(Some(PlaybackEmbedHandle { tx }))
    }

    async fn run(mut self, collector: ComponentInteractionCollector) {
        let mut stream = collector.stream();

        loop {
            tokio::select! {
                opt_command = self.rx.recv() => {
                    let Some(command) = opt_command else {
                        break;
                    };

                    if self.handle_command(command).await.is_break() {
                        break;
                    }
                },

                opt_press = stream.next() => {
                    let Some(press) = opt_press else {
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
                    if self.update_embed().await.is_break() {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_command(&mut self, command: Command) -> ControlFlow<(), ()> {
        trace!("Received command: {command:?}");

        match command {
            Command::InvokeUpdate => {
                if self.last_update.elapsed() < Duration::from_secs(2) {
                    if self.update_in.is_some() {
                        return ControlFlow::Continue(());
                    }

                    self.update_in = Some(Duration::from_secs(2) - self.last_update.elapsed());
                } else {
                    self.update_embed().await?;
                }
            }
        }

        ControlFlow::Continue(())
    }

    async fn handle_press(&self, press: ComponentInteraction) {
        trace!("Received button press: {press:?}");

        let Ok((player, playback_info, owner)) = self.get_info().await else {
            _ = press
                .create_followup(
                    &self.ctx,
                    CreateInteractionResponseFollowup::new()
                        .embed(
                            CreateEmbed::new()
                                .title("Cannot perform action")
                                .description("I'm currently not playing any music in this server"),
                        )
                        .ephemeral(true),
                )
                .await;

            return;
        };

        if press.user.id != owner.id {
            _ = press
                .create_followup(
                    &self.ctx,
                    CreateInteractionResponseFollowup::new()
                        .embed(
                            CreateEmbed::new()
                                .title("Cannot perform action")
                                .description("Only the host may use the media buttons"),
                        )
                        .ephemeral(true),
                )
                .await;

            return;
        }

        match press.data.custom_id.split('-').last() {
            Some("next") => player.next_track().await,
            Some("prev") => player.previous_track().await,
            Some("pause") => {
                if playback_info.playing() {
                    player.pause().await
                } else {
                    player.play().await
                }
            }

            _ => {}
        }

        _ = press
            .create_response(&self.ctx, CreateInteractionResponse::Acknowledge)
            .await;
    }

    async fn get_info(&self) -> Result<(PlayerHandle, PlaybackInfo, User)> {
        let player = self.session.player().await?;
        let owner = self.session.owner().await?.to_user(&self.ctx).await?;
        let playback_info = player
            .playback_info()
            .await?
            .ok_or_else(|| anyhow!("No playback info present"))?;

        Ok((player, playback_info, owner))
    }

    async fn update_embed(&mut self) -> ControlFlow<(), ()> {
        self.update_in = None;

        let Ok(owner) = self.session.owner().await else {
            _ = self.update_not_playing().await;

            return ControlFlow::Break(());
        };

        let Ok(player) = self.session.player().await else {
            _ = self.update_not_playing().await;

            return ControlFlow::Break(());
        };

        let Ok(Some(playback_info)) = player.playback_info().await else {
            _ = self.update_not_playing().await;

            return ControlFlow::Break(());
        };

        let owner = match owner.to_user(&self.ctx).await {
            Ok(owner) => owner,
            Err(why) => {
                error!("Failed to resolve owner: {why}");

                return ControlFlow::Break(());
            }
        };

        if let Err(why) = self
            .message
            .edit(
                &self.ctx,
                EditMessage::new()
                    .embed(build_embed(&playback_info, &owner))
                    .components(vec![build_buttons(self.id, playback_info.playing())]),
            )
            .await
        {
            error!("Failed to update playback embed: {why}");

            return ControlFlow::Break(());
        };

        self.last_update = Instant::now();

        ControlFlow::Continue(())
    }

    async fn update_not_playing(&mut self) -> Result<()> {
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
    pub fn is_valid(&self) -> bool {
        !self.tx.is_closed()
    }

    pub async fn invoke_update(&self) -> Result<()> {
        self.tx.send(Command::InvokeUpdate).await?;

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
        .color(Colors::Error)
}

fn build_embed(playback_info: &PlaybackInfo, owner: &User) -> CreateEmbed {
    let mut description = String::new();

    description += &format!("## [{}]({})\n", playback_info.name(), playback_info.url());

    if let Some(artists) = playback_info.artists() {
        let artists = artists
            .iter()
            .map(|artist| {
                format!(
                    "[{}](https://open.spotify.com/artist/{})",
                    artist.name,
                    artist.id.to_base62().expect("invalid artist")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        description += &format!("By {artists}\n\n");
    }

    if let Some(show_name) = playback_info.show_name() {
        description += &format!("On {show_name}\n\n");
    }

    let position = playback_info.current_position();
    let index = position * 20 / playback_info.duration();

    description.push_str(if playback_info.playing() {
        "▶️ "
    } else {
        "⏸️ "
    });

    for i in 0..20 {
        if i == index {
            description.push('🔵');
        } else {
            description.push('▬');
        }
    }

    description.push_str("\n:alarm_clock: ");
    description.push_str(&format!(
        "{} / {}",
        spoticord_utils::time_to_string(position / 1000),
        spoticord_utils::time_to_string(playback_info.duration() / 1000)
    ));

    CreateEmbed::new()
        .author(
            CreateEmbedAuthor::new("Currently Playing")
                .icon_url("https://spoticord.com/spotify-logo.png"),
        )
        .description(description)
        .thumbnail(playback_info.thumbnail())
        .footer(
            CreateEmbedFooter::new(owner.global_name.as_ref().unwrap_or(&owner.name))
                .icon_url(owner.face()),
        )
        .color(Colors::Info)
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
