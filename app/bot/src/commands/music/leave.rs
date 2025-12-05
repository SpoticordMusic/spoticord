use anyhow::Result;
use poise::{
    CreateReply,
    serenity_prelude::{Channel, CreateEmbed},
};

use crate::{bot::Context, discord::EmbedColor};

/// Disconnect the bot from the current voice channel
#[poise::command(slash_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<()> {
    let manager = ctx.data();
    let guild_id = ctx.guild_id().expect("poise lied to me");
    let member = ctx.author_member().await.expect("poise lied to me");

    let Some(session) = manager.get_session(guild_id) else {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot disconnect bot")
                        .description("I'm currently not connected to any voice channel.")
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    };

    // Users with the "Administrator", "Manage Channels" or "Move Members" permissions can also perform this command,
    // even when they're not the session owner
    let has_admin = 'a: {
        let Ok(channel_id) = session.get_voice_channel_id().await else {
            break 'a false;
        };

        let Ok(Channel::Guild(channel)) = channel_id.to_channel(&ctx).await else {
            break 'a false;
        };

        let Some(guild) = guild_id.to_guild_cached(&ctx) else {
            break 'a false;
        };

        let permissions = guild.user_permissions_in(&channel, &member);

        permissions.move_members() || permissions.manage_channels() || permissions.administrator()
    };

    if session.active().await? && session.get_owner_id().await? != ctx.author().id && !has_admin {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot disconnect bot")
                        .description("Only the host or server admins may disconnect the bot.")
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    session.shutdown().await;

    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .title("Goodbye, for now!")
                .description("I have left the voice channel, goodbye for now.")
                .color(EmbedColor::Info),
        ),
    )
    .await?;

    Ok(())
}
