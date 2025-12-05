use anyhow::Result;
use poise::{
    CreateReply,
    serenity_prelude::{CreateEmbed, CreateEmbedAuthor},
};

use crate::{bot::Context, discord::EmbedColor};

const HELP_MESSAGE: &str = include_str!("HELP.md");

/// Displays the help message
#[poise::command(slash_command)]
pub async fn help(ctx: Context<'_>) -> Result<()> {
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .author(
                    CreateEmbedAuthor::new("Spoticord Help")
                        .icon_url("https://spoticord.com/logo-standard.webp"),
                )
                .description(HELP_MESSAGE)
                .color(EmbedColor::Info),
        ),
    )
    .await?;

    Ok(())
}
