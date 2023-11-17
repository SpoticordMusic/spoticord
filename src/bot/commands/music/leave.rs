use poise::serenity_prelude::Error;

use crate::{bot::Context, utils::embed::Color};

/// Request the bot to leave the current voice channel
#[poise::command(slash_command)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
  let sm = &ctx.data().session_manager;

  let Some(guild) = ctx.guild() else {
    ctx
      .send(|b| {
        b.embed(|e| {
          e.description("You can only execute this command inside of a server")
            .color(Color::Error)
        })
      })
      .await?;

    return Ok(());
  };

  let Some(session) = sm.get_session(&guild.id).await else {
    ctx
      .send(|b| {
        b.embed(|f| {
          f.title("Cannot disconnect bot")
            .description("I'm currently not connected to any voice channel")
            .color(Color::Error)
        })
        .ephemeral(true)
      })
      .await?;
    return Ok(());
  };

  if let Some(owner) = session.owner().await {
    if owner != ctx.author().id {
      ctx
        .send(|b| {
          b.embed(|f| {
            f.description("You are not the one who summoned me")
              .color(Color::Error)
          })
          .ephemeral(true)
        })
        .await?;
    }
  }

  session.disconnect().await;

  ctx
    .send(|b| {
      b.embed(|f| {
        f.description("I have left the voice channel, goodbye for now")
          .color(Color::Info)
      })
    })
    .await?;

  Ok(())
}
