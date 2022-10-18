use self::manager::{SessionCreateError, SessionManager};
use crate::{
  database::{Database, DatabaseError},
  ipc::{self, packet::IpcPacket},
};
use ipc_channel::ipc::IpcError;
use log::*;
use serenity::{
  async_trait,
  model::prelude::{ChannelId, GuildId, UserId},
  prelude::Context,
};
use songbird::{
  create_player,
  error::JoinResult,
  input::{children_to_reader, Input},
  tracks::TrackHandle,
  Call, Event, EventContext, EventHandler,
};
use std::{
  process::{Command, Stdio},
  sync::Arc,
};
use tokio::sync::Mutex;

pub mod manager;

#[derive(Clone)]
pub struct SpoticordSession {
  owner: UserId,
  guild_id: GuildId,
  channel_id: ChannelId,

  session_manager: SessionManager,

  call: Arc<Mutex<Call>>,
  track: TrackHandle,
}

impl SpoticordSession {
  pub async fn new(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    owner_id: UserId,
  ) -> Result<SpoticordSession, SessionCreateError> {
    // Get the Spotify token of the owner
    let data = ctx.data.read().await;
    let database = data.get::<Database>().unwrap();
    let session_manager = data.get::<SessionManager>().unwrap().clone();

    let token = match database.get_access_token(owner_id.to_string()).await {
      Ok(token) => token,
      Err(why) => {
        if let DatabaseError::InvalidStatusCode(code) = why {
          if code == 404 {
            return Err(SessionCreateError::NoSpotifyError);
          }
        }

        return Err(SessionCreateError::DatabaseError);
      }
    };

    let user = match database.get_user(owner_id.to_string()).await {
      Ok(user) => user,
      Err(why) => {
        error!("Failed to get user: {:?}", why);
        return Err(SessionCreateError::DatabaseError);
      }
    };

    // Create IPC oneshot server
    let (server, tx_name, rx_name) = match ipc::Server::create() {
      Ok(server) => server,
      Err(why) => {
        error!("Failed to create IPC server: {:?}", why);
        return Err(SessionCreateError::ForkError);
      }
    };

    // Join the voice channel
    let songbird = songbird::get(ctx).await.unwrap().clone();

    let (call, result) = songbird.join(guild_id, channel_id).await;

    if let Err(why) = result {
      error!("Error joining voice channel: {:?}", why);
      return Err(SessionCreateError::JoinError(channel_id, guild_id));
    }

    let mut call_mut = call.lock().await;

    // Spawn player process
    let args: Vec<String> = std::env::args().collect();
    let child = match Command::new(&args[0])
      .args(["--player", &tx_name, &rx_name])
      .stdout(Stdio::piped())
      .stderr(Stdio::inherit())
      .spawn()
    {
      Ok(child) => child,
      Err(why) => {
        error!("Failed to start player process: {:?}", why);
        return Err(SessionCreateError::ForkError);
      }
    };

    // Establish bi-directional IPC channel
    let client = match server.accept() {
      Ok(client) => client,
      Err(why) => {
        error!("Failed to accept IPC connection: {:?}", why);

        return Err(SessionCreateError::ForkError);
      }
    };

    // Pipe player audio to the voice channel
    let reader = children_to_reader::<f32>(vec![child]);

    // Create track (paused, fixes audio glitches)
    let (mut track, track_handle) = create_player(Input::float_pcm(true, reader));
    track.pause();

    // Set call audio to track
    call_mut.play_only(track);

    // Clone variables for use in the IPC handler
    let ipc_track = track_handle.clone();
    let ipc_client = client.clone();

    // Handle IPC packets
    // This will automatically quit once the IPC connection is closed
    tokio::spawn(async move {
      let check_result = |result| {
        if let Err(why) = result {
          error!("Failed to issue track command: {:?}", why);
        }
      };

      loop {
        let msg = match ipc_client.recv() {
          Ok(msg) => msg,
          Err(why) => {
            if let IpcError::Disconnected = why {
              break;
            }

            error!("Failed to receive IPC message: {:?}", why);
            break;
          }
        };

        match msg {
          IpcPacket::StartPlayback => {
            check_result(ipc_track.play());
          }

          IpcPacket::StopPlayback => {
            check_result(ipc_track.pause());
          }

          _ => {}
        }
      }
    });

    // Set up events
    let instance = Self {
      owner: owner_id,
      guild_id,
      channel_id,
      session_manager,
      call: call.clone(),
      track: track_handle,
    };

    call_mut.add_global_event(
      songbird::Event::Core(songbird::CoreEvent::DriverDisconnect),
      instance.clone(),
    );

    call_mut.add_global_event(
      songbird::Event::Core(songbird::CoreEvent::ClientDisconnect),
      instance.clone(),
    );

    if let Err(why) = client.send(IpcPacket::Connect(token, user.device_name)) {
      error!("Failed to send IpcPacket::Connect packet: {:?}", why);
    }

    Ok(instance)
  }

  pub async fn disconnect(&self) -> JoinResult<()> {
    info!("Disconnecting from voice channel {}", self.channel_id);

    self
      .session_manager
      .clone()
      .remove_session(self.guild_id)
      .await;

    let mut call = self.call.lock().await;

    self.track.stop().unwrap_or(());
    call.remove_all_global_events();
    call.leave().await
  }

  pub fn get_owner(&self) -> UserId {
    self.owner
  }

  pub fn get_guild_id(&self) -> GuildId {
    self.guild_id
  }

  pub fn get_channel_id(&self) -> ChannelId {
    self.channel_id
  }
}

#[async_trait]
impl EventHandler for SpoticordSession {
  async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
    match ctx {
      EventContext::DriverDisconnect(_) => {
        debug!("Driver disconnected, leaving voice channel");
        self.disconnect().await.ok();
      }
      EventContext::ClientDisconnect(who) => {
        debug!("Client disconnected, {}", who.user_id.to_string());
      }
      _ => {}
    }

    return None;
  }
}
