use log::trace;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::application_command::ApplicationCommandInteraction,
  prelude::Context,
};

use crate::{
  bot::commands::{defer_message, respond_message, update_message, CommandOutput},
  session::manager::{SessionCreateError, SessionManager},
  utils::embed::{EmbedBuilder, Status},
};

pub const NAME: &str = "join";

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
        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .title("Cannot join voice channel")
            .description("You need to connect to a voice channel")
            .status(Status::Error)
            .build(),
          true,
        )
        .await;

        return;
      }
    };

    let data = ctx.data.read().await;
    let mut session_manager = data.get::<SessionManager>().unwrap().clone();

    // Check if another session is already active in this server
    let mut session_opt = session_manager.get_session(guild.id).await;

    if let Some(session) = &session_opt {
      if let Some(owner) = session.get_owner().await {
        let msg = if owner == command.user.id {
          "You are already controlling the bot"
        } else {
          "The bot is currently being controlled by someone else"
        };

        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .title("Cannot join voice channel")
            .description(msg)
            .status(Status::Error)
            .build(),
          true,
        )
        .await;

        return;
      }
    };

    // Prevent duplicate Spotify sessions
    if let Some(session) = session_manager.find(command.user.id).await {
      respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
          .title("Cannot join voice channel")
          .description(
          format!(
            "You are already playing music in another server ({}).\nStop playing in that server first before joining this one.",
            ctx.cache.guild(session.get_guild_id()).unwrap().name
          )).status(Status::Error).build(),
          true,
        )
        .await;

      return;
    }

    defer_message(&ctx, &command, false).await;

    if let Some(session) = &session_opt {
      trace!("{} != {}", session.get_channel_id(), channel_id);
      if session.get_channel_id() != channel_id {
        session.disconnect().await;
        session_opt = None;

        // Give serenity/songbird some time to register the disconnect
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    }

    if let Some(session) = &session_opt {
      if let Err(why) = session.update_owner(&ctx, command.user.id).await {
        // Need to link first
        if let SessionCreateError::NoSpotifyError = why {
          update_message(
            &ctx,
            &command,
            EmbedBuilder::new()
              .title("Cannot join voice channel")
              .description("You need to link your Spotify account. Use </link:1036714850367320136> or go to [the accounts website](https://account.spoticord.com/) to get started.")
              .status(Status::Error)
              .build(),
          )
          .await;

          return;
        }

        // Any other error
        update_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .title("Cannot join voice channel")
            .description("An error occured while joining the channel. Please try again later.")
            .status(Status::Error)
            .build(),
        )
        .await;

        return;
      }
    } else {
      // Create the session, and handle potential errors
      if let Err(why) = session_manager
        .create_session(
          &ctx,
          guild.id,
          channel_id,
          command.channel_id,
          command.user.id,
        )
        .await
      {
        // Need to link first
        if let SessionCreateError::NoSpotifyError = why {
          update_message(
            &ctx,
            &command,
            EmbedBuilder::new()
              .title("Cannot join voice channel")
              .description("You need to link your Spotify account. Use </link:1036714850367320136> or go to [the accounts website](https://account.spoticord.com/) to get started.")
              .status(Status::Error)
              .build(),
          )
          .await;

          return;
        }

        // Any other error
        update_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .title("Cannot join voice channel")
            .description("An error occured while joining the channel. Please try again later.")
            .status(Status::Error)
            .build(),
        )
        .await;

        return;
      };
    }

    update_message(
      &ctx,
      &command,
      EmbedBuilder::new()
        .title("Connected to voice channel")
        .icon_url("https://spoticord.com/speaker.png")
        .description(format!("Come listen along in <#{}>", channel_id))
        .footer("Spotify will automatically start playing on Spoticord")
        .status(Status::Info)
        .build(),
    )
    .await;
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name(NAME)
    .description("Request the bot to join the current voice channel")
}
