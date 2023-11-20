use log::error;
use poise::serenity_prelude::{model::prelude::Channel, Error};

use crate::{
    bot::Context, consts::SPOTICORD_ACCOUNTS_URL, session::manager::SessionCreateError,
    utils::embed::Color,
};

/// Request the bot to join the current voice channel
#[poise::command(slash_command)]
pub async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild) = ctx.guild() else {
        ctx.send(|b| {
            b.embed(|e| {
                e.description("You can only execute this command inside of a server")
                    .color(Color::Error)
            })
        })
        .await?;

        return Ok(());
    };

    let Some(channel) = guild
        .voice_states
        .get(&ctx.author().id)
        .and_then(|state| state.channel_id)
    else {
        ctx.send(|b| {
            b.embed(|e| {
                e.title("Cannot join voice channel")
                    .description("You need to connect to a voice channel")
                    .color(Color::Error)
            })
            .ephemeral(true)
        })
        .await?;

        return Ok(());
    };

    // Check for Voice Channel permissions
    {
        let channel = match channel.to_channel(&ctx).await {
            Ok(Channel::Guild(channel)) => channel,
            Ok(_) => {
                ctx.send(|b| {
                    b.embed(|e| {
                        e.title("Cannot join voice channel")
                            .description("The voice channel you are in is not supported")
                            .color(Color::Error)
                    })
                    .ephemeral(true)
                })
                .await?;

                return Ok(());
            }

            Err(why) => {
                error!("Failed to get channel: {why}");

                ctx
          .send(|b| {
            b.embed(|e| {
              e.title("Cannot join voice channel")
                .description("The voice channel you are in is not available.\nI might not the permission to see this channel.")
                .color(Color::Error)
            })
            .ephemeral(true)
          })
          .await?;

                return Ok(());
            }
        };

        if let Ok(permissions) =
            channel.permissions_for_user(ctx.cache(), ctx.cache().current_user_id())
        {
            if !permissions.view_channel() || !permissions.connect() || !permissions.speak() {
                ctx.send(|b| {
                    b.embed(|e| {
                        e.title("Cannot join voice channel")
                            .description(
                                "I do not have the permissions to connect to that voice channel",
                            )
                            .color(Color::Error)
                    })
                    .ephemeral(true)
                })
                .await?;

                return Ok(());
            }
        }
    }

    // Check for Text Channel permissions
    {
        let channel = match ctx.channel_id().to_channel(&ctx).await {
            Ok(Channel::Guild(channel)) => channel,
            Ok(_) => {
                ctx.send(|b| {
                    b.embed(|e| {
                        e.title("Cannot join voice channel")
                            .description("The voice channel you are in is not supported")
                            .color(Color::Error)
                    })
                    .ephemeral(true)
                })
                .await?;

                return Ok(());
            }
            Err(why) => {
                error!("Failed to get channel: {why}");

                ctx
        .send(|b| {
          b.embed(|e| {
            e.title("Cannot join voice channel")
              .description("The voice channel you are in is not available.\nI might not the permission to see this channel.")
              .color(Color::Error)
          })
          .ephemeral(true)
        })
        .await?;

                return Ok(());
            }
        };

        if let Ok(permissions) =
            channel.permissions_for_user(ctx.cache(), ctx.cache().current_user_id())
        {
            if !permissions.view_channel()
                || !permissions.send_messages()
                || !permissions.embed_links()
            {
                ctx.send(|b| {
                    b.embed(|e| {
                        e.title("Cannot join voice channel")
                .description(
                  "I do not have the permissions to send messages / links in this text channel",
                )
                .color(Color::Error)
                    })
                    .ephemeral(true)
                })
                .await?;

                return Ok(());
            }
        }
    }

    let sm = &ctx.data().session_manager;

    // Check if another session is already active in this server
    let mut session_opt = sm.get_session(&guild.id).await;

    if let Some(session) = &session_opt {
        if let Some(owner) = session.owner().await {
            let msg = if owner == ctx.author().id {
                "You are already controlling the bot"
            } else {
                "The bot is currently being controlled by someone else"
            };

            ctx.send(|b| {
                b.embed(|e| {
                    e.title("Cannot join voice channel")
                        .description(msg)
                        .color(Color::Error)
                })
                .ephemeral(true)
            })
            .await?;

            return Ok(());
        }
    }

    // Prevent duplicate Spotify sessions
    if let Some(session) = sm.find(ctx.author().id).await {
        let message = format!("You are already playing music in another server ({}).\nStop playing in that server first before using the bot in this server.",ctx.cache().guild(session.guild_id().await).map(|g| g.name).unwrap_or("<Failed to retrieve server name".into()));

        ctx.send(|b| {
            b.embed(|e| {
                e.title("Cannot join voice channel")
                    .description(message)
                    .color(Color::Error)
            })
            .ephemeral(true)
        })
        .await?;

        return Ok(());
    }

    ctx.defer().await?;

    if let Some(session) = &session_opt {
        if session.channel_id().await != channel {
            session.disconnect().await;
            session_opt = None;

            // Give serenity/songbird some time to register the disconnect
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    macro_rules! report_error {
    ($why:ident) => {
      match $why {
        // User has not linked their account
        SessionCreateError::NoSpotify => {
          ctx
            .send(|b| {
              b.embed(|e| {
                e.title("Cannot join voice channel")
                  .description(format!("You need to link your Spotify account. Use </link:1036714850367320136> or go to [the accounts website]({}) to get started.", SPOTICORD_ACCOUNTS_URL.as_str()))
                  .color(Color::Error)
              })
            })
            .await?;
        }

        SessionCreateError::SpotifyExpired => {
          ctx
            .send(|b| {
              b.embed(|e| {
                e.title("Cannot join voice channel")
                  .description(format!("Spoticord no longer has access to your Spotify account. Use </link:1036714850367320136> or go to [the accounts website]({}) to relink your Spotify account.", SPOTICORD_ACCOUNTS_URL.as_str()))
                  .color(Color::Error)
              })
            })
            .await?;
        }

        SessionCreateError::JoinError(why) => {
          ctx
            .send(|b| {
              b.embed(|e| {
                e.title("Cannot join voice channel")
                  .description(format!(
                    "An error occured while joining the channel. Please try running </join:1036714850367320142> again.\n\nError details: `{why}`"
                  ))
                  .color(Color::Error)
              })
            })
            .await?;
        }

        _ => {
          ctx
            .send(|b| {
              b.embed(|e| {
                e.title("Cannot join voice channel")
                  .description(
                    "An error occured while joining the channel. Please try again later.",
                  )
                  .color(Color::Error)
              })
            })
            .await?;
        }
      }
    };
  }

    if let Some(session) = session_opt.as_mut() {
        if let Err(why) = session.update_owner(&ctx, ctx.author().id).await {
            report_error!(why);
        }
    } else {
        // Create the session, and handle potential errors
        if let Err(why) = sm
            .create_session(&ctx, guild.id, channel, ctx.channel_id(), ctx.author().id)
            .await
        {
            report_error!(why);
        };
    }

    ctx.send(|b| {
        b.embed(|e| {
            e.author(|a| {
                a.name("Connected to voice channel")
                    .icon_url("https://spoticord.com/speaker.png")
            })
            .description(format!("Come listen along in <#{}>", channel))
            .footer(|f| f.text("You must manually go to Spotify and select your device"))
            .color(Color::Info)
        })
    })
    .await?;

    Ok(())
}
