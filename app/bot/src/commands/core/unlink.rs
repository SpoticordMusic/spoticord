use anyhow::Result;
use log::error;
use poise::{
    CreateReply,
    serenity_prelude::{CreateEmbed, CreateEmbedFooter},
};

use crate::{
    bot::{Context, FrameworkError},
    discord::EmbedColor,
};

/// Unlink your Spotify account from Spoticord
#[poise::command(slash_command, on_error = on_error)]
pub async fn unlink(
    ctx: Context<'_>,

    #[description = "Also delete Discord account information"] user_data: Option<bool>,
) -> Result<()> {
    let manager = ctx.data();
    let db = manager.database();
    let user_id = ctx.author().id.to_string();

    // Disconnect sessions if user has any
    for guild in manager.get_owner_guilds(ctx.author().id) {
        let Some(session) = manager.get_session(guild) else {
            continue;
        };

        // double check to make sure
        if session.get_owner_id().await? != ctx.author().id {
            continue;
        }

        session.shutdown().await;
    }

    let deleted_account = db.delete_account(&user_id).await? != 0;
    let deleted_user = if user_data.unwrap_or(false) {
        db.delete_user(&user_id).await? != 0
    } else {
        false
    };

    if !deleted_account && !deleted_user {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("No Spotify account linked")
                        .description(
                            "You cannot unlink your Spotify account if you haven't linked one.",
                        )
                        .footer(CreateEmbedFooter::new(
                            "You can use /link to link a new Spotify account.",
                        ))
                        .color(EmbedColor::Error),
                )
                .ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("Account unlinked")
                    .description("You have unlinked your Spotify account from Spoticord.")
                    .footer(CreateEmbedFooter::new(
                        "Changed your mind? You can use /link to link a new Spotify account.",
                    ))
                    .color(EmbedColor::Success),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

async fn on_error(error: FrameworkError<'_>) {
    if let FrameworkError::Command { error, ctx, .. } = error {
        error!("failed to unlink Spotify account: {error}");

        _ = ctx
            .send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .description("An error occured whilst trying to unlink your account.")
                            .color(EmbedColor::Error),
                    )
                    .ephemeral(true),
            )
            .await;
    } else {
        error!("{error}")
    }
}
