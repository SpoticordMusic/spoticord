use crate::bot::Context;
use log::info;
use poise::serenity_prelude::Error;

/// Check if the bot is alive
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
  info!("Pong!");
  ctx.send(|reply| reply.content("Pong!").reply(true)).await?;

  Ok(())
}
