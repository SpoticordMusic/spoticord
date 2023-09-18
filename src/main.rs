use dotenv::dotenv;

use crate::{bot::commands::CommandManager, database::Database, session::manager::SessionManager};
use log::*;
use serenity::{framework::StandardFramework, prelude::GatewayIntents, Client};
use songbird::SerenityInit;
use std::{any::Any, env, process::exit};

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

  info!("It's a good day");
  info!(" - Spoticord {}", time::OffsetDateTime::now_utc().year());

  let result = dotenv();

  if let Ok(path) = result {
    debug!(
      "Loaded environment file: {}",
      path.to_str().expect("to get the string")
    );
  } else {
    warn!("No .env file found, expecting all necessary environment variables");
  }

  let token = env::var("DISCORD_TOKEN").expect("a token in the environment");
  let db_url = env::var("DATABASE_URL").expect("a database URL in the environment");

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
  .expect("to create a client");

  {
    let mut data = client.data.write().await;

    data.insert::<Database>(Database::new(db_url, None));
    data.insert::<CommandManager>(CommandManager::new());
    data.insert::<SessionManager>(session_manager.clone());
  }

  let shard_manager = client.shard_manager.clone();
  let _cache = client.cache_and_http.cache.clone();

  #[cfg(unix)]
  let mut term: Option<Box<dyn Any + Send>> = Some(Box::new(
    tokio::signal::unix::signal(SignalKind::terminate())
      .expect("to be able to create the signal stream"),
  ));

  #[cfg(not(unix))]
  let term: Option<Box<dyn Any + Send>> = None;

  // Background tasks
  tokio::spawn(async move {
    loop {
      tokio::select! {
        _ = tokio::signal::ctrl_c() => {
          info!("Received interrupt signal, shutting down...");

          shard_manager.lock().await.shutdown_all().await;

          break;
        }

        _ = async {
          #[cfg(unix)]
          match term {
            Some(ref mut term) => {
              let term = term.downcast_mut::<tokio::signal::unix::Signal>().expect("to be able to downcast");

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
