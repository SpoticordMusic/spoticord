/* This file implements all events for the Discord gateway */

use log::*;
use serenity::{
  async_trait,
  model::prelude::{interaction::Interaction, Activity, Ready},
  prelude::{Context, EventHandler},
};

use crate::consts::MOTD;

use super::commands::CommandManager;

// Handler struct with a command parameter, an array of dictionary which takes a string and function
pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
  // READY event, emitted when the bot/shard starts up
  async fn ready(&self, ctx: Context, ready: Ready) {
    let data = ctx.data.read().await;
    let command_manager = data.get::<CommandManager>().unwrap();

    debug!("Ready received, logged in as {}", ready.user.name);

    command_manager.register_commands(&ctx).await;

    ctx.set_activity(Activity::listening(MOTD)).await;

    info!("{} has come online", ready.user.name);
  }

  // INTERACTION_CREATE event, emitted when the bot receives an interaction (slash command, button, etc.)
  async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
    if let Interaction::ApplicationCommand(command) = interaction {
      // Commands must only be executed inside of guilds
      if command.guild_id.is_none() {
        command
          .create_interaction_response(&ctx.http, |response| {
            response
              .kind(serenity::model::prelude::interaction::InteractionResponseType::ChannelMessageWithSource)
              .interaction_response_data(|message| {
                message.content("You can only execute commands inside of a server")
              })
          })
          .await
          .unwrap();

        return;
      }

      trace!(
        "Received command interaction: command={} user={} guild={}",
        command.data.name,
        command.user.id,
        command.guild_id.unwrap()
      );

      let data = ctx.data.read().await;
      let command_manager = data.get::<CommandManager>().unwrap();

      command_manager.execute_command(&ctx, command).await;
    }
  }
}
