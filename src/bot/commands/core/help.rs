use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::application_command::ApplicationCommandInteraction,
  prelude::Context,
};

use crate::{
  bot::commands::{respond_message, CommandOutput},
  utils::embed::{EmbedBuilder, Status},
};

pub const NAME: &str = "help";

pub fn command(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    respond_message(
      &ctx,
      &command,
      EmbedBuilder::new()
        .title("Spoticord Help")
        .icon_url("https://spoticord.com/logo-standard.webp")
        .description("**Welcome to Spoticord**
         It seems you have requested some help. Not to worry, we can help you out.\n
         **Not sure how the bot works?**
         **[Click here](https://spoticord.com/#how-to)** for a quick overview about how to set up Spoticord and how to use it.\n
         **Which commands are there?**
         You can find all **[the commands](https://spoticord.com/#commands)** on the website. You may also just type `/` in Discord and see which commands are available there.\n
         **Need more help?**
         If you still need some help, whether you are having issues with the bot or you just want to give us some feedback, you can join our **[Discord server](https://discord.gg/wRCyhVqBZ5)**.".to_string())
        .status(Status::Info)
        .build(),
      false,
    )
    .await;
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command.name(NAME).description("Shows the help message")
}
