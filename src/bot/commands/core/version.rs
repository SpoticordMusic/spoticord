use log::error;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::{
    application_command::ApplicationCommandInteraction, InteractionResponseType,
  },
  prelude::Context,
};

use crate::{bot::commands::CommandOutput, consts::VERSION, utils::embed::Status};

pub const NAME: &str = "version";

pub fn command(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    if let Err(why) = command
      .create_interaction_response(&ctx.http, |response| {
        response
          .kind(InteractionResponseType::ChannelMessageWithSource)
          .interaction_response_data(|message| {
            message.embed(|embed| {
              embed
                .title("Spoticord Version")
                .author(|author| {
                  author
                    .name("Maintained by: RoDaBaFilms")
                    .url("https://rodabafilms.com/")
                    .icon_url("https://rodabafilms.com/logo_2021_nobg.png")
                })
                .description(format!("Current version: {}\n\nSpoticord is open source, check out [our GitHub](https://github.com/SpoticordMusic)", VERSION))
                .color(Status::Info as u64)
            })
          })
      })
      .await
    {
      error!("Error sending message: {:?}", why);
    }
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name(NAME)
    .description("Shows the current running version of Spoticord")
}
