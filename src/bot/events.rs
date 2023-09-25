/* This file implements all events for the Discord gateway */

use super::commands::CommandManager;
use crate::consts::MOTD;
use log::*;
use serenity::{
  async_trait,
  model::prelude::{
    interaction::{
      application_command::ApplicationCommandInteraction,
      message_component::MessageComponentInteraction, Interaction,
    },
    Activity, GuildId, Ready,
  },
  prelude::{Context, EventHandler},
};

// If the GUILD_ID environment variable is set, only allow commands from that guild
macro_rules! enforce_guild {
  ($interaction:ident) => {
    if let Ok(guild_id) = std::env::var("GUILD_ID") {
      if let Ok(guild_id) = guild_id.parse::<u64>() {
        let guild_id = GuildId(guild_id);

        if let Some(interaction_guild_id) = $interaction.guild_id {
          if guild_id != interaction_guild_id {
            return;
          }
        }
      }
    }
  };
}

// Handler struct with a command parameter, an array of dictionary which takes a string and function
pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
  // READY event, emitted when the bot/shard starts up
  async fn ready(&self, ctx: Context, ready: Ready) {
    let data = ctx.data.read().await;
    let command_manager = data.get::<CommandManager>().expect("to contain a value");

    debug!("Ready received, logged in as {}", ready.user.name);

    command_manager.register(&ctx).await;

    ctx.set_activity(Activity::listening(MOTD)).await;

    info!("{} has come online", ready.user.name);
  }

  // INTERACTION_CREATE event, emitted when the bot receives an interaction (slash command, button, etc.)
  async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
    match interaction {
      Interaction::ApplicationCommand(command) => handle_command(ctx, command).await,
      Interaction::MessageComponent(component) => handle_component(ctx, component).await,
      _ => {}
    }
  }
}

async fn handle_command(ctx: Context, command: ApplicationCommandInteraction) {
  enforce_guild!(command);

  // Commands must only be executed inside of guilds

  let guild_id = match command.guild_id {
    Some(guild_id) => guild_id,
    None => {
      if let Err(why) = command
             .create_interaction_response(&ctx.http, |response| {
               response
                 .kind(serenity::model::prelude::interaction::InteractionResponseType::ChannelMessageWithSource)
                 .interaction_response_data(|message| {
                   message.content("You can only execute commands inside of a server")
                 })
             })
             .await {
               error!("Failed to send run-in-guild-only error message: {}", why);
             }

      return;
    }
  };

  trace!(
    "Received command interaction: command={} user={} guild={}",
    command.data.name,
    command.user.id,
    guild_id
  );

  let data = ctx.data.read().await;
  let command_manager = data.get::<CommandManager>().expect("to contain a value");

  command_manager.execute_command(&ctx, command).await;
}

async fn handle_component(ctx: Context, component: MessageComponentInteraction) {
  enforce_guild!(component);

  // Components can only be interacted with inside of guilds

  let guild_id = match component.guild_id {
    Some(guild_id) => guild_id,
    None => {
      if let Err(why) = component
             .create_interaction_response(&ctx.http, |response| {
               response
                 .kind(serenity::model::prelude::interaction::InteractionResponseType::ChannelMessageWithSource)
                 .interaction_response_data(|message| {
                   message.content("You can only interact with components inside of a server")
                 })
             })
             .await {
               error!("Failed to send run-in-guild-only error message: {}", why);
             }

      return;
    }
  };

  trace!(
    "Received component interaction: command={} user={} guild={}",
    component.data.custom_id,
    component.user.id,
    guild_id
  );

  let data = ctx.data.read().await;
  let command_manager = data.get::<CommandManager>().expect("to contain a value");

  command_manager.execute_component(&ctx, component).await;
}
