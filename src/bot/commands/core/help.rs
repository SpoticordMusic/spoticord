use poise::serenity_prelude::Error;

use crate::{bot::Context, utils::embed::Color};

/// Shows the help message
#[poise::command(slash_command)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
  ctx
    .send(|b| {
      b.embed(|f| {
        f.title("Spoticord Help")
          .author(|a| a.icon_url("https://spoticord.com/logo-standard.webp"))
          .description("**Welcome to Spoticord**
          It seems you have requested some help. Not to worry, we can help you out.\n
          **Not sure how the bot works?**
          **[Click here](https://spoticord.com/#how-to)** for a quick overview about how to set up Spoticord and how to use it.\n
          **Which commands are there?**
          You can find all **[the commands](https://spoticord.com/#commands)** on the website. You may also just type `/` in Discord and see which commands are available there.\n
          **Need more help?**
          If you still need some help, whether you are having issues with the bot or you just want to give us some feedback, you can join our **[Discord server](https://discord.gg/wRCyhVqBZ5)**.")
          .color(Color::Info)
      })
    })
    .await?;

  Ok(())
}
