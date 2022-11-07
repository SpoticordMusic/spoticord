use chrono::Datelike;
use dotenv::dotenv;

use log::*;
use serenity::{framework::StandardFramework, prelude::GatewayIntents, Client};
use songbird::SerenityInit;
use std::{any::Any, env, process::exit};

use crate::{
  bot::commands::CommandManager, database::Database, session::manager::SessionManager,
  stats::StatsManager,
};

#[cfg(unix)]
use tokio::signal::unix::SignalKind;

mod audio;
mod bot;
mod consts;
mod database;
mod ipc;
mod librespot_ext;
mod player;
mod session;
mod stats;
mod utils;

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

  let args: Vec<String> = env::args().collect();

  if args.len() > 2 {
    if &args[1] == "--player" {
      // Woah! We're running in player mode!

      debug!("Starting Spoticord player");

      player::main().await;

      debug!("Player exited, shutting down");

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
  let kv_url = env::var("KV_URL").expect("a redis URL in the environment");

  let stats_manager = StatsManager::new(kv_url).expect("Failed to connect to redis");
  let session_manager = SessionManager::new();

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
    data.insert::<SessionManager>(session_manager.clone());
  }

  let shard_manager = client.shard_manager.clone();
  let cache = client.cache_and_http.cache.clone();

  let mut term: Option<Box<dyn Any + Send>>;

  #[cfg(unix)]
  {
    term = Some(Box::new(
      tokio::signal::unix::signal(SignalKind::terminate()).unwrap(),
    ));
  }

  // Background tasks
  tokio::spawn(async move {
    loop {
      tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
          let guild_count = cache.guilds().len();
          let active_count = session_manager.get_active_session_count().await;

          if let Err(why) = stats_manager.set_server_count(guild_count) {
            error!("Failed to update server count: {}", why);
          }

          if let Err(why) = stats_manager.set_active_count(active_count) {
            error!("Failed to update active count: {}", why);
          }

          // Yes, I like to handle my s's when I'm working with amounts
          debug!("Updated stats: {} guild{}, {} active session{}", guild_count, if guild_count == 1 { "" } else { "s" }, active_count, if active_count == 1 { "" } else { "s" });
        }

        _ = tokio::signal::ctrl_c() => {
          info!("Received interrupt signal, shutting down...");

          shard_manager.lock().await.shutdown_all().await;

          break;
        }

        _ = async {
          #[cfg(unix)]
          match term {
            Some(ref mut term) => {
              let term = term.downcast_mut::<tokio::signal::unix::Signal>().unwrap();

              term.recv().await
            }

            _ => None
          }
        }, if term.is_some() => {
          info!("Received terminate signal, shutting down...");

          shard_manager.lock().await.shutdown_all().await;

          break;
        }
      }
    }
  });

  // Start the bot
  if let Err(why) = client.start_autosharded().await {
    error!("FATAL Error in bot: {:?}", why);
    exit(1);
  }
}
