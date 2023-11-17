#[cfg(feature = "stats")]
use crate::consts::KV_URL;

use crate::{
  bot::Data,
  consts::{DATABASE_URL, DISCORD_TOKEN, MOTD},
  database::Database,
  session::manager::SessionManager,
};
use dotenvy::dotenv;
use log::*;
use poise::{serenity_prelude::GuildId, FrameworkBuilder};
use songbird::SerenityInit;
use std::process::exit;

mod bot;
mod consts;
mod database;
mod librespot_ext;
mod player;
mod session;
mod utils;

#[cfg(feature = "stats")]
mod stats;

#[cfg(feature = "stats")]
use crate::stats::StatsManager;

#[tokio::main]
async fn main() {
  if std::env::var("RUST_LOG").is_err() {
    #[cfg(debug_assertions)]
    {
      std::env::set_var("RUST_LOG", "spoticord");
    }

    #[cfg(not(debug_assertions))]
    {
      std::env::set_var("RUST_LOG", "spoticord=info");
    }
  }

  env_logger::init();

  info!("It's a good day");
  info!(" - Spoticord, {}", MOTD);

  let result = dotenv();

  if let Ok(path) = result {
    debug!(
      "Loaded environment file: {}",
      path.to_str().expect("to get the string")
    );
  } else {
    warn!("No .env file found, expecting all necessary environment variables");
  }

  #[cfg(feature = "stats")]
  let stats_manager = StatsManager::new(KV_URL.as_str()).expect("Failed to connect to redis");
  let session_manager = SessionManager::new();
  let database = Database::new(DATABASE_URL.as_str(), None);

  // Create client
  let client = FrameworkBuilder::default()
    .token(DISCORD_TOKEN.as_str())
    .client_settings(|client| client.register_songbird())
    .intents(bot::get_framework_intents())
    .options(bot::get_framework_opts())
    .setup(move |ctx, _ready, framework| {
      // This runs after the first shard has connected successfully

      Box::pin(async move {
        match std::env::var("GUILD_ID").map(|str| str.parse::<u64>().map(GuildId)) {
          Ok(Ok(id)) => {
            poise::builtins::register_in_guild(ctx, &framework.options().commands, id).await?
          }
          _ => poise::builtins::register_globally(ctx, &framework.options().commands).await?,
        };

        let shard_manager = framework.shard_manager().clone();

        tokio::spawn(bot::background_loop(
          session_manager.clone(),
          #[cfg(feature = "stats")]
          stats_manager,
          shard_manager,
        ));

        Ok(Data {
          database,
          session_manager,
        })
      })
    });

  // Start the bot
  if let Err(why) = client.run_autosharded().await {
    error!("[FATAL] Error while running bot: {why}");
    exit(1);
  }
}
