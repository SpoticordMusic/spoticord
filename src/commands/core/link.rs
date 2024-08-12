use std::fmt::Display;

use anyhow::Result;
use log::error;
use poise::{serenity_prelude::Error, CreateReply};
use serenity::all::{
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
use spoticord_database::error::DatabaseResultExt;
use spoticord_utils::discord::Colors;

use crate::bot::{Context, FrameworkError};

/// Link your Spotify account to Spoticord
#[poise::command(slash_command, on_error = on_error)]
pub async fn link(ctx: Context<'_>) -> Result<()> {
    let db = ctx.data().database();
    let user_id = ctx.author().id.to_string();

    if db.get_account(&user_id).await.optional()?.is_some() {
        ctx.send(
                CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("Spotify account already linked")
                        .description("You already have a Spotify account linked.")
                        .footer(CreateEmbedFooter::new(
                            "If you are trying to re-link your account then please use /unlink first.",
                        )).color(Colors::Info),
                ).ephemeral(true),
            )
            .await?;

        return Ok(());
    };

    if let Some(request) = db.get_request(&user_id).await.optional()? {
        if !request.expired() {
            send_link_message(ctx, request.token).await?;
            return Ok(());
        }
    }

    let user = db.get_or_create_user(&user_id).await?;
    let request = db.create_request(user.id).await?;

    send_link_message(ctx, request.token).await?;

    Ok(())
}

async fn send_link_message(ctx: Context<'_>, token: impl Display) -> Result<(), Error> {
    let link = format!("{}/{token}", spoticord_config::link_url());

    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .author(
                        CreateEmbedAuthor::new("Link your Spotify account")
                            .url(&link)
                            .icon_url("https://spoticord.com/spotify-logo.png"),
                    )
                    .description("Click on the button below to start linking your Spotify account.")
                    .color(Colors::Info),
            )
            .components(vec![CreateActionRow::Buttons(vec![
                CreateButton::new_link(&link).label("Link your account"),
            ])])
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

async fn on_error(error: FrameworkError<'_>) {
    if let FrameworkError::Command { error, ctx, .. } = error {
        error!("An error occured during linking of new account: {error}");

        _ = ctx
            .send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .description("An error occured whilst trying to link your account.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await;
    } else {
        error!("{error}")
    }
}
