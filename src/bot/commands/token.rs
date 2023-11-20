use crate::bot::Context;
use poise::serenity_prelude::Error;

/// Get your Spotify access token
#[poise::command(slash_command)]
pub async fn token(ctx: Context<'_>) -> Result<(), Error> {
    let token = ctx
        .data()
        .database
        .get_access_token(ctx.author().id.to_string())
        .await;

    let content = match token {
        Ok(token) => format!("Your token is: {}", token),
        Err(why) => format!("You don't have a token yet. (Real: {})", why),
    };

    ctx.send(|b| b.content(content).ephemeral(true)).await?;

    Ok(())
}
