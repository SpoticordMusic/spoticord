use log::error;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::application_command::ApplicationCommandInteraction,
  prelude::Context,
};

use crate::{
  bot::commands::{respond_message, CommandOutput},
  database::{Database, DatabaseError},
  session::manager::SessionManager,
  utils::embed::{EmbedBuilder, Status},
};

pub const NAME: &str = "unlink";

pub fn run(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    let data = ctx.data.read().await;
    let database = data.get::<Database>().unwrap();
    let session_manager = data.get::<SessionManager>().unwrap();

    // Disconnect session if user has any
    if let Some(session) = session_manager.find(command.user.id).await {
      session.disconnect().await;
    }

    // Check if user exists in the first place
    if let Err(why) = database
      .delete_user_account(command.user.id.to_string())
      .await
    {
      if let DatabaseError::InvalidStatusCode(status) = why {
        if status == 404 {
          respond_message(
            &ctx,
            &command,
            EmbedBuilder::new()
              .description("You cannot unlink your Spotify account if you haven't linked one.")
              .status(Status::Error)
              .build(),
            true,
          )
          .await;

          return;
        }
      }

      error!("Error deleting user account: {:?}", why);

      respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
                .description("An unexpected error has occured while trying to unlink your account. Please try again later.")
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
        .description("Successfully unlinked your Spotify account from Spoticord")
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
    .description("Unlink your Spotify account from Spoticord")
}
