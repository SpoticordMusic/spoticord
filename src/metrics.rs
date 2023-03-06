use std::{
  collections::hash_map::RandomState,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thread,
  time::Duration,
};

use lazy_static::lazy_static;
use prometheus::{
  opts, push_metrics, register_int_counter_vec, register_int_gauge, IntCounterVec, IntGauge,
};
use serenity::prelude::TypeMapKey;

use crate::session::pbi::PlaybackInfo;

lazy_static! {
  static ref TOTAL_SERVERS: IntGauge =
    register_int_gauge!("total_servers", "Total number of servers Spoticord is in").unwrap();
  static ref ACTIVE_SESSIONS: IntGauge = register_int_gauge!(
    "active_sessions",
    "Total number of servers with an active Spoticord session"
  )
  .unwrap();
  static ref TOTAL_SESSIONS: IntGauge = register_int_gauge!(
    "total_sessions",
    "Total number of servers with Spoticord in a voice channel"
  )
  .unwrap();
  static ref TRACKS_PLAYED: IntCounterVec =
    register_int_counter_vec!(opts!("tracks_played", "Tracks Played"), &["type"]).unwrap();
  static ref COMMANDS_EXECUTED: IntCounterVec = register_int_counter_vec!(
    opts!("commands_executed", "Commands Executed"),
    &["command"]
  )
  .unwrap();
}

#[derive(Clone)]
pub struct MetricsManager {
  should_stop: Arc<AtomicBool>,
}

impl MetricsManager {
  pub fn new(pusher_url: impl Into<String>) -> Self {
    let instance = Self {
      should_stop: Arc::new(AtomicBool::new(false)),
    };

    thread::spawn({
      let instance = instance.clone();
      let pusher_url = pusher_url.into();

      move || loop {
        thread::sleep(Duration::from_secs(5));

        if instance.should_stop() {
          break;
        }

        if let Err(why) = push_metrics::<RandomState>(
          "spoticord_metrics",
          Default::default(),
          &pusher_url,
          prometheus::gather(),
          None,
        ) {
          log::error!("Failed to push metrics: {}", why);
        }
      }
    });

    instance
  }

  pub fn should_stop(&self) -> bool {
    self.should_stop.load(Ordering::Relaxed)
  }

  pub fn stop(&self) {
    self.should_stop.store(true, Ordering::Relaxed);
  }

  pub fn set_server_count(&self, count: usize) {
    TOTAL_SERVERS.set(count as i64);
  }

  pub fn set_total_sessions(&self, count: usize) {
    TOTAL_SESSIONS.set(count as i64);
  }

  pub fn set_active_sessions(&self, count: usize) {
    ACTIVE_SESSIONS.set(count as i64);
  }

  pub fn track_play(&self, track: &PlaybackInfo) {
    let track_type = match track.get_type() {
      Some(track_type) => track_type,
      None => return,
    };

    TRACKS_PLAYED.with_label_values(&[&track_type]).inc();
  }

  pub fn command_exec(&self, command: &str) {
    COMMANDS_EXECUTED.with_label_values(&[command]).inc();
  }
}

impl TypeMapKey for MetricsManager {
  type Value = MetricsManager;
}
