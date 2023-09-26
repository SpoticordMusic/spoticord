use std::time::Duration;

use librespot::core::spotify_id::SpotifyId;
use log::error;
use serenity::{
  builder::{CreateApplicationCommand, CreateButton, CreateComponents, CreateEmbed},
  model::{
    prelude::{
      component::ButtonStyle,
      interaction::{
        application_command::ApplicationCommandInteraction,
        message_component::MessageComponentInteraction, InteractionResponseType,
      },
    },
    user::User,
  },
  prelude::Context,
};

use crate::{
  bot::commands::{respond_component_message, respond_message, CommandOutput},
  session::{manager::SessionManager, pbi::PlaybackInfo},
  utils::{
    self,
    embed::{EmbedBuilder, Status},
  },
};

pub const NAME: &str = "playing";

pub fn command(ctx: Context, command: ApplicationCommandInteraction) -> CommandOutput {
  Box::pin(async move {
    macro_rules! not_playing {
      () => {
        respond_message(
          &ctx,
          &command,
          EmbedBuilder::new()
            .title("Cannot get track info")
            .icon_url("https://spoticord.com/forbidden.png")
            .description("I'm currently not playing any music in this server")
            .status(Status::Error)
            .build(),
          true,
        )
        .await;
      };
    }

    let data = ctx.data.read().await;
    let session_manager = data
      .get::<SessionManager>()
      .expect("to contain a value")
      .clone();

    let Some(session) = session_manager
      .get_session(command.guild_id.expect("to contain a value"))
      .await
    else {
      not_playing!();

      return;
    };

    let Some(owner) = session.owner().await else {
      not_playing!();

      return;
    };

    // Get Playback Info from session
    let Some(pbi) = session.playback_info().await else {
      not_playing!();

      return;
    };

    // Get owner of session
    let Some(owner) = utils::discord::get_user(&ctx, owner).await else {
      // This shouldn't happen

      error!("Could not find user with ID: {owner}");

      respond_message(
        &ctx,
        &command,
        EmbedBuilder::new()
          .title("[INTERNAL ERROR] Cannot get track info")
          .description(format!(
            "Could not find user with ID `{}`\nThis is an issue with the bot!",
            owner
          ))
          .status(Status::Error)
          .build(),
        true,
      )
      .await;

      return;
    };

    // Get metadata
    let (title, description, thumbnail) = get_metadata(&pbi);

    if let Err(why) = command
      .create_interaction_response(&ctx.http, |response| {
        response
          .kind(InteractionResponseType::ChannelMessageWithSource)
          .interaction_response_data(|message| {
            message
              .set_embed(build_playing_embed(
                title,
                pbi.get_type(),
                pbi.spotify_id,
                description,
                owner,
                thumbnail,
              ))
              .components(|components| create_button(components, pbi.is_playing))
          })
      })
      .await
    {
      error!("Error sending message: {why:?}");
    }
  })
}

pub fn component(ctx: Context, mut interaction: MessageComponentInteraction) -> CommandOutput {
  Box::pin(async move {
    let error_message = |title: &'static str, description: &'static str| async {
      respond_component_message(
        &ctx,
        &interaction,
        EmbedBuilder::new()
          .title(title.to_string())
          .icon_url("https://spoticord.com/forbidden.png")
          .description(description.to_string())
          .status(Status::Error)
          .build(),
        true,
      )
      .await;
    };

    let error_edit = |title: &'static str, description: &'static str| {
      let mut interaction = interaction.clone();
      let ctx = ctx.clone();

      async move {
        interaction.defer(&ctx.http).await.ok();

        if let Err(why) = interaction
          .message
          .edit(&ctx, |message| {
            message.embed(|embed| {
              embed
                .description(description)
                .author(|author| {
                  author
                    .name(title)
                    .icon_url("https://spoticord.com/forbidden.png")
                })
                .color(Status::Error)
            })
          })
          .await
        {
          error!("Failed to update playing message: {why}");
        }
      }
    };

    let data = ctx.data.read().await;
    let session_manager = data
      .get::<SessionManager>()
      .expect("to contain a value")
      .clone();

    // Check if session still exists
    let Some(mut session) = session_manager
      .get_session(interaction.guild_id.expect("to contain a value"))
      .await
    else {
      error_edit(
        "Cannot perform action",
        "I'm currently not playing any music in this server",
      )
      .await;

      return;
    };

    // Check if the session contains an owner
    let Some(owner) = session.owner().await else {
      error_edit(
        "Cannot change playback state",
        "I'm currently not playing any music in this server",
      )
      .await;

      return;
    };

    // Get Playback Info from session
    let Some(pbi) = session.playback_info().await else {
      error_edit(
        "Cannot change playback state",
        "I'm currently not playing any music in this server",
      )
      .await;

      return;
    };

    // Check if the user is the owner of the session
    if owner != interaction.user.id {
      error_message(
        "Cannot change playback state",
        "You must be the host to use the media buttons",
      )
      .await;

      return;
    }

    // Get owner of session
    let Some(owner) = utils::discord::get_user(&ctx, owner).await else {
      // This shouldn't happen

      error!("Could not find user with ID: {owner}");

      respond_component_message(
        &ctx,
        &interaction,
        EmbedBuilder::new()
          .title("[INTERNAL ERROR] Cannot get track info")
          .description(format!(
            "Could not find user with ID `{}`\nThis is an issue with the bot!",
            owner
          ))
          .status(Status::Error)
          .build(),
        true,
      )
      .await;

      return;
    };

    // Send the desired command to the session
    match interaction.data.custom_id.as_str() {
      "playing::btn_pause_play" => {
        if pbi.is_playing {
          session.pause().await
        } else {
          session.resume().await
        }
      }

      "playing::btn_previous_track" => session.previous().await,

      "playing::btn_next_track" => session.next().await,

      _ => {
        error!("Unknown custom_id: {}", interaction.data.custom_id);
      }
    };

    interaction.defer(&ctx.http).await.ok();
    tokio::time::sleep(Duration::from_millis(
      if interaction.data.custom_id == "playing::btn_pause_play" {
        0
      } else {
        2500
      },
    ))
    .await;
    update_embed(&mut interaction, &ctx, owner).await;
  })
}

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
  command
    .name(NAME)
    .description("Display which song is currently being played")
}

