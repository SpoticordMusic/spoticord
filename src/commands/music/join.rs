use std::time::Duration;

use anyhow::Result;
use log::error;
use poise::CreateReply;
use serenity::all::{
    Channel, ChannelId, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, UserId,
};
use spoticord_database::error::DatabaseError;
use spoticord_session::manager::SessionQuery;
use spoticord_utils::discord::Colors;

use crate::bot::Context;

/// Join the current voice channel
#[poise::command(slash_command, guild_only)]
pub async fn join(ctx: Context<'_>) -> Result<()> {
    let guild = ctx.guild_id().expect("poise lied to me");
    let manager = ctx.data();

    let Some(guild) = guild
        .to_guild_cached(ctx.serenity_context())
        .map(|guild| guild.clone())
    else {
        error!("Unable to fetch guild from cache, how did we get here?");

        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("An error occured")
                        .description("This server hasn't been cached yet?")
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    };

    let Some(channel) = guild
        .voice_states
        .get(&ctx.author().id)
        .and_then(|state| state.channel_id)
    else {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot join voice channel")
                        .description("You need to connect to a voice channel before running /join")
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    };

    if !has_voice_permissions(ctx, channel).await? {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot join voice channel")
                        .description(
                            "The voice channel you are in is not available.
                                    I might not have the permissions to see this channel.",
                        )
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    if !has_text_permissions(ctx, ctx.channel_id()).await? {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Cannot join voice channel")
                        .description(
                            "I do not have permissions to send messages / links in this text channel.",
                        )
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    // Check whether the user has linked their Spotify account
    if let Err(DatabaseError::NotFound) = manager
        .database()
        .get_account(ctx.author().id.to_string())
        .await
    {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("No Spotify account")
                        .description(
                            "You need to link your Spotify account to Spoticord before being able to use it.\nUse the `/link` command to link your account.",
                        )
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    let mut session_opt = manager.get_session(SessionQuery::Guild(guild.id));

    // Check if this server already has a session active
    if let Some(session) = &session_opt {
        if session.active().await? {
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Spoticord is busy")
                            .description("Spoticord is already being used in this server.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;

            return Ok(());
        }
    }

    // Prevent the user from using Spoticord simultaneously in multiple servers
    if let Some(session) = manager.get_session(SessionQuery::Owner(ctx.author().id)) {
        let server_name = session.guild().to_partial_guild(&ctx).await?.name;

        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("You are already using Spoticord")
                        .description(format!(
                            "You are already using Spoticord in `{}`\n\n\
                            Stop playing in that server first before starting a new session.",
                            spoticord_utils::discord::escape(server_name)
                        ))
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    ctx.defer().await?;

    if let Some(session) = &session_opt {
        if session.voice_channel() != channel {
            session.disconnect().await;
            session_opt = None;

            // Give serenity/songbird some time to register the disconnect
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    if let Some(session) = session_opt {
        if let Err(why) = session.reactivate(ctx.author().id).await {
            error!("Failed to reactivate session: {why}");

            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Failed to reactivate session")
                            .description(
                                "An error occured whilst trying to reactivate the session. Please try again.",
                            )
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;

            return Ok(());
        }
    } else if let Err(why) = manager
        .create_session(
            ctx.serenity_context(),
            guild.id,
            channel,
            ctx.channel_id(),
            ctx.author().id,
        )
        .await
    {
        error!("Failed to create session: {why}");

        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Failed to create session")
                        .description(
                            "An error occured whilst trying to create a session. Please try again.",
                        )
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .author(
                    CreateEmbedAuthor::new("Connected to voice channel")
                        .icon_url("https://spoticord.com/speaker.png"),
                )
                .description(format!("Come listen along in <#{}>", channel))
                .footer(CreateEmbedFooter::new(
                    "You must manually select your device in Spotify",
                ))
                .color(Colors::Info),
        ),
    )
    .await?;

    Ok(())
}

async fn has_voice_permissions(ctx: Context<'_>, channel: ChannelId) -> Result<bool> {
    let me: UserId = ctx.cache().current_user().id;

    let Ok(Channel::Guild(channel)) = channel.to_channel(ctx).await else {
        return Ok(false);
    };

    let Ok(permissions) = channel.permissions_for_user(ctx, me) else {
        return Ok(false);
    };

    Ok(permissions.view_channel() && permissions.connect() && permissions.speak())
}

async fn has_text_permissions(ctx: Context<'_>, channel: ChannelId) -> Result<bool> {
    let me: UserId = ctx.cache().current_user().id;

    let Ok(Channel::Guild(channel)) = channel.to_channel(ctx).await else {
        return Ok(false);
    };

    let Ok(permissions) = channel.permissions_for_user(ctx, me) else {
        return Ok(false);
    };

    Ok(permissions.view_channel() && permissions.send_messages() && permissions.embed_links())
}
