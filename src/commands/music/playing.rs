use anyhow::Result;
use poise::CreateReply;
use serenity::all::CreateEmbed;
use spoticord_session::{manager::SessionQuery, playback_embed::UpdateBehavior};
use spoticord_utils::discord::Colors;

use crate::bot::Context;

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

    let Some(session) = manager.get_session(SessionQuery::Guild(guild)) else {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot display song details")
                        .description("I'm currently not playing any music in this server.")
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    };

    let Context::Application(context) = ctx else {
        panic!("Slash command is a prefix command?");
    };

    session
        .create_playback_embed(context.interaction, update_behavior.unwrap_or_default())
        .await?;

    Ok(())
}
