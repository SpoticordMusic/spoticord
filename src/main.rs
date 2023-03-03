use dotenv::dotenv;

use crate::{bot::commands::CommandManager, database::Database, session::manager::SessionManager};
use log::*;
use serenity::{framework::StandardFramework, prelude::GatewayIntents, Client};
use songbird::SerenityInit;
use std::{any::Any, env, process::exit};

#[cfg(feature = "metrics")]
use metrics::MetricsManager;

#[cfg(unix)]
use tokio::signal::unix::SignalKind;

#[cfg(feature = "metrics")]
mod metrics;

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

  let args: Vec<String> = env::args().collect();

  if args.len() > 2 && &args[1] == "--player" {
    // Woah! We're running in player mode!

    debug!("Starting Spoticord player");

    player::main().await;

    debug!("Player exited, shutting down");

    return;
  }

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

  #[cfg(feature = "metrics")]
  let metrics_manager = {
    let metrics_url = env::var("METRICS_URL").expect("a prometheus pusher URL in the environment");
    MetricsManager::new(metrics_url)
  };

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

    #[cfg(feature = "metrics")]
    data.insert::<MetricsManager>(metrics_manager.clone());
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
        _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
          #[cfg(feature = "metrics")]
          {
            let guild_count = _cache.guilds().len();
            let active_count = session_manager.get_active_session_count().await;
            let total_count = session_manager.get_session_count().await;

            metrics_manager.set_server_count(guild_count);
            metrics_manager.set_active_sessions(active_count);
            metrics_manager.set_total_sessions(total_count);

            // Yes, I like to handle my s's when I'm working with amounts
            debug!(
              "Updated metrics: {} guild{}, {} active session{}, {} total session{}",
              guild_count,
              if guild_count == 1 { "" } else { "s" },
              active_count,
              if active_count == 1 { "" } else { "s" },
              total_count,
              if total_count == 1 { "" } else { "s" }
            );
          }
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
              let term = term.downcast_mut::<tokio::signal::unix::Signal>().expect("to be able to downcast");

              term.recv().await
            }

            _ => None
          }
        }, if term.is_some() => {
          info!("Received terminate signal, shutting down...");

          shard_manager.lock().await.shutdown_all().await;

          #[cfg(feature = "metrics")]
          metrics_manager.stop();

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