fn create_button(components: &mut CreateComponents, playing: bool) -> &mut CreateComponents {
  let mut prev_btn = CreateButton::default();
  prev_btn
    .style(ButtonStyle::Primary)
    .label("<<")
    .custom_id("playing::btn_previous_track");

  let mut toggle_btn = CreateButton::default();
  toggle_btn
    .style(ButtonStyle::Secondary)
    .label(if playing { "Pause" } else { "Play" })
    .custom_id("playing::btn_pause_play");

  let mut next_btn = CreateButton::default();
  next_btn
    .style(ButtonStyle::Primary)
    .label(">>")
    .custom_id("playing::btn_next_track");

  components.create_action_row(|ar| {
    ar.add_button(prev_btn)
      .add_button(toggle_btn)
      .add_button(next_btn)
  })
}

async fn update_embed(interaction: &mut MessageComponentInteraction, ctx: &Context, owner: User) {
  let error_edit = |title: &'static str, description: &'static str| {
    let mut interaction = interaction.clone();
    let ctx = ctx.clone();

    async move {
      interaction.defer(&ctx.http).await.ok();

      if let Err(why) = interaction
        .message
        .edit(&ctx, |message| {
          message.embed(|embed| {
            embed
              .description(description)
              .author(|author| {
                author
                  .name(title)
                  .icon_url("https://spoticord.com/forbidden.png")
              })
              .color(Status::Error)
          })
        })
        .await
      {
        error!("Failed to update playing message: {why}");
      }
    }
  };

  let data = ctx.data.read().await;
  let session_manager = data
    .get::<SessionManager>()
    .expect("to contain a value")
    .clone();

  // Check if session still exists
  let Some(session) = session_manager
    .get_session(interaction.guild_id.expect("to contain a value"))
    .await
  else {
    error_edit(
      "Cannot perform action",
      "I'm currently not playing any music in this server",
    )
    .await;

    return;
  };

  // Get Playback Info from session
  let Some(pbi) = session.playback_info().await else {
    error_edit(
      "Cannot change playback state",
      "I'm currently not playing any music in this server",
    )
    .await;

    return;
  };

  let (title, description, thumbnail) = get_metadata(&pbi);

  if let Err(why) = interaction
    .message
    .edit(&ctx, |message| {
      message
        .set_embed(build_playing_embed(
          title,
          pbi.get_type(),
          pbi.spotify_id,
          description,
          owner,
          thumbnail,
        ))
        .components(|components| create_button(components, pbi.is_playing));

      message
    })
    .await
  {
    error!("Failed to update playing message: {why}");
  }
}

fn build_playing_embed(
  title: impl Into<String>,
  audio_type: impl Into<String>,
  spotify_id: SpotifyId,
  description: impl Into<String>,
  owner: User,
  thumbnail: impl Into<String>,
) -> CreateEmbed {
  let mut embed = CreateEmbed::default();
  embed
    .author(|author| {
      author
        .name("Currently Playing")
        .icon_url("https://spoticord.com/spotify-logo.png")
    })
    .title(title.into())
    .url(format!(
      "https://open.spotify.com/{}/{}",
      audio_type.into(),
      spotify_id
        .to_base62()
        .expect("to be able to convert to base62")
    ))
    .description(description.into())
    .footer(|footer| footer.text(&owner.name).icon_url(owner.face()))
    .thumbnail(thumbnail.into())
    .color(Status::Info);

  embed
}

fn get_metadata(pbi: &PlaybackInfo) -> (String, String, String) {
  // Create title
  let title = format!("{} - {}", pbi.get_artists(), pbi.get_name());

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

  // Get the thumbnail image
  let thumbnail = pbi.get_thumbnail_url().expect("to contain a value");

  (title, description, thumbnail)
}
