use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    hass::{HassHandle, HassMessage},
    model::EventData,
};

#[trait_variant::make(Automation: Send)]
pub trait LocalAutomation {
    type AutomationMessage: Send;

    fn init(&mut self);

    async fn handle_event(&mut self, event: EventData);

    async fn handle_message(&mut self, message: Self::AutomationMessage);

    fn get_message_tx(&self) -> mpsc::Sender<Self::AutomationMessage>;

    fn get_hass(&self) -> HassHandle;
}

struct AutomagicAutomation<T>
where
    T: Automation + Send,
{
    automation: T,
    event_rx: broadcast::Receiver<EventData>,
    message_rx: mpsc::Receiver<T::AutomationMessage>,
}

impl<T: Automation + Send> AutomagicAutomation<T> {
    fn new(
        automation: T,
        event_rx: broadcast::Receiver<EventData>,
        message_rx: mpsc::Receiver<T::AutomationMessage>,
    ) -> Self {
        Self {
            automation,
            event_rx,
            message_rx,
        }
    }

    async fn run(&mut self) {
        self.automation.init();
        loop {
            tokio::select! {
                Ok(event) = self.event_rx.recv() => {
                    self.automation.handle_event(event).await
                }
                Some(message) = self.message_rx.recv() => {
                    self.automation.handle_message(message).await
                }
            }
        }
    }
}

pub async fn new<T: Automation + Send + 'static>(
    automation: T,
    message_rx: mpsc::Receiver<T::AutomationMessage>,
) -> JoinHandle<()> {
    let (tx, rx) = oneshot::channel();
    let cmd = HassMessage::SubscribeEvents { tx };
    let automagic = automation.get_hass();
    automagic
        .send(cmd)
        .await
        .expect("failed to send to automagic");
    let event_rx = rx.await.expect("failed to receive event receiver");

    let mut auto = AutomagicAutomation::new(automation, event_rx, message_rx);
    tokio::spawn(async move { auto.run().await })
}
