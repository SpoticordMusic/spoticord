use log::error;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::application_command::ApplicationCommandInteraction,
  prelude::Context,
};

use crate::{
  bot::commands::{respond_message, CommandOutput},
  database::Database,
  utils::embed::{EmbedBuilder, Status},
};

pub const NAME: &str = "link";

pub fn run(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    let data = ctx.data.read().await;
    let database = data.get::<Database>().unwrap();

    if let Ok(_) = database.get_user_account(command.user.id.to_string()).await {
      respond_message(
        &ctx,
        &command,
        EmbedBuilder::new()
          .description("You have already linked your Spotify account.")
          .status(Status::Error)
          .build(),
        true,
      )
      .await;

      return;
    }

    if let Ok(request) = database.get_user_request(command.user.id.to_string()).await {
      let base = std::env::var("SPOTICORD_ACCOUNTS_URL").unwrap();
      let link = format!("{}/spotify/{}", base, request.token);

      respond_message(
        &ctx,
        &command,
        EmbedBuilder::new()
          .title("Link your Spotify account")
          .title_url(&link)
          .icon_url("https://spoticord.com/img/spotify-logo.png")
          .description(format!(
            "Go to [this link]({}) to connect your Spotify account.",
            link
          ))
          .status(Status::Info)
          .build(),
        true,
      )
      .await;

      return;
    }

    match database
      .create_user_request(command.user.id.to_string())
      .await
    {
      Ok(request) => {
        let base = std::env::var("SPOTICORD_ACCOUNTS_URL").unwrap();
        let link = format!("{}/spotify/{}", base, request.token);

        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .title("Link your Spotify account")
            .title_url(&link)
            .icon_url("https://spoticord.com/img/spotify-logo.png")
            .description(format!(
              "Go to [this link]({}) to connect your Spotify account.",
              link
            ))
            .status(Status::Info)
            .build(),
          true,
        )
        .await;

        return;
      }
      Err(why) => {
        error!("Error creating user request: {:?}", why);

        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .description("An error occurred while serving your request. Please try again later.")
            .status(Status::Error)
            .build(),
          true,
        )
        .await;

        return;
      }
    };
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name(NAME)
    .description("Link your Spotify account to Spoticord")
}
