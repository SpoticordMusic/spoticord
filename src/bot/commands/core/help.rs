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

pub fn run(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    respond_message(
      &ctx,
      &command,
      EmbedBuilder::new()
        .title("Spoticord Help")
        .icon_url("https://spoticord.com/img/logo-standard.webp")
        .description(format!("Click **[here](https://spoticord.com/commands)** for a list of commands.\n{}",
        "If you need help setting Spoticord up you can check out the **[Documentation](https://spoticord.com/documentation)** page on the Spoticord website.\n\n"))
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
