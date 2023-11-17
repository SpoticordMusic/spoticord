use crate::{bot::Context, consts::VERSION, utils::embed::Color};
use poise::serenity_prelude::Error;

const IMAGE_URL: &str = "https://cdn.discordapp.com/avatars/389786424142200835/6bfe3840b0aa6a1baf432bb251b70c9f.webp?size=128";

/// Shows the current running version of Spoticord
#[poise::command(slash_command)]
pub async fn version(ctx: Context<'_>) -> Result<(), Error> {
  // Had to pull this from the builder as rustfmt refused to format the file
  let description = format!("Current version: {}\n\nSpoticord is open source, check out [our GitHub](https://github.com/SpoticordMusic)", VERSION);

  ctx
    .send(|b| {
      b.embed(|e| {
        e.title("Spoticord Version")
          .author(|a| {
            a.name("Maintained by: DaXcess (@rodabafilms)")
              .url("https://github.com/DaXcess")
              .icon_url(IMAGE_URL)
          })
          .description(description)
          .color(Color::Info)
      })
    })
    .await?;

  Ok(())
}
