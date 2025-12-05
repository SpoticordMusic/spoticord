use std::time::Duration;

use anyhow::Result;
use log::error;
use poise::{
    CreateReply,
    serenity_prelude::{
        Channel, ChannelId, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, Error, ModelError,
    },
};
use spoticord_database::error::DatabaseError;

use crate::{
    bot::Context,
    discord::EmbedColor,
    session::{
        error::SessionError,
        manager::{self},
    },
};

/// Join the current voice channel
#[poise::command(slash_command, guild_only)]
pub async fn join(ctx: Context<'_>) -> Result<()> {
    let guild = ctx.guild_id().expect("poise lied to me");
    let manager = ctx.data().clone();

    let Some(guild) = guild
        .to_guild_cached(ctx.serenity_context())
        // Need to clone since we can't hold a ref across an await
        .map(|guild| guild.clone())
    else {
        error!("Unable to fetch guild from cache. Serenity, where is my guild?");

        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("An error occured")
                        .description("This server hasn't been discovered by the bot yet (somehow). I blame Serenity, but you can just blame me.")
                        .color(EmbedColor::Error),
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
                        .color(EmbedColor::Error),
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
                            "I do not have the appropriate permissions to join this voice channel.",
                        )
                        .color(EmbedColor::Error),
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
                        .description("I do not have permissions to send messages / links in this text channel.")
                        .color(EmbedColor::Error),
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
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    let mut session_opt = manager.get_session(guild.id);

    // Check if this server already has an active session
    if let Some(session) = &session_opt
        && session.active().await?
    {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Spoticord is already in use")
                        .description(
                            "Spoticord is already being used by somebody else in this server.",
                        )
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    // In contrary to previous versions of Spoticord, we actually allow people to use multiple Spoticord instances at the same time.
    // That means we don't have to check if the current user already has an active session.

    // Tell Discord that we might need some extra time to respond
    ctx.defer().await?;

    if let Some(session) = &session_opt
        && session.get_voice_channel_id().await? != channel
    {
        session.shutdown().await;
        session_opt = None;

        // Give serenity/songbird some time to register the disconnect
        tokio::time::sleep(Duration::from_secs(1)).await;
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
                            .color(EmbedColor::Error),
                    )
                    .ephemeral(true),
            )
            .await?;

            return Ok(());
        }
    } else if let Err(why) = manager::create_session(
        ctx.serenity_context().clone(),
        manager,
        guild.id,
        channel,
        ctx.channel_id(),
        ctx.author().id,
    )
    .await
    {
        error!("Failed to create session: {why}");

        let description = if matches!(
            why,
            SessionError::Spotify(spoticord_spotify::Error::RefreshTokenFailure)
        ) {
            "Unable to authenticate with Spotify. Did you change your password?\n\nThe broken credentials used have been deleted.\n\nYou might need to relink your account using `/link`."
        } else {
            "An error occured whilst trying to create a session. Please try again."
        };

        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("Failed to create session")
                        .description(description)
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    // Session has been created/reactivated at this point, so we're done here.

    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .author(
                    CreateEmbedAuthor::new("Connected to voice channel")
                        .icon_url("https://spoticord.com/speaker.png"),
                )
                .description(format!("Come listen along in <#{channel}>"))
                .footer(CreateEmbedFooter::new(
                    "You must manually select your device in Spotify",
                ))
                .color(EmbedColor::Info),
        ),
    )
    .await?;

    Ok(())
}

async fn has_voice_permissions(ctx: Context<'_>, channel: ChannelId) -> Result<bool> {
    let Ok(Channel::Guild(channel)) = channel.to_channel(ctx).await else {
        return Ok(false);
    };

    let guild = ctx.guild().ok_or(Error::Model(ModelError::GuildNotFound))?;
    let member = guild
        .members
        .get(&ctx.cache().current_user().id)
        .ok_or(Error::Model(ModelError::MemberNotFound))?;
    let permissions = guild.user_permissions_in(&channel, member);

    Ok(permissions.view_channel() && permissions.connect() && permissions.speak())
}

async fn has_text_permissions(ctx: Context<'_>, channel: ChannelId) -> Result<bool> {
    let Ok(Channel::Guild(channel)) = channel.to_channel(ctx).await else {
        return Ok(false);
    };

    let guild = ctx.guild().ok_or(Error::Model(ModelError::GuildNotFound))?;
    let member = guild
        .members
        .get(&ctx.cache().current_user().id)
        .ok_or(Error::Model(ModelError::MemberNotFound))?;
    let permissions = guild.user_permissions_in(&channel, member);

    Ok(permissions.view_channel() && permissions.send_messages() && permissions.embed_links())
}
