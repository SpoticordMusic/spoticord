use log::error;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::{
    application_command::ApplicationCommandInteraction, InteractionResponseType,
  },
  prelude::Context,
  Result as SerenityResult,
};

use crate::{bot::commands::CommandOutput, database::Database};

pub const NAME: &str = "link";

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

    if let Ok(_) = database.get_user_account(command.user.id.to_string()).await {
      check_msg(
        respond_message(
          &ctx,
          &command,
          "You have already linked your Spotify account.",
          true,
        )
        .await,
      );

      return;
    }

    if let Ok(request) = database.get_user_request(command.user.id.to_string()).await {
      let base = std::env::var("SPOTICORD_ACCOUNTS_URL").unwrap();
      let link = format!("{}/spotify/{}", base, request.token);

      check_msg(
        respond_message(
          &ctx,
          &command,
          format!("Go to the following URL to link your account:\n{}", link),
          true,
        )
        .await,
      );

      return;
    }

    match database
      .create_user_request(command.user.id.to_string())
      .await
    {
      Ok(request) => {
        let base = std::env::var("SPOTICORD_ACCOUNTS_URL").unwrap();
        let link = format!("{}/spotify/{}", base, request.token);

        check_msg(
          respond_message(
            &ctx,
            &command,
            format!("Go to the following URL to link your account:\n{}", link),
            true,
          )
          .await,
        );

        return;
      }
      Err(why) => {
        error!("Error creating user request: {:?}", why);

        check_msg(
          respond_message(
            &ctx,
            &command,
            "An error occurred while serving your request. Please try again later.",
            true,
          )
          .await,
        );

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
