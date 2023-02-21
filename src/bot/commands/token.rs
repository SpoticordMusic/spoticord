use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::{
    application_command::ApplicationCommandInteraction, InteractionResponseType,
  },
  prelude::Context,
};

use crate::database::Database;

use super::CommandOutput;

pub const NAME: &str = "token";

pub fn command(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    let data = ctx.data.read().await;
    let db = data.get::<Database>().expect("to contain a value");

    let token = db.get_access_token(command.user.id.to_string()).await;

    let content = match token {
      Ok(token) => format!("Your token is: {}", token),
      Err(why) => format!("You don't have a token yet. (Real: {})", why),
    };

    command
      .create_interaction_response(&ctx.http, |response| {
        response
          .kind(InteractionResponseType::ChannelMessageWithSource)
          .interaction_response_data(|message| message.content(content).ephemeral(true))
      })
      .await
      .ok();
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name("token")
    .description("Get your Spotify access token")
}
