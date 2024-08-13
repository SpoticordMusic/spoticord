use anyhow::Result;
use poise::CreateReply;
use spoticord_database::error::DatabaseError;

use crate::bot::Context;

/// Retrieve the Spotify access token. For debugging purposes.
#[poise::command(slash_command)]
pub async fn token(ctx: Context<'_>) -> Result<()> {
    let token = ctx
        .data()
        .database()
        .get_access_token(ctx.author().id.to_string())
        .await;

    let content = match token {
        Ok(token) => format!("Your token is:\n```\n{token}\n```"),
        Err(DatabaseError::NotFound) => {
            "You must authenticate first before requesting a token".to_string()
        }
        Err(why) => format!("Failed to retrieve access token: {why}"),
    };

    ctx.send(CreateReply::default().content(content).ephemeral(true))
        .await?;

    Ok(())
}
