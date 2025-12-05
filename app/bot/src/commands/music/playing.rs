use anyhow::Result;
use poise::{CreateReply, serenity_prelude::CreateEmbed};

use crate::{bot::Context, discord::EmbedColor, session::playback_embed::UpdateBehavior};

/// Show details of the current song that is being played
#[poise::command(slash_command, guild_only)]
pub async fn playing(
    ctx: Context<'_>,

    #[description = "How Spoticord should update this information"] update_behavior: Option<
        UpdateBehavior,
    >,
) -> Result<()> {
    let manager = ctx.data();
    let guild = ctx.guild_id().expect("poise lied to me");

    let Some(session) = manager.get_session(guild) else {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot display song details")
                        .description("I'm currently not playing any music in this server.")
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    };

    session
        .create_playback_embed(ctx.interaction, update_behavior.unwrap_or_default())
        .await?;

    Ok(())
}
