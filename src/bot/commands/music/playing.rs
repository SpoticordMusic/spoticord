use librespot::core::spotify_id::SpotifyId;
use log::error;
use poise::serenity_prelude::{builder::CreateEmbed, Error, User};

use crate::{
  bot::Context,
  session::pbi::PlaybackInfo,
  utils::{self, embed::Color},
};

/// Display which song is currently being played
#[poise::command(slash_command)]
pub async fn playing(ctx: Context<'_>) -> Result<(), Error> {
  macro_rules! not_playing {
    () => {
      ctx
        .send(|b| {
          b.embed(|e| {
            e.author(|a| {
              a.name("Cannot get track info")
                .icon_url("https://spoticord.com/forbidden.png")
            })
            .description("I'm currently not playing any music in this server")
            .color(Color::Error)
          })
          .ephemeral(true)
        })
        .await?;

      return Ok(());
    };
  }

  let Some(guild) = ctx.guild() else {
    ctx
      .send(|b| {
        b.embed(|e| {
          e.description("You can only execute this command inside of a server")
            .color(Color::Error)
        })
      })
      .await?;

    return Ok(());
  };

  let sm = &ctx.data().session_manager;

  let Some(session) = sm.get_session(&guild.id).await else {
    not_playing!();
  };

  let Some(owner) = session.owner().await else {
    not_playing!();
  };

  // Get playback Info from session
  let Some(pbi) = session.playback_info().await else {
    not_playing!();
  };

  // Get owner of session as User
  let owner = owner.to_user(&ctx).await?;

  // Get metadata
  let (title, description, thumbnail) = get_metadata(&pbi);

  if let Err(why) = ctx
    .send(|b| {
      b.embed(|e| {
        build_playing_embed(
          e,
          title,
          pbi.get_type(),
          pbi.spotify_id,
          description,
          owner,
          thumbnail,
        )
      })
    })
    .await
  {
    error!("Error sending message: {why}");
  }

  Ok(())
}

fn build_playing_embed(
  embed: &mut CreateEmbed,
  title: impl Into<String>,
  audio_type: impl Into<String>,
  spotify_id: SpotifyId,
  description: impl Into<String>,
  owner: User,
  thumbnail: impl Into<String>,
) -> &mut CreateEmbed {
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
    .color(Color::Info);

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
