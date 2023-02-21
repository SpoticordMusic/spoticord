use std::{collections::HashMap, future::Future, pin::Pin};

use log::{debug, error};
use serenity::{
  builder::{CreateApplicationCommand, CreateApplicationCommands},
  model::application::command::Command,
  model::prelude::{
    interaction::{
      application_command::ApplicationCommandInteraction,
      message_component::MessageComponentInteraction, InteractionResponseType,
    },
    GuildId,
  },
  prelude::{Context, TypeMapKey},
};

use crate::utils::embed::{make_embed_message, EmbedMessageOptions};

mod core;
mod music;

#[cfg(debug_assertions)]
mod ping;

#[cfg(debug_assertions)]
mod token;

pub async fn respond_message(
  ctx: &Context,
  command: &ApplicationCommandInteraction,
  options: EmbedMessageOptions,
  ephemeral: bool,
) {
  if let Err(why) = command
    .create_interaction_response(&ctx.http, |response| {
      response
        .kind(InteractionResponseType::ChannelMessageWithSource)
        .interaction_response_data(|message| {
          message
            .embed(|embed| make_embed_message(embed, options))
            .ephemeral(ephemeral)
        })
    })
    .await
  {
    error!("Error sending message: {:?}", why);
  }
}

pub async fn respond_component_message(
  ctx: &Context,
  component: &MessageComponentInteraction,
  options: EmbedMessageOptions,
  ephemeral: bool,
) {
  if let Err(why) = component
    .create_interaction_response(&ctx.http, |response| {
      response
        .kind(InteractionResponseType::ChannelMessageWithSource)
        .interaction_response_data(|message| {
          message
            .embed(|embed| make_embed_message(embed, options))
            .ephemeral(ephemeral)
        })
    })
    .await
  {
    error!("Error sending message: {:?}", why);
  }
}

pub async fn update_message(
  ctx: &Context,
  command: &ApplicationCommandInteraction,
  options: EmbedMessageOptions,
) {
  if let Err(why) = command
    .edit_original_interaction_response(&ctx.http, |message| {
      message.embed(|embed| make_embed_message(embed, options))
    })
    .await
  {
    error!("Error sending message: {:?}", why);
  }
}

pub async fn defer_message(
  ctx: &Context,
  command: &ApplicationCommandInteraction,
  ephemeral: bool,
) {
  if let Err(why) = command
    .create_interaction_response(&ctx.http, |response| {
      response
        .kind(InteractionResponseType::DeferredChannelMessageWithSource)
        .interaction_response_data(|message| message.ephemeral(ephemeral))
    })
    .await
  {
    error!("Error deferring message: {:?}", why);
  }
}

pub type CommandOutput = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type CommandExecutor = fn(Context, ApplicationCommandInteraction) -> CommandOutput;
pub type ComponentExecutor = fn(Context, MessageComponentInteraction) -> CommandOutput;

#[derive(Clone)]
pub struct CommandManager {
  commands: HashMap<String, CommandInfo>,
}

#[derive(Clone)]
pub struct CommandInfo {
  pub name: String,
  pub command_executor: CommandExecutor,
  pub component_executor: Option<ComponentExecutor>,
  pub register: fn(&mut CreateApplicationCommand) -> &mut CreateApplicationCommand,
}

impl CommandManager {
  pub fn new() -> Self {
    let mut instance = Self {
      commands: HashMap::new(),
    };

    // Debug-only commands
    #[cfg(debug_assertions)]
    {
      instance.insert(ping::NAME, ping::register, ping::command, None);
      instance.insert(token::NAME, token::register, token::command, None);
    }

    // Core commands
    instance.insert(
      core::help::NAME,
      core::help::register,
      core::help::command,
      None,
    );
    instance.insert(
      core::version::NAME,
      core::version::register,
      core::version::command,
      None,
    );
    instance.insert(
      core::link::NAME,
      core::link::register,
      core::link::command,
      None,
    );
    instance.insert(
      core::unlink::NAME,
      core::unlink::register,
      core::unlink::command,
      None,
    );
    instance.insert(
      core::rename::NAME,
      core::rename::register,
      core::rename::command,
      None,
    );

    // Music commands
    instance.insert(
      music::join::NAME,
      music::join::register,
      music::join::command,
      None,
    );
    instance.insert(
      music::leave::NAME,
      music::leave::register,
      music::leave::command,
      None,
    );
    instance.insert(
      music::playing::NAME,
      music::playing::register,
      music::playing::command,
      Some(music::playing::component),
    );

    instance
  }

  pub fn insert(
    &mut self,
    name: impl Into<String>,
    register: fn(&mut CreateApplicationCommand) -> &mut CreateApplicationCommand,
    command_executor: CommandExecutor,
    component_executor: Option<ComponentExecutor>,
  ) {
    let name = name.into();

    self.commands.insert(
      name.clone(),
      CommandInfo {
        name,
        register,
        command_executor,
        component_executor,
      },
    );
  }

  pub async fn register(&self, ctx: &Context) {
    let cmds = &self.commands;

    debug!(
      "Registering {} command{}",
      cmds.len(),
      if cmds.len() == 1 { "" } else { "s" }
    );

    fn _register_commands<'a>(
      cmds: &HashMap<String, CommandInfo>,
      mut commands: &'a mut CreateApplicationCommands,
    ) -> &'a mut CreateApplicationCommands {
      for command_info in cmds.values() {
        commands = commands.create_application_command(|command| (command_info.register)(command));
      }

      commands
    }

    if let Ok(guild_id) = std::env::var("GUILD_ID") {
      if let Ok(guild_id) = guild_id.parse::<u64>() {
        let guild_id = GuildId(guild_id);
        guild_id
          .set_application_commands(&ctx.http, |command| _register_commands(cmds, command))
          .await
          .expect("Failed to create guild commands");

        return;
      }
    }

    Command::set_global_application_commands(&ctx.http, |command| {
      _register_commands(cmds, command)
    })
    .await
    .expect("Failed to create global commands");
  }

  // On slash command interaction
  pub async fn execute_command(&self, ctx: &Context, interaction: ApplicationCommandInteraction) {
    let command = self.commands.get(&interaction.data.name);

    if let Some(command) = command {
      (command.command_executor)(ctx.clone(), interaction.clone()).await;
    } else {
      // Command does not exist
      if let Err(why) = interaction
        .create_interaction_response(&ctx.http, |response| {
          response
            .kind(InteractionResponseType::ChannelMessageWithSource)
            .interaction_response_data(|message| {
              message
                .content("Woops, that command doesn't exist")
                .ephemeral(true)
            })
        })
        .await
      {
        error!("Failed to respond to command: {}", why);
      }
    }
  }

  // On message component interaction (e.g. button)
  pub async fn execute_component(&self, ctx: &Context, interaction: MessageComponentInteraction) {
    let command = match interaction.data.custom_id.split("::").next() {
      Some(command) => command,
      None => return,
    };

    let command = self.commands.get(command);

    if let Some(command) = command {
      if let Some(executor) = command.component_executor {
        executor(ctx.clone(), interaction.clone()).await;

        return;
      }
    }

    if let Err(why) = interaction
      .create_interaction_response(&ctx.http, |response| {
        response
          .kind(InteractionResponseType::ChannelMessageWithSource)
          .interaction_response_data(|message| {
            message
              .content("Woops, that interaction doesn't exist")
              .ephemeral(true)
          })
      })
      .await
    {
      error!("Failed to respond to interaction: {}", why);
    }
  }
}

impl TypeMapKey for CommandManager {
  type Value = CommandManager;
}
