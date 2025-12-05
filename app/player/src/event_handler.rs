use async_trait::async_trait;
use songbird::{
    Event, EventContext, EventHandler,
    model::{id::UserId, payload::ClientDisconnect},
};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum CallEvent {
    DriverDisconnect,
    ClientDisconnect(UserId),
}

#[derive(Clone)]
pub struct CallEventHandler {
    tx: Sender<CallEvent>,
}

impl CallEventHandler {
    pub fn create() -> (Self, Receiver<CallEvent>) {
        let (tx, rx) = tokio::sync::mpsc::channel(2);

        (Self { tx }, rx)
    }
}

#[async_trait]
impl EventHandler for CallEventHandler {
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<Event> {
        if self.tx.is_closed() {
            return Some(Event::Cancel);
        }

        match ctx {
            EventContext::DriverDisconnect(_) => {
                _ = self.tx.send(CallEvent::DriverDisconnect).await;
            }

            EventContext::ClientDisconnect(ClientDisconnect { user_id }) => {
                _ = self.tx.send(CallEvent::ClientDisconnect(*user_id)).await;
            }

            _ => {}
        }

        None
    }
}
