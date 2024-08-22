use anyhow::Error;
use poise::CreateReply;
use serenity::all::CreateEmbed;
use spoticord_session::manager::SessionQuery;
use spoticord_utils::discord::Colors;

use crate::bot::Context;

#[poise::command(slash_command, guild_only)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let manager = ctx.data();
    let guild = ctx.guild_id().expect("poise lied to me");

    let Some(session) = manager.get_session(SessionQuery::Guild(guild)) else {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot stop playback")
                        .description("I'm currently not connected to any voice channel.")
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    };

    if session.active().await? && session.owner().await? != ctx.author().id {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot stop playback")
                        .description("Only the host may stop playback.")
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    session.shutdown_player().await;

    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .title("Stopped playback")
                .description("I have stopped playing for now. To resume playback, please run the /join command again.")
                .color(Colors::Info),
        ),
    )
    .await?;

    Ok(())
}
