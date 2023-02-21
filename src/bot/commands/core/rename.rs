use log::error;
use reqwest::StatusCode;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::{
    command::CommandOptionType, interaction::application_command::ApplicationCommandInteraction,
  },
  prelude::Context,
};

use crate::{
  bot::commands::{respond_message, CommandOutput},
  database::{Database, DatabaseError},
  utils::{
    self,
    embed::{EmbedBuilder, Status},
  },
};

pub const NAME: &str = "rename";

pub fn command(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    let data = ctx.data.read().await;
    let database = data.get::<Database>().expect("to contain a value");

    // Check if user exists, if not, create them
    if let Err(why) = database.get_user(command.user.id.to_string()).await {
      match why {
        DatabaseError::InvalidStatusCode(StatusCode::NOT_FOUND) => {
          if let Err(why) = database.create_user(command.user.id.to_string()).await {
            error!("Error creating user: {:?}", why);

            respond_message(
              &ctx,
              &command,
              EmbedBuilder::new()
                .description("Something went wrong while trying to rename your Spoticord device.")
                .status(Status::Error)
                .build(),
              true,
            )
            .await;

            return;
          }
        }

        _ => {
          respond_message(
            &ctx,
            &command,
            EmbedBuilder::new()
              .description("Something went wrong while trying to rename your Spoticord device.")
              .status(Status::Error)
              .build(),
            true,
          )
          .await;

          return;
        }
      }
    }

    let device_name = match command.data.options.get(0) {
      Some(option) => match option.value {
        Some(ref value) => value.as_str().expect("to be a string").to_string(),
        None => {
          respond_message(
            &ctx,
            &command,
            EmbedBuilder::new()
              .description("You need to provide a name for your Spoticord device.")
              .status(Status::Error)
              .build(),
            true,
          )
          .await;

          return;
        }
      },
      None => {
        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .description("You need to provide a name for your Spoticord device.")
            .status(Status::Error)
            .build(),
          true,
        )
        .await;

        return;
      }
    };

    if let Err(why) = database
      .update_user_device_name(command.user.id.to_string(), &device_name)
      .await
    {
      if let DatabaseError::InvalidInputBody(_) = why {
        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .description(
              "Your device name must not exceed 16 characters and be at least 1 character long.",
            )
            .status(Status::Error)
            .build(),
          true,
        )
        .await;

        return;
      }

      error!("Error updating user device name: {:?}", why);

      respond_message(
        &ctx,
        &command,
        EmbedBuilder::new()
          .description("Something went wrong while trying to rename your Spoticord device.")
          .status(Status::Error)
          .build(),
        true,
      )
      .await;

      return;
    }

    respond_message(
      &ctx,
      &command,
      EmbedBuilder::new()
        .description(format!(
          "Successfully changed the Spotify device name to **{}**",
          utils::discord::escape(device_name)
        ))
        .status(Status::Success)
        .build(),
      true,
    )
    .await;
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name(NAME)
    .description("Set a new device name that is displayed in Spotify")
    .create_option(|option| {
      option
        .name("name")
        .description("The new device name")
        .kind(CommandOptionType::String)
        .max_length(16)
        .required(true)
    })
}
