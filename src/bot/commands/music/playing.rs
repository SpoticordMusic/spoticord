use librespot::core::spotify_id::SpotifyAudioType;
use log::error;
use serenity::{
  builder::CreateApplicationCommand,
  model::prelude::interaction::{
    application_command::ApplicationCommandInteraction, InteractionResponseType,
  },
  prelude::Context,
};

use crate::{
  bot::commands::{respond_message, CommandOutput},
  session::manager::SessionManager,
  utils::{
    self,
    embed::{EmbedBuilder, Status},
  },
};

pub const NAME: &str = "playing";

pub fn run(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    let not_playing = async {
      respond_message(
        &ctx,
        &command,
        EmbedBuilder::new()
          .title("Cannot get track info")
          .icon_url("https://tabler-icons.io/static/tabler-icons/icons/ban.svg")
          .description("I'm currently not playing any music in this server")
          .status(Status::Error)
          .build(),
        true,
      )
      .await;
    };

    let data = ctx.data.read().await;
    let session_manager = data
      .get::<SessionManager>()
      .expect("to contain a value")
      .clone();

    let session = match session_manager
      .get_session(command.guild_id.expect("to contain a value"))
      .await
    {
      Some(session) => session,
      None => {
        not_playing.await;

        return;
      }
    };

    let owner = match session.owner().await {
      Some(owner) => owner,
      None => {
        not_playing.await;

        return;
      }
    };

    // Get Playback Info from session
    let pbi = match session.playback_info().await {
      Some(pbi) => pbi,
      None => {
        not_playing.await;

        return;
      }
    };

    let spotify_id = match pbi.spotify_id {
      Some(spotify_id) => spotify_id,
      None => {
        not_playing.await;

        return;
      }
    };

    // Get audio type
    let audio_type = if spotify_id.audio_type == SpotifyAudioType::Track {
      "track"
    } else {
      "episode"
    };

    // Create title
    let title = format!(
      "{} - {}",
      pbi.get_artists().expect("to contain a value"),
      pbi.get_name().expect("to contain a value")
    );

    // Create description
    let mut description = String::new();

    let position = pbi.get_position();
    let spot = position * 20 / pbi.duration_ms;

    description.push_str(if pbi.is_playing { "▶️ " } else { "⏸️ " });

    for i in 0..20 {
      if i == spot {
        description.push('🔵');
      } else {
        description.push('▬');
      }
    }

    description.push_str("\n:alarm_clock: ");
    description.push_str(&format!(
      "{} / {}",
      utils::time_to_str(position / 1000),
      utils::time_to_str(pbi.duration_ms / 1000)
    ));

    // Get owner of session
    let owner = match utils::discord::get_user(&ctx, owner).await {
      Some(user) => user,
      None => {
        // This shouldn't happen

        error!("Could not find user with id {}", owner);

        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .title("[INTERNAL ERROR] Cannot get track info")
            .description(format!(
              "Could not find user with id {}\nThis is an issue with the bot!",
              owner
            ))
            .status(Status::Error)
            .build(),
          true,
        )
        .await;

        return;
      }
    };

    // Get the thumbnail image
    let thumbnail = pbi.get_thumbnail_url().expect("to contain a value");

    if let Err(why) = command
      .create_interaction_response(&ctx.http, |response| {
        response
          .kind(InteractionResponseType::ChannelMessageWithSource)
          .interaction_response_data(|message| {
            message.embed(|embed| {
              embed
                .author(|author| {
                  author
                    .name("Currently Playing")
                    .icon_url("https://spoticord.com/spotify-logo.png")
                })
                .title(title)
                .url(format!(
                  "https://open.spotify.com/{}/{}",
                  audio_type,
                  spotify_id
                    .to_base62()
                    .expect("to be able to convert to base62")
                ))
                .description(description)
                .footer(|footer| footer.text(&owner.name).icon_url(owner.face()))
                .thumbnail(&thumbnail)
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
    .description("Display which song is currently being played")
}
