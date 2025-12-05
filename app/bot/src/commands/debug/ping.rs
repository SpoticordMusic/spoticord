use anyhow::Result;
use log::info;
use poise::CreateReply;

use crate::bot::Context;

/// Very simple ping command
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<()> {
    info!("Pong");

    ctx.send(CreateReply::default().content("Pong!").reply(true))
        .await?;

    Ok(())
}
