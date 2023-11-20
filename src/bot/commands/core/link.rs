use crate::{bot::Context, consts::SPOTICORD_ACCOUNTS_URL, utils::embed::Color};
use log::error;
use poise::serenity_prelude::Error;

/// Link your Spotify account to Spoticord
#[poise::command(slash_command)]
pub async fn link(ctx: Context<'_>) -> Result<(), Error> {
    let db = &ctx.data().database;

    if db
        .get_user_account(ctx.author().id.to_string())
        .await
        .is_ok()
    {
        ctx.send(|b| {
            b.embed(|e| {
                e.description("You have already linked your Spotify account")
                    .color(Color::Error)
            })
            .ephemeral(true)
        })
        .await?;

        return Ok(());
    }

    macro_rules! send_link_message {
        ($token:expr) => {
            let link = format!("{}/spotify/{}", SPOTICORD_ACCOUNTS_URL.as_str(), $token);

            ctx.send(|b| {
                b.embed(|e| {
                    e.author(|a| {
                        a.name("Link your Spotify account")
                            .url(&link)
                            .icon_url("https://spoticord.com/spotify-logo.png")
                    })
                    .description(format!(
                        "Go to [this link]({}) to connect your Spotify account.",
                        link
                    ))
                    .color(Color::Info)
                })
                .ephemeral(true)
            })
            .await?;
        };
    }

    macro_rules! send_error_message {
        () => {
            ctx.send(|b| {
                b.embed(|e| {
                    e.description("Something went wrong while trying to link your Spotify account.")
                        .color(Color::Error)
                })
                .ephemeral(true)
            })
            .await?;

            return Ok(());
        };
    }

    if let Ok(request) = db.get_user_request(ctx.author().id.to_string()).await {
        send_link_message!(request.token);

        return Ok(());
    }

    // Check if user exists, if not, create them
    let user = match db.get_or_create_user(ctx.author().id.to_string()).await {
        Ok(user) => user,
        Err(why) => {
            error!("Error fetching user: {why:?}");

            send_error_message!();
        }
    };

    match db.create_user_request(user.id).await {
        Ok(request) => {
            send_link_message!(request.token);
        }

        Err(why) => {
            error!("Error creating user request: {why:?}");

            send_error_message!();
        }
    }

    Ok(())
}
