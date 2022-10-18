use log::error;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::{
    application_command::ApplicationCommandInteraction, InteractionResponseType,
  },
  prelude::Context,
  Result as SerenityResult,
};

use crate::{
  bot::commands::CommandOutput,
  database::{Database, DatabaseError},
  session::manager::SessionManager,
};

pub const NAME: &str = "unlink";

async fn respond_message(
  ctx: &Context,
  command: &ApplicationCommandInteraction,
  msg: impl Into<String>,
  ephemeral: bool,
) -> SerenityResult<()> {
  command
    .create_interaction_response(&ctx.http, |response| {
      response
        .kind(InteractionResponseType::ChannelMessageWithSource)
        .interaction_response_data(|message| message.content(msg.into()).ephemeral(ephemeral))
    })
    .await
}

fn check_msg(result: SerenityResult<()>) {
  if let Err(why) = result {
    error!("Error sending message: {:?}", why);
  }
}

pub fn run(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    let data = ctx.data.read().await;
    let database = data.get::<Database>().unwrap();
    let session_manager = data.get::<SessionManager>().unwrap();

    // Disconnect session if user has any
    if let Some(session) = session_manager.find(command.user.id).await {
      if let Err(why) = session.disconnect().await {
        error!("Error disconnecting session: {:?}", why);
      }
    }

    // Check if user exists in the first place
    if let Err(why) = database
      .delete_user_account(command.user.id.to_string())
      .await
    {
      if let DatabaseError::InvalidStatusCode(status) = why {
        if status == 404 {
          check_msg(
            respond_message(
              &ctx,
              &command,
              "You cannot unlink your Spotify account if you currently don't have a linked Spotify account.",
              true,
            )
            .await,
          );

          return;
        }
      }

      error!("Error deleting user account: {:?}", why);

      check_msg(
        respond_message(
          &ctx,
          &command,
          "An unexpected error has occured while trying to unlink your account. Please try again later.",
          true,
        )
        .await,
      );

      return;
    }

    check_msg(
      respond_message(
        &ctx,
        &command,
        "Successfully unlinked your Spotify account from Spoticord",
        true,
      )
      .await,
    );
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name(NAME)
    .description("Unlink your Spotify account from Spoticord")
}
