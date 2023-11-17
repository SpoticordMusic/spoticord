use crate::{
  bot::Context,
  utils::{self, embed::Color},
};
use log::error;
use poise::serenity_prelude::Error;

/// Set a new device name that is displayed in Spotify
#[poise::command(slash_command)]
pub async fn rename(
  ctx: Context<'_>,

  #[description = "The new device name"]
  #[max_length = 16]
  #[min_length = 1]
  name: String,
) -> Result<(), Error> {
  let db = &ctx.data().database;

  let user = match db.get_or_create_user(ctx.author().id.to_string()).await {
    Ok(user) => user,
    Err(why) => {
      error!("Error fetching user: {why:?}");

      ctx
        .send(|b| {
          b.embed(|e| {
            e.description("Something went wrong while trying to rename your Spoticord device.")
              .color(Color::Error)
          })
          .ephemeral(true)
        })
        .await?;

      return Ok(());
    }
  };

  if let Err(why) = db.update_user_device_name(user.id, &name).await {
    error!("Error updating user device name: {why:?}");

    ctx
      .send(|b| {
        b.embed(|e| {
          e.description("Something went wrong while trying to rename your Spoticord device.")
            .color(Color::Error)
        })
        .ephemeral(true)
      })
      .await?;

    return Ok(());
  }

  ctx
    .send(|b| {
      b.embed(|e| {
        e.description(format!(
          "Successfully changed the Spotify device name to **{}**",
          utils::discord::escape(name)
        ))
        .color(Color::Success)
      })
      .ephemeral(true)
    })
    .await?;

  Ok(())
}
