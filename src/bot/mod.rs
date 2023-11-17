pub mod commands;

use crate::{consts::MOTD, database::Database, session::manager::SessionManager};
use log::{debug, info};
use poise::{
  serenity_prelude::{Activity, Error, GatewayIntents, ShardManager},
  FrameworkOptions,
};
use std::{any::Any, sync::Arc};
use tokio::sync::Mutex;

#[cfg(feature = "stats")]
use crate::stats::StatsManager;

#[cfg(feature = "stats")]
use log::error;

#[cfg(unix)]
use tokio::signal::unix::SignalKind;

pub type Context<'a> = poise::ApplicationContext<'a, Data, Error>;

pub struct Data {
  pub database: Database,
  pub session_manager: SessionManager,
}

pub fn get_framework_intents() -> GatewayIntents {
  GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES
}

pub fn get_framework_opts() -> FrameworkOptions<Data, Error> {
  poise::FrameworkOptions {
    commands: vec![
      #[cfg(debug_assertions)]
      commands::ping(),
      #[cfg(debug_assertions)]
      commands::token(),
      commands::core::help(),
      commands::core::link(),
      commands::core::rename(),
      commands::core::unlink(),
      commands::core::version(),
      commands::music::join(),
      commands::music::leave(),
      commands::music::playing(),
      commands::music::stop(),
    ],
    event_handler: |_ctx, event, _framework, _data| {
      Box::pin(event_handler(_ctx, event, _framework, _data))
    },
    ..Default::default()
  }
}

pub async fn background_loop(
  session_manager: SessionManager,
  #[cfg(feature = "stats")] stats_manager: StatsManager,
  shard_manager: Arc<Mutex<ShardManager>>,
) {
  #[cfg(unix)]
  let mut term: Option<Box<dyn Any + Send>> = Some(Box::new(
    tokio::signal::unix::signal(SignalKind::terminate())
      .expect("to be able to create the signal stream"),
  ));

  #[cfg(not(unix))]
  let term: Option<Box<dyn Any + Send>> = None;

  loop {
    tokio::select! {
      _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
        #[cfg(feature = "stats")]
        {
          let active_count = session_manager.get_active_session_count().await;

          if let Err(why) = stats_manager.set_active_count(active_count) {
            error!("Failed to update active count: {why}");
          }
        }
      }

      _ = tokio::signal::ctrl_c() => {
        info!("Received interrupt signal, shutting down...");

        session_manager.shutdown().await;
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

        session_manager.shutdown().await;
        shard_manager.lock().await.shutdown_all().await;

        break;
      }
    }
  }
}

async fn event_handler(
  ctx: &poise::serenity_prelude::Context,
  event: &poise::Event<'_>,
  _framework: poise::FrameworkContext<'_, Data, poise::serenity_prelude::Error>,
  _data: &Data,
) -> Result<(), poise::serenity_prelude::Error> {
  if let poise::Event::Ready {
    data_about_bot: ready,
  } = event
  {
    debug!("Ready received, logged in as {}", ready.user.name);

    ctx.set_activity(Activity::listening(MOTD)).await;

    info!("{} has come online", ready.user.name);
  }

  Ok(())
}
