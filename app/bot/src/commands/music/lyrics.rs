use anyhow::Result;
use poise::{CreateReply, serenity_prelude::CreateEmbed};

use crate::{bot::Context, discord::EmbedColor};

/// Show the lyrics of the current song that is being played
#[poise::command(slash_command, guild_only)]
pub async fn lyrics(ctx: Context<'_>) -> Result<()> {
    let manager = ctx.data();
    let guild = ctx.guild_id().expect("poise lied to me");

    let Some(session) = manager.get_session(guild) else {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot get lyrics")
                        .description("I'm currently not playing any music in this server.")
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    };

    session.create_lyrics_embed(ctx.interaction).await?;

    Ok(())
}
