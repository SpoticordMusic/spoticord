use std::sync::{Arc, Mutex};

use ipc_channel::ipc::{self, IpcError, IpcOneShotServer, IpcReceiver, IpcSender};

use self::packet::IpcPacket;

pub mod packet;

pub struct Server {
  tx: IpcOneShotServer<IpcSender<IpcPacket>>,
  rx: IpcOneShotServer<IpcReceiver<IpcPacket>>,
}

impl Server {
  pub fn create() -> Result<(Self, String, String), IpcError> {
    let (tx, tx_name) = IpcOneShotServer::new().map_err(IpcError::Io)?;
    let (rx, rx_name) = IpcOneShotServer::new().map_err(IpcError::Io)?;

    Ok((Self { tx, rx }, tx_name, rx_name))
  }

  pub fn accept(self) -> Result<Client, IpcError> {
    let (_, tx) = self.tx.accept().map_err(IpcError::Bincode)?;
    let (_, rx) = self.rx.accept().map_err(IpcError::Bincode)?;

    Ok(Client::new(tx, rx))
  }
}

#[derive(Clone)]
pub struct Client {
  tx: Arc<Mutex<IpcSender<IpcPacket>>>,
  rx: Arc<Mutex<IpcReceiver<IpcPacket>>>,
}

impl Client {
  pub fn new(tx: IpcSender<IpcPacket>, rx: IpcReceiver<IpcPacket>) -> Client {
    Client {
      tx: Arc::new(Mutex::new(tx)),
      rx: Arc::new(Mutex::new(rx)),
    }
  }

  pub fn connect(tx_name: impl Into<String>, rx_name: impl Into<String>) -> Result<Self, IpcError> {
    let (tx, remote_rx) = ipc::channel().map_err(IpcError::Io)?;
    let (remote_tx, rx) = ipc::channel().map_err(IpcError::Io)?;

    let ttx = IpcSender::connect(tx_name.into()).map_err(IpcError::Io)?;
    let trx = IpcSender::connect(rx_name.into()).map_err(IpcError::Io)?;

    ttx.send(remote_tx).map_err(IpcError::Bincode)?;
    trx.send(remote_rx).map_err(IpcError::Bincode)?;

    Ok(Client::new(tx, rx))
  }

  pub fn send(&self, packet: IpcPacket) -> Result<(), IpcError> {
    self
      .tx
      .lock()
      .unwrap()
      .send(packet)
      .map_err(IpcError::Bincode)
  }

  pub fn recv(&self) -> Result<IpcPacket, IpcError> {
    self.rx.lock().unwrap().recv()
  }
}
