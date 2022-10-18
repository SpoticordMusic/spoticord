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
  session::manager::{SessionCreateError, SessionManager},
};

pub const NAME: &str = "join";

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
    let guild = ctx.cache.guild(command.guild_id.unwrap()).unwrap();

    // Get the voice channel id of the calling user
    let channel_id = match guild
      .voice_states
      .get(&command.user.id)
      .and_then(|state| state.channel_id)
    {
      Some(channel_id) => channel_id,
      None => {
        check_msg(
          respond_message(
            &ctx,
            &command,
            "You need to connect to a voice channel",
            true,
          )
          .await,
        );

        return;
      }
    };

    let data = ctx.data.read().await;
    let mut session_manager = data.get::<SessionManager>().unwrap().clone();

    // Check if another session is already active in this server
    if let Some(session) = session_manager.get_session(guild.id).await {
      let msg = if session.get_owner() == command.user.id {
        "You are already playing music in this server"
      } else {
        "Someone else is already playing music in this server"
      };

      check_msg(respond_message(&ctx, &command, msg, true).await);

      return;
    };

    // Prevent duplicate Spotify sessions
    if let Some(session) = session_manager.find(command.user.id).await {
      check_msg(
        respond_message(
          &ctx,
          &command,
          format!(
            "You are already playing music in another server ({}).\nStop playing in that server first before joining this one.",
            ctx.cache.guild(session.get_guild_id()).unwrap().name
          ),
          true,
        )
        .await,
      );

      return;
    }

    // Create the session, and handle potential errors
    if let Err(why) = session_manager
      .create_session(&ctx, guild.id, channel_id, command.user.id)
      .await
    {
      // Need to link first
      if let SessionCreateError::NoSpotifyError = why {
        check_msg(
          respond_message(
            &ctx,
            &command,
            "You need to link your Spotify account. Use `/link` or go to https://account.spoticord.com/ to get started.",
            true,
          )
          .await,
        );

        return;
      }

      // Any other error
      check_msg(
        respond_message(
          &ctx,
          &command,
          "An error occurred while joining the channel. Please try again later.",
          true,
        )
        .await,
      );

      return;
    };

    check_msg(respond_message(&ctx, &command, "Joined the voice channel.", false).await);
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name(NAME)
    .description("Request the bot to join the current voice channel")
}
