use std::{collections::HashMap, future::Future, pin::Pin};

use log::{debug, error};
use serenity::{
  builder::{CreateApplicationCommand, CreateApplicationCommands},
  model::application::command::Command,
  model::prelude::{
    interaction::{application_command::ApplicationCommandInteraction, InteractionResponseType},
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

pub type CommandOutput = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type CommandExecutor = fn(Context, ApplicationCommandInteraction) -> CommandOutput;

pub struct CommandManager {
  commands: HashMap<String, CommandInfo>,
}

pub struct CommandInfo {
  pub name: String,
  pub executor: CommandExecutor,
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
      instance.insert_command(ping::NAME, ping::register, ping::run);
      instance.insert_command(token::NAME, token::register, token::run);
    }

    // Core commands
    instance.insert_command(core::help::NAME, core::help::register, core::help::run);
    instance.insert_command(
      core::version::NAME,
      core::version::register,
      core::version::run,
    );
    instance.insert_command(core::link::NAME, core::link::register, core::link::run);
    instance.insert_command(
      core::unlink::NAME,
      core::unlink::register,
      core::unlink::run,
    );
    instance.insert_command(
      core::rename::NAME,
      core::rename::register,
      core::rename::run,
    );

    // Music commands
    instance.insert_command(music::join::NAME, music::join::register, music::join::run);
    instance.insert_command(
      music::leave::NAME,
      music::leave::register,
      music::leave::run,
    );
    instance.insert_command(
      music::playing::NAME,
      music::playing::register,
      music::playing::run,
    );

    instance
  }

  pub fn insert_command(
    &mut self,
    name: impl Into<String>,
    register: fn(&mut CreateApplicationCommand) -> &mut CreateApplicationCommand,
    executor: CommandExecutor,
  ) {
    let name = name.into();

    self.commands.insert(
      name.clone(),
      CommandInfo {
        name,
        register,
        executor,
      },
    );
  }

  pub async fn register_commands(&self, ctx: &Context) {
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
      for (_, command_info) in cmds {
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

  pub async fn execute_command(&self, ctx: &Context, interaction: ApplicationCommandInteraction) {
    let command = self.commands.get(&interaction.data.name);

    if let Some(command) = command {
      (command.executor)(ctx.clone(), interaction.clone()).await;
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
}

impl TypeMapKey for CommandManager {
  type Value = CommandManager;
}
