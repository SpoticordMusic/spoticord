use crate::{bot::Context, database::DatabaseError, utils::embed::Color};
use log::error;
use poise::serenity_prelude::Error;
use reqwest::StatusCode;

/// Unlink your Spotify account from Spoticord
#[poise::command(slash_command)]
pub async fn unlink(ctx: Context<'_>) -> Result<(), Error> {
    let db = &ctx.data().database;
    let sm = &ctx.data().session_manager;

    // Disconnect session if user has any
    if let Some(session) = sm.find(ctx.author().id).await {
        session.disconnect().await;
    }

    // Check if user exists in the first place
    if let Err(why) = db.delete_user_account(ctx.author().id.to_string()).await {
        match why {
            DatabaseError::InvalidStatusCode(StatusCode::NOT_FOUND) => {
                ctx.send(|b| {
                    b.embed(|e| {
                        e.description(
                            "You cannot unlink your Spotify account if you haven't linked one.",
                        )
                        .color(Color::Error)
                    })
                    .ephemeral(true)
                })
                .await?;
            }

            _ => {
                error!("Error deleting user account: {why:?}");

                ctx
          .send(|b| {
            b.embed(|e| {
              e.description("An unexpected error has occured while trying to unlink your account. Please try again later.")
                .color(Color::Error)
            })
            .ephemeral(true)
          })
          .await?;
            }
        }

        return Ok(());
    }

    ctx.send(|b| {
        b.embed(|e| {
            e.description("Successfully unlinked your Spotify account from Spoticord")
                .color(Color::Success)
        })
        .ephemeral(true)
    })
    .await?;

    Ok(())
}
