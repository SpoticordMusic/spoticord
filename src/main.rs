use chrono::Datelike;
use dotenv::dotenv;

use log::*;
use serenity::{framework::StandardFramework, prelude::GatewayIntents, Client};
use songbird::SerenityInit;
use std::env;

use crate::{bot::commands::CommandManager, database::Database, session::manager::SessionManager};

mod audio;
mod bot;
mod database;
mod ipc;
mod librespot_ext;
mod player;
mod session;
mod utils;

#[tokio::main]
async fn main() {
  env_logger::init();

  let args: Vec<String> = env::args().collect();

  if args.len() > 2 {
    if &args[1] == "--player" {
      // Woah! We're running in player mode!

      debug!("Starting Spoticord player");

      player::main().await;

      return;
    }
  }

  info!("It's a good day");
  info!(" - Spoticord {}", chrono::Utc::now().year());

  let result = dotenv();

  if let Ok(path) = result {
    debug!("Loaded environment file: {}", path.to_str().unwrap());
  } else {
    warn!("No .env file found, expecting all necessary environment variables");
  }

  let token = env::var("TOKEN").expect("a token in the environment");
  let db_url = env::var("DATABASE_URL").expect("a database URL in the environment");

  // Create client
  let mut client = Client::builder(
    token,
    GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES,
  )
  .event_handler(crate::bot::events::Handler)
  .framework(StandardFramework::new())
  .register_songbird()
  .await
  .unwrap();

  {
    let mut data = client.data.write().await;

    data.insert::<Database>(Database::new(db_url, None));
    data.insert::<CommandManager>(CommandManager::new());
    data.insert::<SessionManager>(SessionManager::new());
  }

  let shard_manager = client.shard_manager.clone();

  // Spawn a task to shutdown the bot when a SIGINT is received
  tokio::spawn(async move {
    tokio::signal::ctrl_c()
      .await
      .expect("Could not register CTRL+C handler");

    info!("SIGINT Received, shutting down...");

    shard_manager.lock().await.shutdown_all().await;
  });

  if let Err(why) = client.start_autosharded().await {
    println!("Error in bot: {:?}", why);
  }
}
